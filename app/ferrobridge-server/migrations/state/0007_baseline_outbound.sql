-- SPDX-FileCopyrightText: Cadasto B.V.
-- SPDX-License-Identifier: BUSL-1.1

-- The outbound FHIR lane (docs/architecture.md section 7, "The state store").
--
-- outbound_delivered holds, for each composition the lane delivered, the
-- [type]/[id] of every resource its latest delivery carried, so a deletion,
-- an erasure or an update retracts what the endpoint holds. A row is written
-- after the endpoint accepted the delivery, in the transaction that moves the
-- lane's watermark under its lease generation, so a crash never separates
-- the two. Identifiers only.
--
-- No specification governs the outbound lane: our own design.

CREATE TABLE outbound_delivered (
    versioned_object_id text PRIMARY KEY
        CONSTRAINT outbound_delivered_versioned_object_id_check CHECK (versioned_object_id <> ''),
    ehr_id text NOT NULL,
    refs text[] NOT NULL CONSTRAINT outbound_delivered_refs_check CHECK (cardinality(refs) > 0),
    delivered_at timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX outbound_delivered_ehr ON outbound_delivered (ehr_id);

COMMENT ON TABLE outbound_delivered IS 'What the outbound FHIR lane last delivered for each composition, committed with the lane watermark. Our own design.';
