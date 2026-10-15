// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The parser of the N1 grammar.
//!
//! It reads the tokens of the private `lex` module by the grammar of
//! <https://hl7.org/fhirpath/N1/grammar.html>, with the operator precedence
//! of N1 §6.8 and every binary operator associating to the left.

use std::ops::Range;

use chumsky::prelude::*;
use rust_decimal::Decimal;

use crate::ast::{BinaryOp, Expr, Literal, TypeSpecifier};
use crate::lex::{Token, lex};
use crate::quantity::Unit;
use crate::temporal::parse::{Temporal, literal};

/// Why a text is no FHIRPath expression.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ParseError {
    /// Text no token of the grammar matches, such as an unterminated string
    /// or comment.
    #[error("no token of the grammar matches {text:?} at byte {offset}")]
    Lex {
        /// The byte offset.
        offset: usize,
        /// The text no token matches.
        text: String,
    },
    /// Tokens the grammar does not admit at that place.
    #[error("the expression leaves the grammar at byte {offset}")]
    Syntax {
        /// The byte offset of the first token not admitted, the length of the
        /// text when the expression ends too early.
        offset: usize,
    },
    /// A literal the grammar admits but whose value does not exist, such as
    /// `@2015-02-30` or an integer past 2^31-1.
    #[error("the literal {text:?} at byte {offset} names no value")]
    Literal {
        /// The byte offset.
        offset: usize,
        /// The literal as written.
        text: String,
    },
}

/// The chumsky extra parameter of this grammar.
#[expect(
    unused_qualifications,
    reason = "the alias shadows the name it qualifies, so the path keeps the definition from naming itself"
)]
type Extra<'t> = chumsky::extra::Err<Simple<'t, Token>>;

/// Parses `text` into a syntax tree.
///
/// # Errors
///
/// Returns [`ParseError::Lex`] for text no token matches,
/// [`ParseError::Syntax`] for tokens outside the grammar and
/// [`ParseError::Literal`] for a literal that names no value.
pub fn parse(text: &str) -> Result<Expr, ParseError> {
    let spanned = lex(text)?;
    for (token, span) in &spanned {
        check_literal(token, span)?;
    }
    let tokens: Vec<Token> = spanned.iter().map(|(token, _)| token.clone()).collect();
    grammar()
        .parse(tokens.as_slice())
        .into_result()
        .map_err(|errors| {
            let index = errors
                .first()
                .map_or(tokens.len(), |error| error.span().start);
            ParseError::Syntax {
                offset: spanned
                    .get(index)
                    .map_or(text.len(), |(_, span)| span.start),
            }
        })
}

/// Refuses a number or a date and time literal that names no value.
fn check_literal(token: &Token, span: &Range<usize>) -> Result<(), ParseError> {
    let refuse = |text: &str| ParseError::Literal {
        offset: span.start,
        text: String::from(text),
    };
    match token {
        Token::Temporal(text) if literal(text).is_none() => Err(refuse(text)),
        Token::Number(text) if number(text).is_none() => Err(refuse(text)),
        _ => Ok(()),
    }
}

/// The literal a number token stands for: an integer within 32 bits (N1
/// §4.1.3), or a decimal.
fn number(text: &str) -> Option<Literal> {
    if text.contains('.') {
        text.parse::<Decimal>().ok().map(Literal::Decimal)
    } else {
        text.parse::<i32>()
            .ok()
            .map(|value| Literal::Integer(i64::from(value)))
    }
}

/// The literal a date or time token stands for.
fn temporal(text: &str) -> Option<Literal> {
    Some(match literal(text)? {
        Temporal::Date(value) => Literal::Date(value),
        Temporal::DateTime(value) => Literal::DateTime(value),
        Temporal::Time(value) => Literal::Time(value),
    })
}

/// A left-associative level of binary operators over `operand`.
fn level<'t>(
    operand: impl Parser<'t, &'t [Token], Expr, Extra<'t>> + Clone + 't,
    operators: impl Parser<'t, &'t [Token], BinaryOp, Extra<'t>> + Clone + 't,
) -> impl Parser<'t, &'t [Token], Expr, Extra<'t>> + Clone {
    operand
        .clone()
        .foldl(operators.then(operand).repeated(), |left, (op, right)| {
            Expr::Binary {
                op,
                left: Box::new(left),
                right: Box::new(right),
            }
        })
        .boxed()
}

