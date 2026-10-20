// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Escape sequences and the ERL rendering of a location.

use ferrobridge_hl7v2::decode::Charset;
use ferrobridge_hl7v2::parse::lex::{LexError, lex};
use ferrobridge_hl7v2::parse::{ErrorCode, Location};

#[test]
#[expect(clippy::panic_in_result_fn, reason = "test assertions")]
fn escape_sequences_decode_to_their_delimiters() -> Result<(), LexError> {
    let lexed = lex(
        "MSH|^~\\&|A\rNTE|||a\\F\\b\\S\\c\\E\\d\\X41\\",
        Charset::Ascii,
    )?;
    let note = lexed
        .message
        .segments()
        .get(1)
        .and_then(|segment| segment.text(3));
    assert_eq!(note, Some("a|b^c\\dA"));
    assert!(lexed.refusals.is_empty(), "{:?}", lexed.refusals);
    Ok(())
}

#[test]
#[expect(clippy::panic_in_result_fn, reason = "test assertions")]
fn an_undecoded_escape_sequence_is_refused_at_its_location() -> Result<(), LexError> {
    let lexed = lex("MSH|^~\\&|A\rNTE|||\\Zlocal\\", Charset::Ascii)?;
    let refusal = lexed.refusals.first();
    assert_eq!(
        refusal.map(|refusal| refusal.code),
        Some(ErrorCode::DataType)
    );
    assert_eq!(
        refusal.map(|refusal| refusal.location.erl('^')),
        Some(String::from("NTE^1^3^1^1^1"))
    );
    Ok(())
}

#[test]
fn a_location_renders_its_known_erl_components() {
    let location = Location::segment("PID", 1).with_field(5);
    assert_eq!(location.erl('^'), "PID^1^5");
    assert_eq!(location.to_string(), "PID[1]-5");
}
