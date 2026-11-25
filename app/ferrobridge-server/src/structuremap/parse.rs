// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The FHIR Mapping Language parser, by the R4 grammar `mapping.g4`.
//!
//! The grammar leaves its `fhirPath` production a placeholder and the
//! language page embeds FHIRPath after `default`, `where`, `check` and `log`
//! and in the `(expression)` shorthand of `evaluate` (FHIR R4 §7.7.0.7). An
//! embedded expression is either parenthesized, its parentheses balanced, or
//! runs to the first `,`, `;`, `->` or clause keyword outside a string or a
//! bracket, which is how the tutorial writes it. Comments are `//` to the end
//! of the line and `/* */` (§7.7.0.2, `mapping.g4` COMMENT).

use super::syntax::{
    Dependent, Expression, Group, InputMode, Invocation, Literal, ModelMode, Param, Parameter,
    Rule, Source, SourceListMode, StructureMap, Target, TargetListMode, Transform, TypeMode, Uses,
};

/// Why a map does not parse, with the byte offset of the failure.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("the map does not parse at byte {at}: {expected}")]
pub struct ParseError {
    /// The byte offset in the map text.
    pub at: usize,
    /// What the grammar expected there.
    pub expected: &'static str,
}

/// One token.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Token {
    /// An `IDENTIFIER`.
    Ident(String),
    /// A `DELIMITEDIDENTIFIER`, unquoted.
    Delimited(String),
    /// A `STRING`, unescaped.
    Str(String),
    /// An `INTEGER`.
    Int(String),
    /// A `NUMBER` with a fraction.
    Number(String),
    /// A `DATETIME`, without its `@`.
    DateTime(String),
    /// A `TIME`, without its `@T`.
    Time(String),
    /// A punctuation mark.
    Punct(&'static str),
    /// The end of the text.
    End,
}

/// The punctuation the grammar uses, longest first.
const PUNCTUATION: [&str; 14] = [
    "->", "<<", ">>", "..", "{", "}", "(", ")", ",", ";", ":", ".", "=", "*",
];

/// The words that end an unparenthesized embedded expression.
const CLAUSE_WORDS: [&str; 10] = [
    "as",
    "where",
    "check",
    "log",
    "then",
    "first",
    "not_first",
    "last",
    "not_last",
    "only_one",
];

/// The parser's position in the map text.
struct Cursor<'a> {
    text: &'a str,
    at: usize,
}