/// What follows a term: an invocation after `.`, or an indexer.
enum Postfix {
    /// `.invocation`.
    Invoke(Expr),
    /// `[index]`.
    Index(Expr),
}

/// `identifier`: a simple or delimited identifier, or one of the four
/// keywords the grammar admits as one.
fn identifier<'t>() -> impl Parser<'t, &'t [Token], String, Extra<'t>> + Clone {
    select! {
        Token::Identifier(name) => name,
        Token::Delimited(name) => name,
        Token::As => String::from("as"),
        Token::Contains => String::from("contains"),
        Token::In => String::from("in"),
        Token::Is => String::from("is"),
    }
}

/// `typeSpecifier`: a type name with an optional model qualifier.
fn type_specifier<'t>() -> impl Parser<'t, &'t [Token], TypeSpecifier, Extra<'t>> + Clone {
    identifier()
        .then(just(Token::Dot).ignore_then(identifier()).or_not())
        .map(|(first, second)| match second {
            Some(name) => TypeSpecifier {
                namespace: Some(first),
                name,
            },
            None => TypeSpecifier {
                namespace: None,
                name: first,
            },
        })
}

/// `literal`, quantities included.
fn literal_term<'t>() -> impl Parser<'t, &'t [Token], Expr, Extra<'t>> + Clone {
    let unit = select! {
        Token::Text(code) => Unit::from_code(code),
        Token::Calendar(calendar) => Unit::Calendar(calendar),
    };
    let numeric = select! { Token::Number(text) => text }
        .then(unit.or_not())
        .try_map(|(text, unit), span| match (number(&text), unit) {
            (Some(Literal::Integer(value)), Some(unit)) => {
                Ok(Literal::Quantity(Decimal::from(value), unit))
            }
            (Some(Literal::Decimal(value)), Some(unit)) => Ok(Literal::Quantity(value, unit)),
            (Some(literal), None) => Ok(literal),
            _ => Err(Simple::new(None, span)),
        });
    choice((
        just(Token::OpenBrace)
            .then(just(Token::CloseBrace))
            .to(Expr::Empty),
        just(Token::True).to(Expr::Literal(Literal::Boolean(true))),
        just(Token::False).to(Expr::Literal(Literal::Boolean(false))),
        select! { Token::Text(text) => Expr::Literal(Literal::String(text)) },
        select! { Token::Temporal(text) => text }.try_map(|text, span| {
            temporal(&text)
                .map(Expr::Literal)
                .ok_or_else(|| Simple::new(None, span))
        }),
        numeric.map(Expr::Literal),
    ))
}

/// `invocation`: a member, a function call, or a special variable.
fn invocation<'t>(
    expression: impl Parser<'t, &'t [Token], Expr, Extra<'t>> + Clone + 't,
) -> impl Parser<'t, &'t [Token], Expr, Extra<'t>> + Clone {
    let arguments = expression
        .separated_by(just(Token::Comma))
        .collect::<Vec<Expr>>()
        .delimited_by(just(Token::OpenParen), just(Token::CloseParen));
    choice((
        identifier()
            .then(arguments.or_not())
            .map(|(name, args)| match args {
                Some(args) => Expr::Call {
                    input: None,
                    name,
                    args,
                },
                None => Expr::Member { input: None, name },
            }),
        just(Token::This).to(Expr::This),
        just(Token::Index).to(Expr::Index),
        just(Token::Total).to(Expr::Total),
    ))
}

