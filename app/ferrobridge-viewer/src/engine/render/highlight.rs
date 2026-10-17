// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The syntax highlighting of a mapping file's YAML, done on the server.
//!
//! Each line is tokenized on its own, in one pass over its characters, so a
//! multi-byte character is never split, and the texts of a line's tokens
//! joined together are the line exactly: text the tokenizer does not
//! classify is a [`Kind::Plain`] token, never dropped. The classes are
//! decoration over text that reads the same without them (YAML 1.2.2, the
//! block mapping key, the flow scalars, comments, anchors, aliases and
//! tags). No specification governs the highlighting: our own design.

/// The class of one token.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// Whitespace, and text the tokenizer does not classify.
    Plain,
    /// A block mapping key.
    Key,
    /// A quoted scalar.
    String,
    /// A plain scalar that reads as a number.
    Number,
    /// `true`, `false`, `null`, `~`, an anchor, an alias or a tag.
    Keyword,
    /// A comment, from `#` to the end of the line.
    Comment,
    /// The `:` of a key and the `-` of a sequence entry.
    Punctuation,
}

impl Kind {
    /// The class the stylesheet paints a token of this kind with, or `None`
    /// for text drawn as it stands.
    #[must_use]
    pub const fn class(self) -> Option<&'static str> {
        match self {
            Self::Plain | Self::Punctuation => None,
            Self::Key => Some("tok-key"),
            Self::String => Some("tok-string"),
            Self::Number => Some("tok-number"),
            Self::Keyword => Some("tok-keyword"),
            Self::Comment => Some("tok-comment"),
        }
    }
}

/// One token: its kind and the exact text of the line it covers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    /// What the text is.
    pub kind: Kind,
    /// The text, as the line has it.
    pub text: String,
}

/// The tokens of the YAML line `line`, which carries no line break.
#[must_use]
pub fn line(line: &str) -> Vec<Token> {
    let mut tokens = Vec::new();
    let mut rest = line;
    let indent = rest.len() - rest.trim_start_matches([' ', '\t']).len();
    rest = take(&mut tokens, Kind::Plain, rest, indent);
    while let Some(after) = rest.strip_prefix('-')
        && (after.is_empty() || after.starts_with([' ', '\t']))
    {
        rest = take(&mut tokens, Kind::Punctuation, rest, 1);
        let space = rest.len() - rest.trim_start_matches([' ', '\t']).len();
        rest = take(&mut tokens, Kind::Plain, rest, space);
    }
    if let Some(key) = key_length(rest) {
        rest = take(&mut tokens, Kind::Key, rest, key);
        rest = take(&mut tokens, Kind::Punctuation, rest, 1);
    }
    value(&mut tokens, rest);
    tokens
}

/// Pushes the first `bytes` of `text` as a token of `kind`, and returns what
/// follows; an empty token is not pushed.
fn take<'a>(tokens: &mut Vec<Token>, kind: Kind, text: &'a str, bytes: usize) -> &'a str {
    let (head, tail) = text.split_at_checked(bytes).unwrap_or((text, ""));
    if !head.is_empty() {
        tokens.push(Token {
            kind,
            text: head.to_owned(),
        });
    }
    tail
}

/// The length of the block mapping key `text` opens with, when it opens
/// with one: a quoted or plain scalar followed by `:` and a space or the
/// end of the line.
fn key_length(text: &str) -> Option<usize> {
    if text.starts_with('#') {
        return None;
    }
    let first = text.chars().next()?;
    let end = if first == '"' || first == '\'' {
        quoted_length(text, first)?
    } else {
        plain_key_length(text)?
    };
    let after = text.get(end..)?;
    let next = after.strip_prefix(':')?;
    (next.is_empty() || next.starts_with([' ', '\t'])).then_some(end)
}

/// The length of the plain key `text` opens with: up to the first `:` that
/// a space or the end of the line follows, unless a comment starts first.
fn plain_key_length(text: &str) -> Option<usize> {
    for (index, character) in text.char_indices() {
        if character == ':' {
            let next = text.get(index + 1..).unwrap_or_default();
            if next.is_empty() || next.starts_with([' ', '\t']) {
                return Some(index);
            }
        }
        if character == '#'
            && text
                .get(..index)
                .is_some_and(|before| before.ends_with(' '))
        {
            return None;
        }
    }
    None
}

/// The length of the quoted scalar `text` opens with, its closing quote
/// included, or `None` when the line ends inside it.
fn quoted_length(text: &str, quote: char) -> Option<usize> {
    let mut characters = text.char_indices().skip(1).peekable();
    while let Some((index, character)) = characters.next() {
        if quote == '"' && character == '\\' {
            characters.next();
            continue;
        }
        if character == quote {
            if quote == '\'' && characters.peek().is_some_and(|(_, next)| *next == '\'') {
                characters.next();
                continue;
            }
            return Some(index + character.len_utf8());
        }
    }
    None
}