impl<'a> Cursor<'a> {
    /// Returns the rest of the text.
    fn rest(&self) -> &'a str {
        self.text.get(self.at..).unwrap_or_default()
    }

    /// Skips whitespace and comments.
    fn skip(&mut self) {
        loop {
            let rest = self.rest();
            let trimmed = rest.trim_start();
            self.at += rest.len() - trimmed.len();
            if let Some(after) = trimmed.strip_prefix("//") {
                let line = after.find('\n').map_or(after.len(), |end| end + 1);
                self.at += 2 + line;
            } else if let Some(after) = trimmed.strip_prefix("/*") {
                let end = after.find("*/").map_or(after.len(), |end| end + 2);
                self.at += 2 + end;
            } else {
                return;
            }
        }
    }

    /// Fails at the current position.
    fn fail<T>(&self, expected: &'static str) -> Result<T, ParseError> {
        Err(ParseError {
            at: self.at,
            expected,
        })
    }

    /// Reads the next token, moving past it.
    fn next(&mut self) -> Result<Token, ParseError> {
        self.skip();
        let rest = self.rest();
        let Some(first) = rest.chars().next() else {
            return Ok(Token::End);
        };
        if first == '"' || first == '`' {
            let (text, length) = quoted(rest, first).ok_or(ParseError {
                at: self.at,
                expected: "a closing quote",
            })?;
            self.at += length;
            return Ok(Token::Delimited(text));
        }
        if first == '\'' {
            let (text, length) = quoted(rest, '\'').ok_or(ParseError {
                at: self.at,
                expected: "a closing quote",
            })?;
            self.at += length;
            return Ok(Token::Str(text));
        }
        if first == '@' {
            let body: String = rest
                .chars()
                .skip(1)
                .take_while(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | ':' | '.' | '+'))
                .collect();
            self.at += 1 + body.len();
            return Ok(match body.strip_prefix('T') {
                Some(time) => Token::Time(time.to_owned()),
                None => Token::DateTime(body),
            });
        }
        if first.is_ascii_digit() {
            let digits = rest.chars().take_while(char::is_ascii_digit).count();
            let after = rest.get(digits..).unwrap_or_default();
            let fraction = after
                .strip_prefix('.')
                .filter(|tail| tail.starts_with(|c: char| c.is_ascii_digit()))
                .map_or(0, |tail| {
                    1 + tail.chars().take_while(char::is_ascii_digit).count()
                });
            let lexeme = rest.get(..digits + fraction).unwrap_or_default().to_owned();
            self.at += digits + fraction;
            return Ok(if fraction == 0 {
                Token::Int(lexeme)
            } else {
                Token::Number(lexeme)
            });
        }
        if first.is_ascii_alphabetic() || first == '_' {
            let word: String = rest
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '+')
                .collect();
            let word = if word == "type+" {
                word
            } else {
                word.trim_end_matches('+').to_owned()
            };
            self.at += word.len();
            return Ok(Token::Ident(word));
        }
        for mark in PUNCTUATION {
            if rest.starts_with(mark) {
                self.at += mark.len();
                return Ok(Token::Punct(mark));
            }
        }
        self.fail("a token of the mapping language")
    }

    /// Returns the next token without moving past it.
    fn peek(&mut self) -> Result<Token, ParseError> {
        let at = self.at;
        let token = self.next();
        self.at = at;
        token
    }

    /// Moves past the punctuation `mark`, or fails.
    fn expect(&mut self, mark: &'static str, expected: &'static str) -> Result<(), ParseError> {
        match self.next()? {
            Token::Punct(found) if found == mark => Ok(()),
            _ => self.fail(expected),
        }
    }

    /// Moves past `mark` when it comes next.
    fn eat(&mut self, mark: &'static str) -> Result<bool, ParseError> {
        if self.peek()? == Token::Punct(mark) {
            self.at = self.after_peek();
            return Ok(true);
        }
        Ok(false)
    }

    /// Moves past the keyword `word` when it comes next.
    fn eat_word(&mut self, word: &str) -> Result<bool, ParseError> {
        if matches!(self.peek()?, Token::Ident(found) if found == word) {
            self.at = self.after_peek();
            return Ok(true);
        }
        Ok(false)
    }

    /// Returns the position after the next token.
    fn after_peek(&mut self) -> usize {
        let at = self.at;
        // NOTE: the peek that preceded this call already read the token without error.
        let _token = self.next();
        let after = self.at;
        self.at = at;
        after
    }

    /// Reads an `identifier`: an `IDENTIFIER` or a `DELIMITEDIDENTIFIER`.
    fn identifier(&mut self, expected: &'static str) -> Result<String, ParseError> {
        match self.next()? {
            Token::Ident(name) | Token::Delimited(name) => Ok(name),
            _ => self.fail(expected),
        }
    }

    /// Reads an embedded FHIRPath expression.
    fn expression(&mut self) -> Result<Expression, ParseError> {
        self.skip();
        let rest = self.rest();
        if rest.starts_with('(') {
            let length = balanced(rest).ok_or(ParseError {
                at: self.at,
                expected: "a closing parenthesis",
            })?;
            let inner = rest
                .get(1..length - 1)
                .unwrap_or_default()
                .trim()
                .to_owned();
            self.at += length;
            return Ok(Expression(inner));
        }
        let length = unparenthesized(rest);
        if length == 0 {
            return self.fail("a FHIRPath expression");
        }
        let text = rest.get(..length).unwrap_or_default().trim().to_owned();
        self.at += length;
        Ok(Expression(text))
    }
}

/// Returns the unescaped text of the quoted token at the start of `text` and
/// the bytes it spans (`mapping.g4` ESC).
fn quoted(text: &str, quote: char) -> Option<(String, usize)> {
    let mut out = String::new();
    let mut chars = text.char_indices().skip(1);
    while let Some((index, c)) = chars.next() {
        if c == quote {
            return Some((out, index + c.len_utf8()));
        }
        if c == '\\' {
            let (_, escaped) = chars.next()?;
            out.push(match escaped {
                'n' => '\n',
                't' => '\t',
                'r' => '\r',
                'f' => '\u{c}',
                'u' => {
                    let hex: String = (0..4)
                        .filter_map(|_| chars.next().map(|(_, c)| c))
                        .collect();
                    char::from_u32(u32::from_str_radix(&hex, 16).ok()?)?
                }
                other => other,
            });
        } else {
            out.push(c);
        }
    }
    None
}