/// A term followed by its invocations and indexers, the tightest-binding
/// operators of N1 §6.8.
fn invoked<'t>(
    expression: impl Parser<'t, &'t [Token], Expr, Extra<'t>> + Clone + 't,
) -> impl Parser<'t, &'t [Token], Expr, Extra<'t>> + Clone {
    let external = just(Token::Percent)
        .ignore_then(choice((
            identifier(),
            select! { Token::Text(text) => text },
        )))
        .map(Expr::External);
    let term = choice((
        literal_term(),
        external,
        invocation(expression.clone()),
        expression
            .clone()
            .delimited_by(just(Token::OpenParen), just(Token::CloseParen)),
    ));
    let postfix = choice((
        just(Token::Dot)
            .ignore_then(invocation(expression.clone()))
            .map(Postfix::Invoke),
        expression
            .delimited_by(just(Token::OpenBracket), just(Token::CloseBracket))
            .map(Postfix::Index),
    ));
    term.foldl(postfix.repeated(), |input, postfix| match postfix {
        Postfix::Invoke(Expr::Member { name, .. }) => Expr::Member {
            input: Some(Box::new(input)),
            name,
        },
        Postfix::Invoke(Expr::Call { name, args, .. }) => Expr::Call {
            input: Some(Box::new(input)),
            name,
            args,
        },
        Postfix::Invoke(variable) => Expr::Invocation {
            input: Box::new(input),
            variable: Box::new(variable),
        },
        Postfix::Index(index) => Expr::Indexer {
            input: Box::new(input),
            index: Box::new(index),
        },
    })
    .boxed()
}

/// The unary and the arithmetic operators over `invoked`, up to the type
/// operators (N1 §6.8, #03 to #06).
fn arithmetic<'t>(
    invoked: impl Parser<'t, &'t [Token], Expr, Extra<'t>> + Clone + 't,
) -> impl Parser<'t, &'t [Token], Expr, Extra<'t>> + Clone {
    let polarity = choice((just(Token::Plus).to(true), just(Token::Minus).to(false)))
        .repeated()
        .foldr(invoked, |plus, operand| {
            if plus {
                Expr::Plus(Box::new(operand))
            } else {
                Expr::Negate(Box::new(operand))
            }
        })
        .boxed();
    let multiplicative = level(
        polarity,
        choice((
            just(Token::Star).to(BinaryOp::Multiply),
            just(Token::Slash).to(BinaryOp::Divide),
            just(Token::Div).to(BinaryOp::Div),
            just(Token::Mod).to(BinaryOp::Mod),
        )),
    );
    let additive = level(
        multiplicative,
        choice((
            just(Token::Plus).to(BinaryOp::Add),
            just(Token::Minus).to(BinaryOp::Subtract),
            just(Token::Ampersand).to(BinaryOp::Concatenate),
        )),
    );
    additive
        .foldl(
            choice((just(Token::Is).to(true), just(Token::As).to(false)))
                .then(type_specifier())
                .repeated(),
            |operand, (is, type_specifier)| {
                let operand = Box::new(operand);
                if is {
                    Expr::Is {
                        operand,
                        type_specifier,
                    }
                } else {
                    Expr::As {
                        operand,
                        type_specifier,
                    }
                }
            },
        )
        .boxed()
}

/// The collection, comparison and Boolean operators over `typed` (N1 §6.8,
/// #07 to #13).
fn logical<'t>(
    typed: impl Parser<'t, &'t [Token], Expr, Extra<'t>> + Clone + 't,
) -> impl Parser<'t, &'t [Token], Expr, Extra<'t>> + Clone {
    let union = level(typed, just(Token::Pipe).to(BinaryOp::Union));
    let inequality = level(
        union,
        choice((
            just(Token::LessOrEqual).to(BinaryOp::LessOrEqual),
            just(Token::Less).to(BinaryOp::Less),
            just(Token::GreaterOrEqual).to(BinaryOp::GreaterOrEqual),
            just(Token::Greater).to(BinaryOp::Greater),
        )),
    );
    let equality = level(
        inequality,
        choice((
            just(Token::Equal).to(BinaryOp::Equal),
            just(Token::Tilde).to(BinaryOp::Equivalent),
            just(Token::NotEqual).to(BinaryOp::NotEqual),
            just(Token::NotTilde).to(BinaryOp::NotEquivalent),
        )),
    );
    let membership = level(
        equality,
        choice((
            just(Token::In).to(BinaryOp::In),
            just(Token::Contains).to(BinaryOp::Contains),
        )),
    );
    let and = level(membership, just(Token::And).to(BinaryOp::And));
    let or = level(
        and,
        choice((
            just(Token::Or).to(BinaryOp::Or),
            just(Token::Xor).to(BinaryOp::Xor),
        )),
    );
    level(or, just(Token::Implies).to(BinaryOp::Implies))
}

/// The grammar.
fn grammar<'t>() -> impl Parser<'t, &'t [Token], Expr, Extra<'t>> {
    recursive(|expression| logical(arithmetic(invoked(expression)))).then_ignore(end())
}
