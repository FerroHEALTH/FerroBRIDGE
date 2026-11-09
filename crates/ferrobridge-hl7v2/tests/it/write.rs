// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Composing messages by the encoding rules of HL7 v2.9 chapter 2, checked
//! against the generated structures and parsed back.

use ferrobridge_hl7v2::ack::{Code, Error, Header, Stamp};
use ferrobridge_hl7v2::decode::{Charset, DecodeError, decode};
use ferrobridge_hl7v2::parse::lex::lex;
use ferrobridge_hl7v2::parse::profile::IHE_ITI_21;
use ferrobridge_hl7v2::parse::structure::{structure_declared, structure_for};
use ferrobridge_hl7v2::parse::{Delimiters, ErrorCode, Location, Message, Unplaced};
use ferrobridge_hl7v2::write::header::Msh;
use ferrobridge_hl7v2::write::value::{Component, Field, Repetition, Segment};
use ferrobridge_hl7v2::write::{ComposeError, EchoError, Writer};

/// The delimiters chapter 2 §2.5.4 suggests, with no truncation character.
const SUGGESTED: Delimiters = Delimiters {
    field: '|',
    component: '^',
    repetition: '~',
    escape: '\\',
    subcomponent: '&',
    truncation: None,
};

/// A synthetic PIX query, QBP^Q23 in HL7 v2.5, whose QPD-3 holds an
/// escaped component separator.
const QUERY: &str = "MSH|^~\\&|CONSUMER|EAST|PIX|NORTH|20261025100000||QBP^Q23^QBP_Q21|Q-1|P|2.5\r\
                     QPD|IHE PIX Query|TAG-1|A\\S\\1^^^EAST&2.999.1.1&ISO|^^^&2.999.1.2&ISO\r\
                     RCP|I\r";

fn stamp() -> Stamp<'static> {
    Stamp {
        control_id: "ANSWER-1",
        timestamp: "20261025100001+0000",
    }
}

fn query() -> Message {
    lex(QUERY, Charset::Ascii).expect("the query lexes").message
}

/// The field at `position` of the first segment `id` of `message`.
fn field<'m>(
    message: &'m Message,
    id: &str,
    position: usize,
) -> &'m ferrobridge_hl7v2::parse::Field {
    message
        .segments()
        .iter()
        .find(|segment| segment.id() == id)
        .and_then(|segment| segment.field(position))
        .expect("the field is present")
}

/// A PID-3 repetition: the id and its assigning authority's three
/// subcomponents.
fn cx(id: &str, namespace: &str, universal: &str, kind: &str) -> Repetition {
    Repetition::from_iter([
        Component::from(id),
        Component::default(),
        Component::default(),
        Component::from_iter([namespace, universal, kind]),
    ])
}

#[test]
fn an_rsp_k23_replies_in_the_query_s_delimiters_and_parses_back_as_its_structure() {
    let query = query();
    let header = Header::from_text(QUERY).expect("a header");
    let writer = header.writer();
    let qak = Segment::new(
        "QAK",
        [
            writer
                .echo(field(&query, "QPD", 2), query.delimiters())
                .expect("same delimiters"),
            Field::from("OK"),
        ],
    );
    let qpd = writer
        .echo_segment(&query, "QPD")
        .expect("same delimiters")
        .expect("the query holds a QPD");
    let pid = Segment::new(
        "PID",
        [
            Field::default(),
            Field::default(),
            Field::from_iter([cx("N|1", "NORTH", "2.999.1.2", "ISO")]),
            Field::default(),
            Field::from(" "),
        ],
    );
    let segments = [
        header.reply(
            Field::from(Repetition::from_iter(["RSP", "K23", "RSP_K23"])),
            stamp(),
        ),
        header.msa(Code::Accept),
        qak,
        qpd,
        pid,
    ];
    let composed = writer
        .compose(&segments, structure_declared, Charset::Ascii)
        .expect("the reply conforms to RSP_K23");
    assert_eq!(
        composed.text(),
        "MSH|^~\\&|PIX|NORTH|CONSUMER|EAST|20261025100001+0000||RSP^K23^RSP_K23|ANSWER-1|P|2.5\r\
         MSA|AA|Q-1\r\
         QAK|TAG-1|OK\r\
         QPD|IHE PIX Query|TAG-1|A\\S\\1^^^EAST&2.999.1.1&ISO|^^^&2.999.1.2&ISO\r\
         PID|||N\\F\\1^^^NORTH&2.999.1.2&ISO|| \r"
    );
    let reply = lex(composed.text(), composed.charset())
        .expect("the reply lexes")
        .message;
    let id = field(&reply, "PID", 3).repetitions()[0]
        .component(1)
        .and_then(|component| component.subcomponent(1));
    assert_eq!(id, Some("N|1"), "the escaped id reads back as written");
}