/// Pushes the tokens of the value part of a line: scalars, anchors, aliases,
/// tags, a comment, and the whitespace between them.
fn value(tokens: &mut Vec<Token>, mut rest: &str) {
    while !rest.is_empty() {
        let space = rest.len() - rest.trim_start_matches([' ', '\t']).len();
        if space > 0 {
            rest = take(tokens, Kind::Plain, rest, space);
            continue;
        }
        let first = rest.chars().next().unwrap_or(' ');
        if first == '#' {
            rest = take(tokens, Kind::Comment, rest, rest.len());
            continue;
        }
        if first == '"' || first == '\'' {
            let length = quoted_length(rest, first).unwrap_or(rest.len());
            rest = take(tokens, Kind::String, rest, length);
            continue;
        }
        if matches!(first, '&' | '*' | '!') {
            let length = rest.find([' ', '\t']).unwrap_or(rest.len());
            rest = take(tokens, Kind::Keyword, rest, length);
            continue;
        }
        let length = scalar_length(rest);
        let (scalar, _) = rest.split_at_checked(length).unwrap_or((rest, ""));
        let kind = classify(scalar.trim_end());
        rest = take(tokens, kind, rest, length);
    }
}

/// The length of the plain scalar `text` opens with: up to a comment, which
/// a `#` after whitespace starts, or the end of the line.
fn scalar_length(text: &str) -> usize {
    let mut previous = ' ';
    for (index, character) in text.char_indices() {
        if character == '#' && index > 0 && matches!(previous, ' ' | '\t') {
            let head = text.get(..index).unwrap_or(text);
            return head.trim_end_matches([' ', '\t']).len();
        }
        previous = character;
    }
    text.len()
}

/// The kind of the plain scalar `scalar`.
fn classify(scalar: &str) -> Kind {
    match scalar {
        "true" | "false" | "null" | "~" | "True" | "False" | "Null" | "TRUE" | "FALSE" | "NULL" => {
            Kind::Keyword
        }
        _ if is_number(scalar) => Kind::Number,
        _ => Kind::Plain,
    }
}

/// Whether `scalar` reads as a YAML 1.2 core schema integer or float.
fn is_number(scalar: &str) -> bool {
    let digits = scalar.strip_prefix(['-', '+']).unwrap_or(scalar);
    !digits.is_empty()
        && digits.chars().any(|character| character.is_ascii_digit())
        && digits.chars().all(|character| {
            character.is_ascii_digit() || matches!(character, '.' | 'e' | 'E' | '-' | '+')
        })
        && digits
            .chars()
            .next()
            .is_some_and(|first| first.is_ascii_digit() || first == '.')
}

#[cfg(test)]
mod tests {
    use super::{Kind, Token, line};

    /// The texts of `tokens`, joined.
    fn joined(tokens: &[Token]) -> String {
        tokens.iter().map(|token| token.text.as_str()).collect()
    }

    /// The kinds and texts of `tokens` that carry a class.
    fn classed(tokens: &[Token]) -> Vec<(Kind, &str)> {
        tokens
            .iter()
            .filter(|token| token.kind.class().is_some())
            .map(|token| (token.kind, token.text.as_str()))
            .collect()
    }

    #[test]
    fn every_line_reads_back_as_itself() {
        for text in [
            "",
            "   ",
            "grammar: FHIRConnect/v1.0.0",
            "  - name: \"a: b\" # a comment",
            "- - x",
            "key: 'it''s' # c",
            "url: http://example.org/a#b",
            "count: -12.5e3",
            "anchor: &base",
            "<<: *base",
            "tag: !!str 7",
            "naïve: ünïcode # ü",
            "\"quoted key\": value",
            "unterminated: \"open",
            "# a whole-line comment",
            "\ttabbed: true",
        ] {
            assert_eq!(text, joined(&line(text)), "{text:?}");
        }
    }

    #[test]
    fn keys_scalars_comments_anchors_and_tags_are_classed() {
        assert_eq!(
            vec![
                (Kind::Key, "name"),
                (Kind::String, "\"a: b\""),
                (Kind::Comment, "# a comment")
            ],
            classed(&line("  - name: \"a: b\" # a comment"))
        );
        assert_eq!(
            vec![(Kind::Key, "count"), (Kind::Number, "-12.5e3")],
            classed(&line("count: -12.5e3"))
        );
        assert_eq!(
            vec![(Kind::Key, "enabled"), (Kind::Keyword, "true")],
            classed(&line("enabled: true"))
        );
        assert_eq!(
            vec![(Kind::Key, "url")],
            classed(&line("url: http://example.org/a#b")),
            "a # inside a scalar is no comment"
        );
        assert_eq!(
            vec![
                (Kind::Key, "tag"),
                (Kind::Keyword, "!!str"),
                (Kind::Number, "7")
            ],
            classed(&line("tag: !!str 7"))
        );
        assert_eq!(
            vec![(Kind::Comment, "# a whole-line comment")],
            classed(&line("# a whole-line comment"))
        );
    }
}