/// Returns the bytes of the parenthesized group at the start of `text`,
/// skipping quoted text inside it.
fn balanced(text: &str) -> Option<usize> {
    let mut depth = 0usize;
    let mut chars = text.char_indices();
    while let Some((index, c)) = chars.next() {
        match c {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    return Some(index + 1);
                }
            }
            '\'' | '"' | '`' => {
                let (_, length) = quoted(text.get(index..)?, c)?;
                for _ in 1..text.get(index..index + length)?.chars().count() {
                    chars.next();
                }
            }
            _ => {}
        }
    }
    None
}

/// Returns the bytes of the unparenthesized expression at the start of
/// `text`: up to a `,`, `;`, `->` or clause word at depth zero.
fn unparenthesized(text: &str) -> usize {
    let mut depth = 0usize;
    let mut index = 0usize;
    let bytes = text.as_bytes();
    while index < bytes.len() {
        let rest = text.get(index..).unwrap_or_default();
        let Some(c) = rest.chars().next() else {
            break;
        };
        if depth == 0 && (c == ',' || c == ';' || rest.starts_with("->")) {
            break;
        }
        if depth == 0 && (c.is_ascii_alphabetic() || c == '_') {
            let previous = text.get(..index).and_then(|head| head.chars().last());
            let boundary =
                previous.is_none_or(|p| !(p.is_ascii_alphanumeric() || p == '_' || p == '.'));
            let word: String = rest
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                .collect();
            if boundary && CLAUSE_WORDS.contains(&word.as_str()) {
                break;
            }
            index += word.len();
            continue;
        }
        match c {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => {
                if depth == 0 {
                    break;
                }
                depth -= 1;
            }
            '\'' | '"' | '`' => {
                if let Some((_, length)) = quoted(rest, c) {
                    index += length;
                    continue;
                }
            }
            _ => {}
        }
        index += c.len_utf8();
    }
    index
}

/// Parses `text` as one map.
///
/// # Errors
/// Returns [`ParseError`] at the first place the text leaves the grammar.
pub fn parse(text: &str) -> Result<StructureMap, ParseError> {
    let mut cursor = Cursor { text, at: 0 };
    if !cursor.eat_word("map")? {
        return cursor.fail("`map`");
    }
    let url = read_url(&mut cursor)?;
    cursor.expect("=", "`=` after the map URL")?;
    let name = cursor.identifier("the map name")?;
    let mut uses = Vec::new();
    while cursor.eat_word("uses")? {
        let url = read_url(&mut cursor)?;
        let alias = if cursor.eat_word("alias")? {
            Some(cursor.identifier("the structure alias")?)
        } else {
            None
        };
        if !cursor.eat_word("as")? {
            return cursor.fail("`as` and the model mode");
        }
        let mode = match cursor.identifier("the model mode")?.as_str() {
            "source" => ModelMode::Source,
            "queried" => ModelMode::Queried,
            "target" => ModelMode::Target,
            "produced" => ModelMode::Produced,
            _ => return cursor.fail("source, queried, target or produced"),
        };
        uses.push(Uses { url, alias, mode });
    }
    let mut imports = Vec::new();
    while cursor.eat_word("imports")? {
        imports.push(read_url(&mut cursor)?);
    }
    let mut groups = Vec::new();
    while cursor.eat_word("group")? {
        groups.push(group(&mut cursor)?);
    }
    if groups.is_empty() {
        return cursor.fail("at least one group");
    }
    if cursor.next()? != Token::End {
        return cursor.fail("the end of the map");
    }
    Ok(StructureMap {
        url,
        name,
        uses,
        imports,
        groups,
    })
}

/// Reads a `url`: a `DELIMITEDIDENTIFIER`.
fn read_url(cursor: &mut Cursor<'_>) -> Result<String, ParseError> {
    match cursor.next()? {
        Token::Delimited(url) => Ok(url),
        _ => cursor.fail("a quoted URL"),
    }
}