#[test]
fn an_adt_a31_names_utf_8_in_msh_18_and_travels_in_it() {
    let writer = Writer::new(SUGGESTED);
    let msh = Msh {
        sending_application: Field::from("FERROPIX"),
        sending_facility: Field::from("NORTH"),
        receiving_application: Field::from("PIXCONSUMER"),
        receiving_facility: Field::from("WEST"),
        timestamp: Field::from("20261026120000+0000"),
        message_type: Field::from(Repetition::from_iter(["ADT", "A31", "ADT_A05"])),
        control_id: Field::from("NOTICE-1"),
        processing_id: Field::from("P"),
        version: Field::from("2.5"),
        charset: Field::from(Charset::Utf8.code()),
    };
    let segments = [
        Segment::from(msh),
        Segment::new("EVN", ["A31", "20261026120000+0000"]),
        Segment::new(
            "PID",
            [
                Field::from("1"),
                Field::default(),
                Field::from_iter([cx("\u{C9}-1", "EAST", "2.999.1.1", "ISO")]),
                Field::default(),
                Field::from(" "),
            ],
        ),
        Segment::new("PV1", ["", "N"]),
    ];
    let composed = writer
        .compose(&segments, structure_declared, Charset::Ascii)
        .expect("the notification conforms to ADT_A05");
    assert_eq!(composed.charset(), Charset::Utf8);
    assert!(
        composed
            .text()
            .starts_with("MSH|^~\\&|FERROPIX|NORTH|PIXCONSUMER|WEST|20261026120000+0000||ADT^A31^ADT_A05|NOTICE-1|P|2.5||||||UNICODE UTF-8\r"),
        "{}",
        composed.text()
    );
    let bytes = composed.encoded().expect("UTF-8 holds every character");
    let decoded = decode(&bytes, Charset::Ascii).expect("the bytes decode in the set MSH-18 names");
    assert_eq!(decoded.charset, Charset::Utf8);
    assert!(
        decoded
            .text
            .contains("PID|1||\u{C9}-1^^^EAST&2.999.1.1&ISO|| \r")
    );
}

#[test]
fn a_character_outside_the_default_set_is_refused_when_encoded() {
    let writer = Writer::new(SUGGESTED);
    let header = Header::from_text(QUERY).expect("a header");
    let segments = [
        header.reply(
            Field::from(Repetition::from_iter(["ACK", "Q23", "ACK"])),
            stamp(),
        ),
        header.msa(Code::Error),
        Segment::from(&Error {
            location: None,
            code: ErrorCode::Application,
            text: String::from("M\u{FC}ller"),
        }),
    ];
    let composed = writer
        .compose(&segments, structure_for, Charset::Ascii)
        .expect("the acknowledgment conforms to ACK");
    assert!(matches!(
        composed.encoded(),
        Err(DecodeError::Undeclared {
            charset: "ASCII",
            ..
        })
    ));
}

#[test]
fn msh_18_naming_a_set_this_crate_does_not_encode_is_refused() {
    let writer = Writer::new(SUGGESTED);
    let msh = Msh {
        message_type: Field::from(Repetition::from_iter(["ACK", "A01", "ACK"])),
        control_id: Field::from("A-1"),
        processing_id: Field::from("P"),
        version: Field::from("2.5"),
        charset: Field::from("ISO IR87"),
        ..Msh::default()
    };
    let segments = [Segment::from(msh), Segment::new("MSA", ["AA", "M-1"])];
    assert_eq!(
        writer.compose(&segments, structure_for, Charset::Ascii),
        Err(ComposeError::UnsupportedCharset {
            code: String::from("ISO IR87")
        })
    );
}

