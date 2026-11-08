// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The date value and the range its precision implies (search.html
//! §3.1.1.4.5 and §3.1.1.4.7).

use fhir_search::date::{DateError, DateValue, Offset, Precision, SearchDate};
use fhir_search::escape::EscapeError;
use fhir_search::prefix::{Prefix, PrefixError};

/// The low and high bound of `text`'s range as written.
fn bounds(text: &str) -> (String, Option<String>) {
    let range = SearchDate::parse(text).unwrap().range();
    (
        range.low().to_string(),
        range.high().map(ToString::to_string),
    )
}

fn range(low: &str, high: &str) -> (String, Option<String>) {
    (low.to_owned(), Some(high.to_owned()))
}

#[test]
fn a_day_spans_from_midnight_to_midnight() {
    // The date 2015-08-12 has a range from 00:00 to 00:00 exclusive.
    assert_eq!(bounds("2015-08-12"), range("2015-08-12", "2015-08-13"));
    // eq2013-01-14 matches 2013-01-14T10:00 and not 2013-01-15T00:00.
    let value = DateValue::parse("eq2013-01-14").unwrap();
    assert_eq!(value.prefix(), Prefix::Eq);
    assert_eq!(value.date().precision(), Precision::Day);
}

#[test]
fn a_year_and_a_month_span_their_whole_interval() {
    // 2000 is equivalent to [2000-01-01T00:00, 2000-12-31T23:59].
    assert_eq!(bounds("2000"), range("2000", "2001"));
    // 2000-04 is equivalent to [2000-04-01T00:00, 2000-04-30T23:59].
    assert_eq!(bounds("2000-04"), range("2000-04", "2000-05"));
    assert_eq!(bounds("2026-12"), range("2026-12", "2027-01"));
}

#[test]
fn a_day_crosses_months_years_and_leap_days() {
    assert_eq!(bounds("2024-02-28"), range("2024-02-28", "2024-02-29"));
    assert_eq!(bounds("2024-02-29"), range("2024-02-29", "2024-03-01"));
    assert_eq!(bounds("2023-02-28"), range("2023-02-28", "2023-03-01"));
    assert_eq!(bounds("2026-12-31"), range("2026-12-31", "2027-01-01"));
}

#[test]
fn a_time_without_seconds_spans_its_minute() {
    // Time can consist of hours and minutes with no seconds.
    let value = DateValue::parse("lt2013-01-14T10:00").unwrap();
    assert_eq!(value.prefix(), Prefix::Lt);
    assert_eq!(value.date().precision(), Precision::Minute);
    assert_eq!(
        value.date().offset(),
        None,
        "a zone SHOULD be given and need not be"
    );
    assert_eq!(
        bounds("2026-09-15T23:59Z"),
        range("2026-09-15T23:59Z", "2026-09-16T00:00Z")
    );
}

#[test]
fn a_time_with_seconds_spans_its_second_and_keeps_its_offset() {
    assert_eq!(
        bounds("2015-08-12T05:23:45+02:00"),
        range("2015-08-12T05:23:45+02:00", "2015-08-12T05:23:46+02:00")
    );
    assert_eq!(
        bounds("2026-12-31T23:59:59-05:30"),
        range("2026-12-31T23:59:59-05:30", "2027-01-01T00:00:00-05:30")
    );
    assert_eq!(
        SearchDate::parse("2015-08-12T05:23:45-00:00")
            .unwrap()
            .to_string(),
        "2015-08-12T05:23:45-00:00",
        "the sign is kept as written"
    );
}

#[test]
fn each_fraction_digit_narrows_the_range() {
    assert_eq!(
        bounds("2026-09-15T09:00:00.25"),
        range("2026-09-15T09:00:00.25", "2026-09-15T09:00:00.26")
    );
    assert_eq!(
        bounds("2026-09-15T09:00:00.5Z"),
        range("2026-09-15T09:00:00.5Z", "2026-09-15T09:00:00.6Z")
    );
    assert_eq!(
        SearchDate::parse("2026-09-15T09:00:00.250")
            .unwrap()
            .precision(),
        Precision::Fraction(3)
    );
}

#[test]
fn a_fraction_of_nines_carries_into_the_next_second() {
    assert_eq!(
        bounds("2026-12-31T23:59:59.999Z"),
        range("2026-12-31T23:59:59.999Z", "2027-01-01T00:00:00.000Z")
    );
}