/// Reads a group after its `group` keyword.
fn group(cursor: &mut Cursor<'_>) -> Result<Group, ParseError> {
    let name = cursor.identifier("the group name")?;
    cursor.expect("(", "`(` and the group parameters")?;
    let mut parameters = Vec::new();
    loop {
        let mode = match cursor.identifier("source or target")?.as_str() {
            "source" => InputMode::Source,
            "target" => InputMode::Target,
            _ => return cursor.fail("source or target"),
        };
        let name = cursor.identifier("the parameter name")?;
        let type_name = if cursor.eat(":")? {
            Some(cursor.identifier("the parameter type")?)
        } else {
            None
        };
        parameters.push(Parameter {
            mode,
            name,
            type_name,
        });
        if !cursor.eat(",")? {
            break;
        }
    }
    cursor.expect(")", "`)` after the group parameters")?;
    let extends = if cursor.eat_word("extends")? {
        Some(cursor.identifier("the extended group")?)
    } else {
        None
    };
    let type_mode = if cursor.eat("<<")? {
        let mode = match cursor.identifier("types or type+")?.as_str() {
            "types" => TypeMode::Types,
            "type+" => TypeMode::TypePlus,
            _ => return cursor.fail("types or type+"),
        };
        cursor.expect(">>", "`>>`")?;
        Some(mode)
    } else {
        None
    };
    let rules = rules(cursor)?;
    Ok(Group {
        name,
        parameters,
        extends,
        type_mode,
        rules,
    })
}

/// Reads `{ rule+ }`.
fn rules(cursor: &mut Cursor<'_>) -> Result<Vec<Rule>, ParseError> {
    cursor.expect("{", "`{` and the rules")?;
    let mut rules = Vec::new();
    while !cursor.eat("}")? {
        rules.push(rule(cursor)?);
    }
    if rules.is_empty() {
        return cursor.fail("at least one rule");
    }
    Ok(rules)
}

/// Reads one rule, through its `;`.
fn rule(cursor: &mut Cursor<'_>) -> Result<Rule, ParseError> {
    let mut sources = vec![source(cursor)?];
    while cursor.eat(",")? {
        sources.push(source(cursor)?);
    }
    let mut targets = Vec::new();
    if cursor.eat("->")? {
        targets.push(target(cursor)?);
        while cursor.eat(",")? {
            targets.push(target(cursor)?);
        }
    }
    let dependent = if cursor.eat_word("then")? {
        if cursor.peek()? == Token::Punct("{") {
            Some(Dependent::Rules(rules(cursor)?))
        } else {
            let name = cursor.identifier("a group or `{`")?;
            Some(Dependent::Invocation(invocation(cursor, name)?))
        }
    } else {
        None
    };
    let name = match cursor.peek()? {
        Token::Delimited(_) => Some(cursor.identifier("the rule name")?),
        _ => None,
    };
    cursor.expect(";", "`;` at the end of the rule")?;
    Ok(Rule {
        name,
        sources,
        targets,
        dependent,
    })
}

/// Reads a `ruleContext`: `identifier ('.' identifier)?`.
fn context(cursor: &mut Cursor<'_>) -> Result<(String, Option<String>), ParseError> {
    let variable = cursor.identifier("a variable")?;
    let element = if cursor.eat(".")? {
        Some(cursor.identifier("an element name")?)
    } else {
        None
    };
    Ok((variable, element))
}

/// Reads one source statement.
fn source(cursor: &mut Cursor<'_>) -> Result<Source, ParseError> {
    let (context, element) = context(cursor)?;
    let mut source = Source {
        context,
        element,
        type_name: None,
        cardinality: None,
        default: None,
        list_mode: None,
        variable: None,
        condition: None,
        check: None,
        log: None,
    };
    if cursor.eat(":")? {
        source.type_name = Some(cursor.identifier("the source type")?);
        if let Token::Int(min) = cursor.peek()? {
            cursor.at = cursor.after_peek();
            cursor.expect("..", "`..` in the cardinality")?;
            let max = match cursor.next()? {
                Token::Int(max) => Some(number(cursor, &max)?),
                Token::Punct("*") => None,
                _ => return cursor.fail("an upper bound"),
            };
            source.cardinality = Some((number(cursor, &min)?, max));
        }
    }
    if cursor.eat_word("default")? {
        source.default = Some(cursor.expression()?);
    }
    source.list_mode = match cursor.peek()? {
        Token::Ident(word) => match word.as_str() {
            "first" => Some(SourceListMode::First),
            "not_first" => Some(SourceListMode::NotFirst),
            "last" => Some(SourceListMode::Last),
            "not_last" => Some(SourceListMode::NotLast),
            "only_one" => Some(SourceListMode::OnlyOne),
            _ => None,
        },
        _ => None,
    };
    if source.list_mode.is_some() {
        cursor.at = cursor.after_peek();
    }
    if cursor.eat_word("as")? {
        source.variable = Some(cursor.identifier("the source variable")?);
    }
    if cursor.eat_word("where")? {
        source.condition = Some(cursor.expression()?);
    }
    if cursor.eat_word("check")? {
        source.check = Some(cursor.expression()?);
    }
    if cursor.eat_word("log")? {
        source.log = Some(cursor.expression()?);
    }
    Ok(source)
}

