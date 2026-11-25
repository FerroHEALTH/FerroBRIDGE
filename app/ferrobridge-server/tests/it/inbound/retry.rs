// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A transient failure runs again after a capped, jittered backoff until the
//! attempts run out; a refusal parks at once.

use std::time::Duration;

use ferrobridge_server::inbound::STATUSES;
use ferrobridge_server::retry::{Backoff, Failure, Next, Policy};
use http::StatusCode;

const POLICY: Policy = Policy {
    backoff: Backoff {
        initial: Duration::from_secs(2),
        max: Duration::from_secs(60),
    },
    max_attempts: 5,
};

#[test]
fn the_failure_statuses_split_into_transient_and_refusal() {
    for status in [
        StatusCode::INTERNAL_SERVER_ERROR,
        StatusCode::BAD_GATEWAY,
        StatusCode::SERVICE_UNAVAILABLE,
        StatusCode::GATEWAY_TIMEOUT,
        StatusCode::TOO_MANY_REQUESTS,
        StatusCode::LOCKED,
    ] {
        assert_eq!(Failure::Transient, STATUSES.failure(status), "{status}");
    }
    for status in [
        StatusCode::BAD_REQUEST,
        StatusCode::UNAUTHORIZED,
        StatusCode::FORBIDDEN,
        StatusCode::NOT_FOUND,
        StatusCode::CONFLICT,
        StatusCode::UNPROCESSABLE_ENTITY,
    ] {
        assert_eq!(Failure::Poisoned, STATUSES.failure(status), "{status}");
    }
}

#[test]
fn the_delay_doubles_and_jitter_keeps_half_to_all_of_it() {
    assert_eq!(
        Next::Retry(Duration::from_secs(1)),
        POLICY.next(Failure::Transient, 1, 0)
    );
    let full = POLICY.next(Failure::Transient, 1, u32::MAX);
    assert!(
        matches!(full, Next::Retry(delay) if delay <= Duration::from_secs(2) && delay > Duration::from_millis(1999)),
        "{full:?}"
    );
    assert_eq!(
        Next::Retry(Duration::from_secs(4)),
        POLICY.next(Failure::Transient, 3, 0)
    );
}

#[test]
fn the_delay_stops_at_its_cap() {
    let wide = Policy {
        max_attempts: 100,
        ..POLICY
    };
    assert_eq!(
        Next::Retry(Duration::from_secs(30)),
        wide.next(Failure::Transient, 10, 0)
    );
    assert_eq!(
        Next::Retry(Duration::from_secs(30)),
        wide.next(Failure::Transient, 90, 0)
    );
}

#[test]
fn a_refusal_parks_at_once_and_the_last_attempt_parks() {
    assert_eq!(Next::Park, POLICY.next(Failure::Poisoned, 1, 0));
    assert_eq!(Next::Park, POLICY.next(Failure::Transient, 5, 0));
    assert!(matches!(
        POLICY.next(Failure::Transient, 4, 0),
        Next::Retry(_)
    ));
}