#[test]
fn a_fraction_finer_than_a_nanosecond_stays_exact() {
    let digits = "1".repeat(30);
    let text = format!("2026-09-15T09:00:00.{digits}Z");
    let high = format!("2026-09-15T09:00:00.{}2Z", "1".repeat(29));
    assert_eq!(bounds(&text), range(&text, &high));
}

#[test]
fn the_last_year_runs_to_the_end_of_the_calendar() {
    assert_eq!(bounds("9999"), (String::from("9999"), None));
    assert_eq!(bounds("9999-12"), (String::from("9999-12"), None));
    assert_eq!(bounds("9999-12-31"), (String::from("9999-12-31"), None));
    assert_eq!(
        bounds("9999-12-31T23:59:59Z"),
        (String::from("9999-12-31T23:59:59Z"), None)
    );
    assert_eq!(bounds("9999-11"), range("9999-11", "9999-12"));
    assert_eq!(bounds("0001"), range("0001", "0002"));
}

#[test]
fn a_value_reads_back_as_written() {
    for text in [
        "2026",
        "2026-09",
        "2026-09-15",
        "2026-09-15T09:00",
        "2026-09-15T09:00Z",
        "2026-09-15T09:00:00",
        "2026-09-15T09:00:00.000000001+14:00",
        "2026-09-15T09:00:00-13:59",
    ] {
        assert_eq!(SearchDate::parse(text).unwrap().to_string(), text);
    }
}

#[test]
fn the_offset_is_read_with_its_sign() {
    let offset = |text: &str| SearchDate::parse(text).unwrap().offset();
    assert_eq!(offset("2026-09-15T09:00Z"), Some(Offset::Utc));
    assert_eq!(
        offset("2026-09-15T09:00+05:30").map(Offset::seconds),
        Some(19_800)
    );
    assert_eq!(
        offset("2026-09-15T09:00-03:00").map(Offset::seconds),
        Some(-10_800)
    );
    assert_eq!(offset("2026-09-15"), None);
}

#[test]
fn a_value_outside_the_form_is_refused() {
    for text in [
        "",
        "26",
        "20131",
        "2013-1",
        "2013-001",
        "2013-13",
        "2013-00",
        "2013-01-00",
        "2013-01-32",
        "2013-01-5",
        "2013-01-05T10",
        "2013-01-05T",
        "2013-01-05T1:00",
        "2013-01-05T24:00",
        "2013-01-05T10:60",
        "2013-01-05T10:00:61",
        "2013-01-05T10:00:0",
        "2013-01-05T10:00:00.",
        "2013-01-05T10:00:00.5x",
        "2013-01-05T10:00.5",
        "2013-01-05T10:00:00+2",
        "2013-01-05T10:00:00+02",
        "2013-01-05T10:00:00+0200",
        "2013-01-05T10:00:00+14:01",
        "2013-01-05T10:00:00+15:00",
        "2013-01-05T10:00:00-13:60",
        "2013-01-05T10:00:00ZZ",
        "2013-01-05T10:00:00z",
        "2013-01-05Z",
        "2013Z",
        "2013-01T10:00",
        "2013T10:00",
        "0000",
        "-2013",
        "+2013",
        " 2013",
        "2013 ",
        "2013-01-05 10:00",
        "23 May 2009",
        "２０１３",
        "2013-01-05T10:00:00Z/2014",
    ] {
        assert_eq!(SearchDate::parse(text), Err(DateError::Form), "{text:?}");
    }
}

#[test]
fn a_day_the_calendar_lacks_is_refused_as_such() {
    for text in ["2026-02-30", "2023-02-29", "2026-04-31", "1900-02-29"] {
        assert_eq!(SearchDate::parse(text), Err(DateError::Calendar), "{text}");
    }
    assert!(
        SearchDate::parse("2000-02-29").is_ok(),
        "2000 is a leap year"
    );
}

#[test]
fn a_leap_second_is_refused_as_such() {
    assert_eq!(
        SearchDate::parse("2016-12-31T23:59:60Z"),
        Err(DateError::LeapSecond)
    );
}

