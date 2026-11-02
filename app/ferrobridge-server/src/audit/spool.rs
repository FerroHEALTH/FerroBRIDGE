// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The recorder every access record goes through: it writes the BALP
//! `AuditEvent` and stores it in the spool the forwarder delivers from.
//!
//! ITI TF-2 §3.20.4.1.1 has a sender store a record it cannot deliver and
//! send it when it can, so the recorder returns once the record is stored,
//! and the forwarder sends it over the ATX: FHIR Feed.

use std::sync::Arc;

use ihe_atna::atna::forwarder::Forwarder;
use ihe_atna::atna::spool::Spool;
use ihe_atna::atna::spool::reserve::Reservation;
use ihe_atna::balp::AuditError;
use ihe_atna::balp::AuditRecorder;
use ihe_atna::balp::Exchange;
use ihe_atna::balp::Observer;
use ihe_atna::balp::Reserved;
use secrecy::SecretSlice;

/// The bytes one record's place takes before the record exists; a record
/// that turns out larger takes the excess when it is stored.
// NOTE: no specification governs this: our own design; an access record of the facade
// runs to a few kilobytes, so a 16 KiB place covers one without a resize.
const PLACE_BYTES: u64 = 16 << 10;

/// The recorder over the spool of one forwarder, naming the bridge as
/// `observer`.
#[derive(Debug)]
pub(super) struct SpoolRecorder {
    /// The forwarder whose spool stores the records.
    forwarder: Arc<Forwarder<Spool>>,
    /// The recording system.
    observer: Arc<Observer>,
}

impl SpoolRecorder {
    /// Returns the recorder over `forwarder`'s spool.
    pub(super) fn new(forwarder: Arc<Forwarder<Spool>>, observer: Observer) -> Self {
        Self {
            forwarder,
            observer: Arc::new(observer),
        }
    }
}

/// Returns the `AuditEvent` `exchange` is written as.
fn written(exchange: &Exchange, observer: &Observer) -> Result<SecretSlice<u8>, AuditError> {
    exchange
        .audit_event(observer)
        .map(ihe_atna::balp::AuditRecord::into_bytes)
        .map_err(|error| AuditError(Box::new(error)))
}

#[async_trait::async_trait]
impl AuditRecorder for SpoolRecorder {
    async fn record(&self, exchange: Exchange) -> Result<(), AuditError> {
        let record = written(&exchange, &self.observer)?;
        self.forwarder
            .submit(record)
            .await
            .map_err(|error| AuditError(Box::new(error)))
    }

    async fn reserve(&self) -> Result<Option<Box<dyn Reserved>>, AuditError> {
        let reservation = self
            .forwarder
            .reserve(PLACE_BYTES)
            .map_err(|error| AuditError(Box::new(error)))?;
        Ok(Some(Box::new(Place {
            reservation,
            observer: Arc::clone(&self.observer),
        })))
    }
}

/// One record's place in the spool.
#[derive(Debug)]
struct Place {
    /// The place.
    reservation: Reservation,
    /// The recording system.
    observer: Arc<Observer>,
}

#[async_trait::async_trait]
impl Reserved for Place {
    async fn record(self: Box<Self>, exchange: Exchange) -> Result<(), AuditError> {
        let record = written(&exchange, &self.observer)?;
        self.reservation
            .push(record)
            .await
            .map_err(|error| AuditError(Box::new(error)))
    }
}