#[test]
fn every_delimiter_the_truncation_character_included_is_escaped_and_read_back() {
    let delimiters = Delimiters {
        truncation: Some('#'),
        ..SUGGESTED
    };
    let writer = Writer::new(delimiters);
    let value = "a|b^c~d\\e&f#g";
    assert_eq!(
        writer.escape(value),
        "a\\F\\b\\S\\c\\R\\d\\E\\e\\T\\f\\P\\g"
    );
    let text = writer.message(&[
        Segment::from(Msh {
            message_type: Field::from(Repetition::from_iter(["ACK", "A01", "ACK"])),
            control_id: Field::from("A-1"),
            processing_id: Field::from("P"),
            version: Field::from("2.9"),
            ..Msh::default()
        }),
        Segment::new("MSA", ["AA", value]),
    ]);
    assert!(text.starts_with("MSH|^~\\&#|"), "{text}");
    let lexed = lex(&text, Charset::Ascii).expect("the message lexes");
    assert!(lexed.refusals.is_empty(), "{:?}", lexed.refusals);
    assert_eq!(
        lexed.message.segments()[1].text(2),
        Some(value),
        "the value reads back unchanged"
    );
}

#[test]
fn a_line_break_in_a_value_becomes_a_space() {
    let writer = Writer::new(SUGGESTED);
    assert_eq!(writer.escape("a\rb\nc^d"), "a b c\\S\\d");
}

#[test]
fn trailing_empty_fields_components_and_subcomponents_are_left_out() {
    let writer = Writer::new(SUGGESTED);
    let segment = Segment::new(
        "PID",
        [
            Field::from("1"),
            Field::from(Repetition::from_iter([
                Component::from_iter(["", "2.999", "ISO", ""]),
                Component::default(),
            ])),
            Field::default(),
            Field::default(),
        ],
    );
    assert_eq!(writer.segment(&segment), "PID|1|&2.999&ISO");
}

#[test]
fn an_echo_keeps_a_formatting_sequence_as_read() {
    let text = "MSH|^~\\&|A|B|C|D|20261025||ORU^R01^ORU_R01|M-1|P|2.5\rNTE|1||\\H\\bold\\N\\ and a\\E\\b\r";
    let message = lex(text, Charset::Ascii)
        .expect("the message lexes")
        .message;
    let writer = Writer::new(*message.delimiters());
    let echoed = writer
        .echo(field(&message, "NTE", 3), message.delimiters())
        .expect("same delimiters");
    assert_eq!(
        writer.segment(&Segment::new(
            "NTE",
            [Field::from("1"), Field::default(), echoed]
        )),
        "NTE|1||\\H\\bold\\N\\ and a\\E\\b"
    );
}

#[test]
fn an_echo_into_other_delimiters_is_refused() {
    let query = query();
    let writer = Writer::new(Delimiters {
        component: '!',
        ..SUGGESTED
    });
    assert_eq!(
        writer.echo(field(&query, "QPD", 3), query.delimiters()),
        Err(EchoError::Delimiters)
    );
    assert_eq!(
        writer.echo_segment(&query, "QPD"),
        Err(EchoError::Delimiters)
    );
}

/// A synthetic demographics query, QBP^Q22 in HL7 v2.5.
const PDQ_QUERY: &str = "MSH|^~\\&|CONSUMER|EAST|PDQ|NORTH|20261025100000||QBP^Q22^QBP_Q21|Q-2|P|2.5\r\
                         QPD|IHE PDQ Query|TAG-2|@PID.8^F\r\
                         RCP|I\r";

/// The ITI-21 RSP^K22 with one PID and no QRI.
fn rsp_k22() -> (Header, Vec<Segment>) {
    rsp_k22_to(PDQ_QUERY)
}

/// The ITI-21 RSP^K22 answering `query`, its QPD echoed, with one PID and no
/// QRI.
fn rsp_k22_to(text: &str) -> (Header, Vec<Segment>) {
    let query = lex(text, Charset::Ascii).expect("the query lexes").message;
    let header = Header::from_text(text).expect("a header");
    let writer = header.writer();
    let qak = Segment::new(
        "QAK",
        [
            writer
                .echo(field(&query, "QPD", 2), query.delimiters())
                .expect("same delimiters"),
            Field::from("OK"),
        ],
    );
    let qpd = writer
        .echo_segment(&query, "QPD")
        .expect("same delimiters")
        .expect("the query holds a QPD");
    let pid = Segment::new(
        "PID",
        [
            Field::from("1"),
            Field::default(),
            Field::from_iter([cx("E-1", "EAST", "2.999.1.1", "ISO")]),
            Field::default(),
            Field::from(Repetition::from_iter(["Family", "Given"])),
        ],
    );
    let segments = vec![
        header.reply(
            Field::from(Repetition::from_iter(["RSP", "K22", "RSP_K21"])),
            stamp(),
        ),
        header.msa(Code::Accept),
        qak,
        qpd,
        pid,
    ];
    (header, segments)
}