#[test]
fn a_prefixed_value_reads_its_prefix_and_its_date() {
    // GET [base]/Patient/23/Procedure?date=ge2010-01-01&date=le2011-12-31
    let from = DateValue::parse("ge2010-01-01").unwrap();
    let to = DateValue::parse("le2011-12-31").unwrap();
    assert_eq!((from.prefix(), to.prefix()), (Prefix::Ge, Prefix::Le));
    assert_eq!(
        to.date().range().high().map(ToString::to_string).as_deref(),
        Some("2012-01-01")
    );
    // GET [base]/Condition?onset=le1995
    assert_eq!(
        DateValue::parse("le1995").unwrap().date().precision(),
        Precision::Year
    );
    let near = DateValue::parse("ap2013-03-14").unwrap();
    assert_eq!(
        near.prefix(),
        Prefix::Ap,
        "ap is read, its margin is approximate's"
    );
}

#[test]
fn a_date_value_refuses_an_unknown_prefix_an_escape_and_a_bad_date() {
    assert_eq!(
        DateValue::parse("xx2013"),
        Err(DateError::Prefix(PrefixError::Unknown))
    );
    assert_eq!(
        DateValue::parse(r"2013\x"),
        Err(DateError::Escape(EscapeError::Illegal { at: 4 }))
    );
    assert_eq!(DateValue::parse(r"2013\,"), Err(DateError::Form));
    assert_eq!(DateValue::parse("gt"), Err(DateError::Form));
    assert_eq!(DateValue::parse("gtgt2013"), Err(DateError::Form));
    // GET [base]/Condition?onset=23%20May%202009 has an incorrect format.
    assert_eq!(DateValue::parse("23 May 2009"), Err(DateError::Form));
}

#[test]
fn debug_never_shows_the_date() {
    let shown = format!("{:?}", DateValue::parse("1958-04-27").unwrap());
    assert!(!shown.contains("1958"), "{shown}");
    assert!(shown.contains("Day"), "{shown}");
}

#[cfg(feature = "jiff")]
#[test]
fn first_day_is_the_calendar_day_a_range_starts_on() {
    let day = |text: &str| SearchDate::parse(text).unwrap().first_day().to_string();
    assert_eq!(day("1958"), "1958-01-01");
    assert_eq!(day("1958-04"), "1958-04-01");
    assert_eq!(day("1958-04-27"), "1958-04-27");
    assert_eq!(
        day("1958-04-27T23:59:59.9-05:00"),
        "1958-04-27",
        "the written day"
    );
    let range = SearchDate::parse("2024-02").unwrap().range();
    assert_eq!(
        range
            .high()
            .map(|high| high.first_day().to_string())
            .as_deref(),
        Some("2024-03-01")
    );
}

#[test]
fn a_time_steps_across_its_minute_hour_day_month_and_year() {
    for (text, high) in [
        ("2026-09-15T09:59", "2026-09-15T10:00"),
        ("2026-09-15T23:59", "2026-09-16T00:00"),
        ("2026-09-30T23:59:59Z", "2026-10-01T00:00:00Z"),
        ("2024-02-29T23:59:59.99", "2024-03-01T00:00:00.00"),
        ("2023-02-28T23:59", "2023-03-01T00:00"),
        ("2000-02-28T23:59", "2000-02-29T00:00"),
        ("1900-02-28T23:59", "1900-03-01T00:00"),
        ("2026-12-31T23:59:59.9+01:00", "2027-01-01T00:00:00.0+01:00"),
        ("2026-09-15T09:00:00.09", "2026-09-15T09:00:00.10"),
    ] {
        assert_eq!(bounds(text), range(text, high), "{text}");
    }
}

#[test]
fn the_calendar_knows_every_month_and_leap_year() {
    for (month, days) in [
        (1, 31),
        (3, 31),
        (4, 30),
        (5, 31),
        (6, 30),
        (7, 31),
        (8, 31),
        (9, 30),
        (10, 31),
        (11, 30),
        (12, 31),
    ] {
        let last = format!("2026-{month:02}-{days:02}");
        assert!(SearchDate::parse(&last).is_ok(), "{last}");
        if days < 31 {
            let past = format!("2026-{month:02}-{:02}", days + 1);
            assert_eq!(SearchDate::parse(&past), Err(DateError::Calendar), "{past}");
        }
    }
    for (year, leap) in [
        (2024, true),
        (2023, false),
        (2000, true),
        (1900, false),
        (2400, true),
    ] {
        let text = format!("{year}-02-29");
        assert_eq!(SearchDate::parse(&text).is_ok(), leap, "{text}");
    }
}
