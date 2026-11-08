// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The instants a date's range holds, and the `ap` margin (search.html
//! §3.1.1.4.5 and §3.1.1.4.7).

use fhir_search::date::{DateError, Instants, SearchDate};
use jiff::Timestamp;
use jiff::tz::{Offset, TimeZone};

fn at(text: &str) -> Timestamp {
    text.parse().unwrap()
}

fn instants(text: &str, zone: &TimeZone) -> Instants {
    SearchDate::parse(text).unwrap().instants(zone).unwrap()
}

fn span(text: &str) -> (Timestamp, Option<Timestamp>) {
    let read = instants(text, &TimeZone::UTC);
    (read.from(), read.until())
}

#[test]
fn a_year_is_its_whole_year() {
    // 2000 is equivalent to [2000-01-01T00:00, 2000-12-31T23:59].
    assert_eq!(
        span("2000"),
        (at("2000-01-01T00:00:00Z"), Some(at("2001-01-01T00:00:00Z")))
    );
}

#[test]
fn a_month_ends_on_its_last_day() {
    // 2000-04 is equivalent to [2000-04-01T00:00, 2000-04-30T23:59].
    assert_eq!(
        span("2000-04"),
        (at("2000-04-01T00:00:00Z"), Some(at("2000-05-01T00:00:00Z")))
    );
}

#[test]
fn a_day_is_its_whole_day() {
    // The date 2013-01-10 specifies 00:00 on 10-Jan to before 00:00 on 11-Jan.
    assert_eq!(
        span("2013-01-10"),
        (at("2013-01-10T00:00:00Z"), Some(at("2013-01-11T00:00:00Z")))
    );
}

#[test]
fn a_date_without_a_zone_reads_in_the_server_zone() {
    // The server's local time zone should be assumed.
    let zone = TimeZone::fixed(Offset::constant(2));
    let read = instants("2013-01-05", &zone);
    assert_eq!(read.from(), at("2013-01-04T22:00:00Z"));
    assert_eq!(read.until(), Some(at("2013-01-05T22:00:00Z")));
    let time = instants("2013-01-05T10:00", &zone);
    assert_eq!(
        time.from(),
        at("2013-01-05T08:00:00Z"),
        "a zoneless time too"
    );
}

#[test]
fn a_written_offset_wins_over_the_server_zone() {
    let zone = TimeZone::fixed(Offset::constant(9));
    let read = instants("2013-01-05T10:00+01:00", &zone);
    assert_eq!(read.from(), at("2013-01-05T09:00:00Z"));
    assert_eq!(read.until(), Some(at("2013-01-05T09:01:00Z")));
}

#[test]
fn fraction_digits_narrow_the_instants() {
    assert_eq!(
        span("2013-01-05T10:00:00.5Z"),
        (
            at("2013-01-05T10:00:00.5Z"),
            Some(at("2013-01-05T10:00:00.6Z"))
        )
    );
    assert_eq!(
        span("2013-01-05T10:00:00.123456789Z"),
        (
            at("2013-01-05T10:00:00.123456789Z"),
            Some(at("2013-01-05T10:00:00.12345679Z"))
        )
    );
}

#[test]
fn a_range_finer_than_a_nanosecond_holds_the_nanoseconds_inside_it() {
    // [x.0000000001, x.0000000002) holds no nanosecond instant.
    let (from, until) = span("2013-01-05T10:00:00.0000000001Z");
    assert_eq!(from, at("2013-01-05T10:00:00.000000001Z"));
    assert_eq!(until, Some(from), "an empty range");
    // [x.0000000000, x.0000000001) holds x itself.
    let (from, until) = span("2013-01-05T10:00:00.0000000000Z");
    assert_eq!(from, at("2013-01-05T10:00:00Z"));
    assert_eq!(until, Some(at("2013-01-05T10:00:00.000000001Z")));
}

#[test]
fn a_range_past_the_last_instant_stays_open() {
    let read = instants("9999", &TimeZone::UTC);
    assert_eq!(read.from(), at("9999-01-01T00:00:00Z"));
    assert_eq!(read.until(), None);
}

#[test]
fn a_range_starting_past_the_last_instant_is_unrepresentable() {
    assert_eq!(
        SearchDate::parse("9999-12-31T23:59Z")
            .unwrap()
            .instants(&TimeZone::UTC),
        Err(DateError::Unrepresentable)
    );
}

#[test]
fn ap_widens_the_range_by_a_tenth_of_its_gap_to_now() {
    let now = at("2023-03-14T00:00:00Z");
    let date = SearchDate::parse("2013-03-14").unwrap();
    let near = date.approximate(&TimeZone::UTC, now).unwrap();
    let gap = now.duration_since(at("2013-03-15T00:00:00Z"));
    assert_eq!(
        near.from(),
        at("2013-03-14T00:00:00Z").checked_sub(gap / 10).unwrap()
    );
    assert_eq!(
        near.until(),
        at("2013-03-15T00:00:00Z").checked_add(gap / 10).ok()
    );
    // 15-Jun 2015 is not near 14-Mar 2013, and 14-Mar 2013 is.
    assert!(
        near.until()
            .is_some_and(|until| at("2015-06-15T00:00:00Z") >= until)
    );
    assert!(near.from() <= at("2013-03-14T12:00:00Z"));
}

#[test]
fn ap_on_a_range_holding_now_is_the_range() {
    let now = at("2013-03-14T12:00:00Z");
    let date = SearchDate::parse("2013-03-14").unwrap();
    assert_eq!(
        date.approximate(&TimeZone::UTC, now).unwrap(),
        date.instants(&TimeZone::UTC).unwrap()
    );
}

#[test]
fn ap_on_a_future_date_measures_from_its_low_bound() {
    let now = at("2013-03-04T00:00:00Z");
    let date = SearchDate::parse("2013-03-14").unwrap();
    let near = date.approximate(&TimeZone::UTC, now).unwrap();
    assert_eq!(near.from(), at("2013-03-13T00:00:00Z"));
    assert_eq!(near.until(), Some(at("2013-03-16T00:00:00Z")));
}