// NOTE: IHE ITI TF-2 §3.21.4.2.2 Table 3.21-6 makes QRI optional in the ITI-21 response,
// where the v2.5 RSP_K21 requires one per PID.
#[test]
fn an_iti_21_response_without_qri_conforms_under_the_ihe_constraint_and_not_the_base() {
    let (header, segments) = rsp_k22();
    let refused = header
        .writer()
        .compose(&segments, structure_declared, Charset::Ascii);
    let Err(ComposeError::Nonconforming { refusals, .. }) = refused else {
        panic!("the base RSP_K21 requires QRI: {refused:?}");
    };
    assert!(
        refusals
            .iter()
            .any(|refusal| refusal.detail.contains("RSP_K21.7-QUERY_RESPONSE.4-QRI")),
        "{refusals:?}"
    );
    let composed = header
        .writer()
        .with_profile(&IHE_ITI_21)
        .compose(&segments, structure_declared, Charset::Ascii)
        .expect("the response conforms to RSP_K21 as ITI-21 constrains it");
    assert_eq!(
        composed.text(),
        "MSH|^~\\&|PDQ|NORTH|CONSUMER|EAST|20261025100001+0000||RSP^K22^RSP_K21|ANSWER-1|P|2.5\r\
         MSA|AA|Q-2\r\
         QAK|TAG-2|OK\r\
         QPD|IHE PDQ Query|TAG-2|@PID.8^F\r\
         PID|1||E-1^^^EAST&2.999.1.1&ISO||Family^Given\r"
    );
}

#[test]
fn a_profile_relaxes_only_the_members_it_names() {
    let (header, mut segments) = rsp_k22();
    segments.retain(|segment| segment.id() != "QAK");
    let refused = header.writer().with_profile(&IHE_ITI_21).compose(
        &segments,
        structure_declared,
        Charset::Ascii,
    );
    let Err(ComposeError::Nonconforming { refusals, .. }) = refused else {
        panic!("QAK stays required under ITI-21: {refused:?}");
    };
    assert!(
        refusals
            .iter()
            .any(|refusal| refusal.detail.contains("QAK")),
        "{refusals:?}"
    );
    assert!(
        !refusals
            .iter()
            .any(|refusal| refusal.detail.contains("QRI")),
        "{refusals:?}"
    );
}

#[test]
fn a_reply_missing_a_required_segment_is_refused() {
    let header = Header::from_text(QUERY).expect("a header");
    let segments = [
        header.reply(
            Field::from(Repetition::from_iter(["RSP", "K23", "RSP_K23"])),
            stamp(),
        ),
        header.msa(Code::Accept),
    ];
    let refused = header
        .writer()
        .compose(&segments, structure_declared, Charset::Ascii);
    let Err(ComposeError::Nonconforming { refusals, .. }) = refused else {
        panic!("the reply lacks QAK and QPD: {refused:?}");
    };
    assert!(
        refusals
            .iter()
            .any(|refusal| refusal.detail.contains("RSP_K23.5-QAK")),
        "{refusals:?}"
    );
}

#[test]
fn a_segment_the_structure_has_no_place_for_is_refused() {
    let header = Header::from_text(QUERY).expect("a header");
    let segments = [
        header.reply(
            Field::from(Repetition::from_iter(["ACK", "Q23", "ACK"])),
            stamp(),
        ),
        header.msa(Code::Accept),
        Segment::new("PID", ["1"]),
    ];
    let refused = header
        .writer()
        .compose(&segments, structure_for, Charset::Ascii);
    let Err(ComposeError::Nonconforming { unplaced, .. }) = refused else {
        panic!("ACK has no PID: {refused:?}");
    };
    assert!(
        unplaced
            .iter()
            .any(|outcome| matches!(outcome, Unplaced::OutOfStructure { location } if location.segment == "PID")),
        "{unplaced:?}"
    );
}

#[test]
fn an_err_segment_carries_its_location_as_erl_components() {
    let writer = Writer::new(SUGGESTED);
    let mut location = Location::segment("QPD", 1).with_field(4);
    location.repetition = Some(2);
    let error = Error {
        location: Some(location),
        code: ErrorCode::UnknownKeyIdentifier,
        text: String::from("a | b"),
    };
    assert_eq!(
        writer.segment(&Segment::from(&error)),
        "ERR||QPD^1^4^2|204^Unknown key identifier^HL70357|E||||a \\F\\ b"
    );
}

