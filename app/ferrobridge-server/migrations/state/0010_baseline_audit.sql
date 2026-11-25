-- SPDX-FileCopyrightText: Cadasto B.V.
-- SPDX-License-Identifier: BUSL-1.1

-- The audit outbox (docs/architecture.md section 7, "The state store").
--
-- audit_outbox holds each IHE BALP AuditEvent the bridge recorded and has not
-- yet delivered over the ITI-20 ATX: FHIR Feed (ITI TF-2 3.20.4.1.1: a sender
-- stores a record it cannot deliver and sends it when it can). A replica's
-- forwarder claims the oldest unclaimed row for a lease, so two replicas
-- never send one record at once and a record a stopped replica claimed falls
-- due again when the lease runs out; a delivered record is deleted.
-- audit_quarantine keeps a record the repository refused, for an operator.
--
-- The rows are the access records of Regulation (EU) 2025/327 Annex II 3.2.
-- Where they are kept is our own design.

CREATE TABLE audit_outbox (
    seq bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    -- The AuditEvent as it is sent.
    message bytea NOT NULL CONSTRAINT audit_outbox_message_check CHECK (length(message) > 0),
    recorded_at timestamptz NOT NULL DEFAULT now(),
    -- Until when a forwarder holds the row; null while unclaimed.
    claimed_until timestamptz
);

COMMENT ON TABLE audit_outbox IS 'The access records not yet delivered to the Audit Record Repository, claimed by one forwarder at a time. Our own design.';

CREATE TABLE audit_quarantine (
    seq bigint PRIMARY KEY,
    message bytea NOT NULL,
    recorded_at timestamptz NOT NULL,
    quarantined_at timestamptz NOT NULL DEFAULT now()
);

COMMENT ON TABLE audit_quarantine IS 'The access records the Audit Record Repository refused, kept for an operator. Our own design.';
