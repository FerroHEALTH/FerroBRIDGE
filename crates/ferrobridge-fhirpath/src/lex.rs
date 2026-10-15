// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The lexical rules of the N1 grammar (<https://hl7.org/fhirpath/N1/grammar.html>).
//!
//! Whitespace and both comment forms are skipped (N1 §8.1, §8.2). A number,
//! a date, a datetime and a time are scanned by hand after their first
//! character, so `1.convertsToInteger()` lexes as the integer `1` followed by
//! an invocation and `@2015-02-04T14:34:28.is(DateTime)` stops before `.is`,
//! which the grammar's fragments decide and a regular expression engine
//! without backtracking would not.

use std::ops::Range;

use logos::{Lexer, Logos};

use crate::parse::ParseError;
use crate::quantity::Calendar;

/// One token of the grammar.
#[derive(Logos, Debug, Clone, PartialEq, Eq)]
#[logos(skip r"[ \t\r\n]+")]
#[logos(skip(r"//[^\r\n]*", allow_greedy = true))]
#[logos(skip r"/\*([^*]|\*+[^*/])*\*+/")]
pub(crate) enum Token {
    #[token(".")]
    Dot,
    #[token("[")]
    OpenBracket,
    #[token("]")]
    CloseBracket,
    #[token("(")]
    OpenParen,
    #[token(")")]
    CloseParen,
    #[token("{")]
    OpenBrace,
    #[token("}")]
    CloseBrace,
    #[token(",")]
    Comma,
    #[token("+")]
    Plus,
    #[token("-")]
    Minus,
    #[token("*")]
    Star,
    #[token("/")]
    Slash,
    #[token("&")]
    Ampersand,
    #[token("|")]
    Pipe,
    #[token("=")]
    Equal,
    #[token("~")]
    Tilde,
    #[token("!=")]
    NotEqual,
    #[token("!~")]
    NotTilde,
    #[token("<")]
    Less,
    #[token("<=")]
    LessOrEqual,
    #[token(">")]
    Greater,
    #[token(">=")]
    GreaterOrEqual,
    #[token("%")]
    Percent,
    #[token("div")]
    Div,
    #[token("mod")]
    Mod,
    #[token("and")]
    And,
    #[token("or")]
    Or,
    #[token("xor")]
    Xor,
    #[token("implies")]
    Implies,
    #[token("is")]
    Is,
    #[token("as")]
    As,
    #[token("in")]
    In,
    #[token("contains")]
    Contains,
    #[token("true")]
    True,
    #[token("false")]
    False,
    #[token("$this")]
    This,
    #[token("$index")]
    Index,
    #[token("$total")]
    Total,
    #[regex(
        "years?|months?|weeks?|days?|hours?|minutes?|seconds?|milliseconds?",
        |lexer| Calendar::from_keyword(lexer.slice())
    )]
    Calendar(Calendar),
    #[regex("[A-Za-z_][A-Za-z0-9_]*", |lexer| String::from(lexer.slice()))]
    Identifier(String),
    #[regex(r"`([^`\\]|\\[\x00-\u{10FFFF}])*`", |lexer| unescape(lexer.slice()))]
    Delimited(String),
    #[regex(r"'([^'\\]|\\[\x00-\u{10FFFF}])*'", |lexer| unescape(lexer.slice()))]
    Text(String),
    #[regex("[0-9]", number)]
    Number(String),
    #[token("@", temporal)]
    Temporal(String),
}