/// Returns the cardinality bound `text` writes.
fn number(cursor: &Cursor<'_>, text: &str) -> Result<u32, ParseError> {
    text.parse()
        .or_else(|_too_big| cursor.fail("a cardinality bound"))
}

/// Reads one target statement.
fn target(cursor: &mut Cursor<'_>) -> Result<Target, ParseError> {
    let (context, element) = context(cursor)?;
    if element.is_none() && cursor.peek()? == Token::Punct("(") {
        let invocation = invocation(cursor, context)?;
        let variable = alias(cursor)?;
        return Ok(Target {
            context: None,
            element: None,
            transform: Some(Transform::Invocation(invocation)),
            variable,
            list_mode: None,
        });
    }
    let transform = if cursor.eat("=")? {
        Some(transform(cursor)?)
    } else {
        None
    };
    let variable = alias(cursor)?;
    let list_mode = match cursor.peek()? {
        Token::Ident(word) => match word.as_str() {
            "first" => Some(TargetListMode::First),
            "share" => Some(TargetListMode::Share),
            "last" => Some(TargetListMode::Last),
            "collate" => Some(TargetListMode::Collate),
            _ => None,
        },
        _ => None,
    };
    if list_mode.is_some() {
        cursor.at = cursor.after_peek();
    }
    Ok(Target {
        context: Some(context),
        element,
        transform,
        variable,
        list_mode,
    })
}

/// Reads an optional `as identifier`.
fn alias(cursor: &mut Cursor<'_>) -> Result<Option<String>, ParseError> {
    if cursor.eat_word("as")? {
        return Ok(Some(cursor.identifier("the target variable")?));
    }
    Ok(None)
}

/// Reads a `transform`: a literal, a copy, an invocation or `(expression)`.
fn transform(cursor: &mut Cursor<'_>) -> Result<Transform, ParseError> {
    cursor.skip();
    if cursor.rest().starts_with('(') {
        return Ok(Transform::Evaluate(cursor.expression()?));
    }
    match cursor.peek()? {
        Token::Ident(word) if word == "true" || word == "false" => {
            cursor.at = cursor.after_peek();
            Ok(Transform::Literal(Literal::Boolean(word == "true")))
        }
        Token::Ident(_) | Token::Delimited(_) => {
            let (variable, element) = context(cursor)?;
            if element.is_none() && cursor.peek()? == Token::Punct("(") {
                return Ok(Transform::Invocation(invocation(cursor, variable)?));
            }
            Ok(Transform::Copy {
                context: variable,
                element,
            })
        }
        _ => Ok(Transform::Literal(literal(cursor)?)),
    }
}

/// Reads a literal.
fn literal(cursor: &mut Cursor<'_>) -> Result<Literal, ParseError> {
    match cursor.next()? {
        Token::Int(text) => Ok(Literal::Integer(text)),
        Token::Number(text) => Ok(Literal::Decimal(text)),
        Token::Str(text) => Ok(Literal::String(text)),
        Token::DateTime(text) => Ok(Literal::DateTime(text)),
        Token::Time(text) => Ok(Literal::Time(text)),
        Token::Ident(word) if word == "true" => Ok(Literal::Boolean(true)),
        Token::Ident(word) if word == "false" => Ok(Literal::Boolean(false)),
        _ => cursor.fail("a literal"),
    }
}

/// Reads the parameter list of the invocation named `name`.
fn invocation(cursor: &mut Cursor<'_>, name: String) -> Result<Invocation, ParseError> {
    cursor.expect("(", "`(` and the parameters")?;
    let mut params = Vec::new();
    if !cursor.eat(")")? {
        loop {
            let param = match cursor.peek()? {
                Token::Ident(word) if word != "true" && word != "false" => {
                    Param::Variable(cursor.identifier("a variable")?)
                }
                Token::Delimited(word) => {
                    cursor.at = cursor.after_peek();
                    Param::Literal(Literal::String(word))
                }
                _ => Param::Literal(literal(cursor)?),
            };
            params.push(param);
            if !cursor.eat(",")? {
                break;
            }
        }
        cursor.expect(")", "`)` after the parameters")?;
    }
    Ok(Invocation { name, params })
}