/// Lexes and groups `text` by the structure it declares, under `profile`.
fn parse_text(
    text: &str,
    profile: &'static ferrobridge_hl7v2::parse::profile::Profile,
) -> ferrobridge_hl7v2::parse::Parsed {
    let lexed = lex(text, Charset::Ascii).expect("the message lexes");
    let structure = structure_declared(&lexed.message).expect("a declared structure");
    ferrobridge_hl7v2::parse::grouping::group(lexed, structure, profile)
}

/// A synthetic demographics query with two parameters, family name and sex,
/// in the shape of the example of IHE ITI TF-2 §3.21.4.1.2.3.
const PDQ_TWO_PARAMETERS: &str = "MSH|^~\\&|CONSUMER|EAST|PDQ|NORTH|20261025100000||QBP^Q22^QBP_Q21|Q-3|P|2.5\r\
                                  QPD|IHE PDQ Query|TAG-3|@PID.5.1.1^SMITH~@PID.8^F\r\
                                  RCP|I\r";

// NOTE: IHE ITI TF-2 §3.21.4.1.2.3: QPD-3 "consists of one or more repetitions", one per
// parameter, where the v2.5 QPD gives QPD-3 one; the RSP^K22 echoes the QPD (Table 3.21-6).
#[test]
fn an_iti_21_query_and_response_with_two_parameters_conform_under_the_ihe_constraint() {
    let query = parse_text(PDQ_TWO_PARAMETERS, &IHE_ITI_21);
    assert!(
        !query
            .unplaced()
            .iter()
            .any(|unplaced| matches!(unplaced, Unplaced::ExtraRepetition { .. })),
        "{:?}",
        query.unplaced()
    );
    let (header, segments) = rsp_k22_to(PDQ_TWO_PARAMETERS);
    let composed = header
        .writer()
        .with_profile(&IHE_ITI_21)
        .compose(&segments, structure_declared, Charset::Ascii)
        .expect("the response echoes both parameters under ITI-21");
    assert_eq!(
        composed.text(),
        "MSH|^~\\&|PDQ|NORTH|CONSUMER|EAST|20261025100001+0000||RSP^K22^RSP_K21|ANSWER-1|P|2.5\r\
         MSA|AA|Q-3\r\
         QAK|TAG-3|OK\r\
         QPD|IHE PDQ Query|TAG-3|@PID.5.1.1^SMITH~@PID.8^F\r\
         PID|1||E-1^^^EAST&2.999.1.1&ISO||Family^Given\r"
    );
}

#[test]
fn a_second_qpd_3_repetition_is_extra_in_the_base_rsp_k21() {
    let (header, segments) = rsp_k22_to(PDQ_TWO_PARAMETERS);
    let refused = header
        .writer()
        .compose(&segments, structure_declared, Charset::Ascii);
    let Err(ComposeError::Nonconforming { unplaced, .. }) = refused else {
        panic!("the base QPD-3 holds one repetition: {refused:?}");
    };
    assert!(
        unplaced
            .iter()
            .any(|unplaced| matches!(unplaced, Unplaced::ExtraRepetition { field: "QPD.3", .. })),
        "{unplaced:?}"
    );
}

// NOTE: IHE ITI TF-2 §3.21.4.1.2.3 lets QPD-3 repeat and no other QPD field; QPD-2, the query
// tag the QAK echoes, keeps its one repetition under ITI-21.
#[test]
fn a_second_query_tag_stays_extra_under_the_ihe_constraint() {
    let query = "MSH|^~\\&|CONSUMER|EAST|PDQ|NORTH|20261025100000||QBP^Q22^QBP_Q21|Q-4|P|2.5\r\
                 QPD|IHE PDQ Query|TAG-4~TAG-5|@PID.8^F\r\
                 RCP|I\r";
    let (header, segments) = rsp_k22_to(query);
    let refused = header.writer().with_profile(&IHE_ITI_21).compose(
        &segments,
        structure_declared,
        Charset::Ascii,
    );
    let Err(ComposeError::Nonconforming { unplaced, .. }) = refused else {
        panic!("QPD-2 holds one repetition under ITI-21: {refused:?}");
    };
    assert!(
        unplaced
            .iter()
            .any(|unplaced| matches!(unplaced, Unplaced::ExtraRepetition { field: "QPD.2", .. })),
        "{unplaced:?}"
    );
}