/// The text of a string or delimited identifier, its escapes resolved.
///
/// N1 §4.1.2 lists the escapes; a backslash before any other character is
/// dropped and the character kept, and `\u` without four hex digits is the
/// letter `u` (the specification's own `'\u005'` example).
fn unescape(slice: &str) -> String {
    let mut chars = slice.chars();
    chars.next();
    chars.next_back();
    let mut text = String::with_capacity(slice.len());
    while let Some(c) = chars.next() {
        if c != '\\' {
            text.push(c);
            continue;
        }
        match chars.next() {
            Some('r') => text.push('\r'),
            Some('n') => text.push('\n'),
            Some('t') => text.push('\t'),
            Some('f') => text.push('\u{c}'),
            Some('u') => {
                let hex: String = chars.clone().take(4).collect();
                match (hex.len() == 4)
                    .then(|| u32::from_str_radix(&hex, 16).ok())
                    .flatten()
                    .and_then(char::from_u32)
                {
                    Some(decoded) => {
                        text.push(decoded);
                        for _ in 0..4 {
                            chars.next();
                        }
                    }
                    None => text.push('u'),
                }
            }
            Some(other) => text.push(other),
            None => {}
        }
    }
    text
}

/// Extends a number past its first digit: more digits, then a fraction only
/// when a digit follows the point (the grammar's `NUMBER`).
fn number(lexer: &mut Lexer<'_, Token>) -> String {
    let rest = lexer.remainder().as_bytes();
    let mut length = digits(rest, 0);
    if rest.get(length) == Some(&b'.')
        && rest
            .get(length.saturating_add(1))
            .is_some_and(u8::is_ascii_digit)
    {
        length = digits(rest, length.saturating_add(1));
    }
    lexer.bump(length);
    String::from(lexer.slice())
}

/// The index after the ASCII digits of `bytes` that start at `from`.
fn digits(bytes: &[u8], from: usize) -> usize {
    let mut at = from;
    while bytes.get(at).is_some_and(u8::is_ascii_digit) {
        at = at.saturating_add(1);
    }
    at
}

/// Extends `@` over a date, a datetime or a time (the grammar's `DATE`,
/// `DATETIME` and `TIME`), returning the text after the `@`, or `None` when
/// no form follows.
fn temporal(lexer: &mut Lexer<'_, Token>) -> Option<String> {
    let rest = lexer.remainder().as_bytes();
    let length = if rest.first() == Some(&b'T') {
        time_format(rest, 1)?
    } else {
        let date = date_format(rest)?;
        if rest.get(date) == Some(&b'T') {
            let after = date.saturating_add(1);
            match time_format(rest, after) {
                Some(time) => offset(rest, time),
                None => after,
            }
        } else {
            date
        }
    };
    lexer.bump(length);
    lexer.slice().strip_prefix('@').map(String::from)
}

/// The index after `n` digits at `at`, or `None`.
fn fixed(bytes: &[u8], at: usize, n: usize) -> Option<usize> {
    let end = at.checked_add(n)?;
    bytes
        .get(at..end)
        .filter(|run| run.iter().all(u8::is_ascii_digit))
        .map(|_| end)
}

/// The index after `-dd` at `at`, or `None`.
fn dashed(bytes: &[u8], at: usize, separator: u8) -> Option<usize> {
    (bytes.get(at) == Some(&separator))
        .then(|| fixed(bytes, at.saturating_add(1), 2))
        .flatten()
}

/// `[0-9]{4} ('-'[0-9]{2} ('-'[0-9]{2})?)?`
fn date_format(bytes: &[u8]) -> Option<usize> {
    let year = fixed(bytes, 0, 4)?;
    let Some(month) = dashed(bytes, year, b'-') else {
        return Some(year);
    };
    Some(dashed(bytes, month, b'-').unwrap_or(month))
}

/// `[0-9]{2} (':'[0-9]{2} (':'[0-9]{2} ('.'[0-9]+)?)?)?` at `at`.
fn time_format(bytes: &[u8], at: usize) -> Option<usize> {
    let hour = fixed(bytes, at, 2)?;
    let Some(minute) = dashed(bytes, hour, b':') else {
        return Some(hour);
    };
    let Some(second) = dashed(bytes, minute, b':') else {
        return Some(minute);
    };
    if bytes.get(second) == Some(&b'.')
        && bytes
            .get(second.saturating_add(1))
            .is_some_and(u8::is_ascii_digit)
    {
        return Some(digits(bytes, second.saturating_add(1)));
    }
    Some(second)
}

/// `('Z' | ('+' | '-') [0-9]{2} ':' [0-9]{2})?` at `at`.
fn offset(bytes: &[u8], at: usize) -> usize {
    match bytes.get(at) {
        Some(b'Z') => at.saturating_add(1),
        Some(b'+' | b'-') => fixed(bytes, at.saturating_add(1), 2)
            .and_then(|hours| dashed(bytes, hours, b':'))
            .unwrap_or(at),
        _ => at,
    }
}

/// Lexes `text` into tokens with their byte spans.
pub(crate) fn lex(text: &str) -> Result<Vec<(Token, Range<usize>)>, ParseError> {
    let mut tokens = Vec::new();
    let mut lexer = Token::lexer(text);
    while let Some(token) = lexer.next() {
        match token {
            Ok(token) => tokens.push((token, lexer.span())),
            Err(()) => {
                return Err(ParseError::Lex {
                    offset: lexer.span().start,
                    text: String::from(lexer.slice()),
                });
            }
        }
    }
    Ok(tokens)
}

#[cfg(test)]
mod tests {
    use super::{Token, lex};
    use crate::quantity::Calendar;

    fn tokens(text: &str) -> Vec<Token> {
        lex(text)
            .expect("the text should lex")
            .into_iter()
            .map(|(token, _)| token)
            .collect()
    }

    #[test]
    fn an_integer_before_an_invocation_stops_at_the_point() {
        assert_eq!(
            tokens("1.toString()"),
            vec![
                Token::Number(String::from("1")),
                Token::Dot,
                Token::Identifier(String::from("toString")),
                Token::OpenParen,
                Token::CloseParen,
            ]
        );
    }

    #[test]
    fn a_decimal_keeps_its_fraction() {
        assert_eq!(tokens("1.50"), vec![Token::Number(String::from("1.50"))]);
    }

    #[test]
    fn a_datetime_stops_before_an_invocation() {
        assert_eq!(
            tokens("@2015-02-04T14:34:28.123.is"),
            vec![
                Token::Temporal(String::from("2015-02-04T14:34:28.123")),
                Token::Dot,
                Token::Is,
            ]
        );
        assert_eq!(
            tokens("@2015T"),
            vec![Token::Temporal(String::from("2015T"))]
        );
        assert_eq!(
            tokens("@2017-11-05T01:30:00.0-04:00"),
            vec![Token::Temporal(String::from("2017-11-05T01:30:00.0-04:00"))]
        );
    }

    #[test]
    fn a_time_takes_no_offset() {
        assert_eq!(
            tokens("@T14:34:28Z"),
            vec![
                Token::Temporal(String::from("T14:34:28")),
                Token::Identifier(String::from("Z")),
            ]
        );
    }

    #[test]
    fn escapes_resolve_and_unknown_ones_keep_their_character() {
        assert_eq!(
            tokens(r#"'\\\/\f\r\n\t\"\`\'*\p'"#),
            vec![Token::Text(String::from("\\/\u{c}\r\n\t\"`'*p"))]
        );
    }

    #[test]
    fn comments_are_skipped_and_an_open_one_is_left_as_operators() {
        assert_eq!(
            tokens("2 /* x */ // y\n"),
            vec![Token::Number(String::from("2"))]
        );
        assert_eq!(
            tokens("2 /* open")[..3],
            [Token::Number(String::from("2")), Token::Slash, Token::Star]
        );
        assert!(crate::parse::parse("2 /* open").is_err());
    }

    #[test]
    fn keywords_win_over_identifiers_of_the_same_length() {
        assert_eq!(
            tokens("div days divs"),
            vec![
                Token::Div,
                Token::Calendar(Calendar::Day),
                Token::Identifier(String::from("divs")),
            ]
        );
    }
}
