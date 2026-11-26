-- SPDX-FileCopyrightText: Cadasto B.V.
-- SPDX-License-Identifier: BUSL-1.1

-- The inbound intake (docs/architecture.md section 7, "The inbound intake").
--
-- inbound_event is the inbox: every verified event a source sends, stored
-- before the source is answered and recognized again by its de-duplication
-- key. inbound_work holds one item per entity, so many events for one
-- patient cost one fetch; every accepted event moves its generation on, and
-- a worker finishing at an older generation leaves the item pending.
-- inbound_slot bounds the remote calls in flight per source and tenant
-- across every replica. inbound_cursor holds where each reconciliation poll
-- reads from, and fhir_subscription_state each FHIR Subscription's status,
-- highest event and gap. An accepted event notifies ferrobridge_inbound.
--
-- No specification governs an inbound intake: our own design.

CREATE TABLE inbound_event (
    id uuid PRIMARY KEY DEFAULT uuidv7(),
    source_id uuid NOT NULL REFERENCES inbound_source ON DELETE CASCADE,
    dedupe_key bytea NOT NULL CONSTRAINT inbound_event_dedupe_key_check
        CHECK (octet_length(dedupe_key) = 32),
    -- The tenant as the source document names it.
    tenant text NOT NULL,
    entity_key text NOT NULL CONSTRAINT inbound_event_entity_key_check CHECK (entity_key <> ''),
    change text NOT NULL CONSTRAINT inbound_event_change_check CHECK (change IN ('upsert', 'remove')),
    -- The source's own stamp of the change, when it sends one.
    stamp text,
    received_at timestamptz NOT NULL DEFAULT now(),
    -- The sealed body of a source whose event carries the record, cleared
    -- once applied, discarded or expired, with the key it is sealed under.
    body bytea,
    body_key_id text,
    -- Whether the event is older than the entity's applied or pending one,
    -- so it moves no work item and keeps no body.
    superseded boolean NOT NULL DEFAULT false,
    CONSTRAINT inbound_event_dedupe_key UNIQUE (source_id, dedupe_key),
    CONSTRAINT inbound_event_body_check CHECK ((body IS NULL) = (body_key_id IS NULL))
);

CREATE INDEX inbound_event_received ON inbound_event (source_id, received_at);

COMMENT ON TABLE inbound_event IS 'Every verified inbound event, stored before its source is answered, once per de-duplication key. Our own design.';

CREATE TABLE inbound_work (
    source_id uuid REFERENCES inbound_source ON DELETE CASCADE,
    tenant text,
    entity_key text,
    -- Moved on by every accepted event and every replay.
    generation bigint NOT NULL CONSTRAINT inbound_work_generation_check CHECK (generation > 0),
    -- The change the newest event asks for.
    change text NOT NULL CONSTRAINT inbound_work_change_check CHECK (change IN ('upsert', 'remove')),
    state text NOT NULL CONSTRAINT inbound_work_state_check
        CHECK (state IN ('pending', 'running', 'done', 'parked', 'discarded')),
    attempts integer NOT NULL DEFAULT 0 CONSTRAINT inbound_work_attempts_check CHECK (attempts >= 0),
    next_at timestamptz NOT NULL DEFAULT now(),
    -- The replica running the item, and until when; both null otherwise.
    holder uuid,
    leased_until timestamptz,
    -- The newest source stamp applied.
    applied_stamp text,
    -- The event that moved the item to its generation, whose body the
    -- worker applies; null for a mark or a replay of a source without one.
    event_id uuid REFERENCES inbound_event ON DELETE SET NULL,
    -- The fixed reason a parked item carries; it names no value.
    park_reason text,
    -- Who settled a parked item, why and when.
    settled_by text,
    settled_reason text,
    settled_at timestamptz,
    updated_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (source_id, tenant, entity_key),
    CONSTRAINT inbound_work_held_check CHECK ((holder IS NULL) = (leased_until IS NULL)),
    CONSTRAINT inbound_work_parked_check CHECK ((state = 'parked') <= (park_reason IS NOT NULL))
);

CREATE INDEX inbound_work_due ON inbound_work (next_at) WHERE state = 'pending';
CREATE INDEX inbound_work_running ON inbound_work (leased_until) WHERE state = 'running';
CREATE INDEX inbound_work_parked ON inbound_work (source_id) WHERE state = 'parked';

COMMENT ON TABLE inbound_work IS 'One inbound work item per entity, claimed with SKIP LOCKED under a lease and rerun when an event arrives during the work. Our own design.';

CREATE TABLE inbound_slot (
    source_id uuid REFERENCES inbound_source ON DELETE CASCADE,
    tenant text,
    slot integer CONSTRAINT inbound_slot_slot_check CHECK (slot >= 0),
    holder uuid,
    leased_until timestamptz,
    PRIMARY KEY (source_id, tenant, slot),
    CONSTRAINT inbound_slot_held_check CHECK ((holder IS NULL) = (leased_until IS NULL))
);

COMMENT ON TABLE inbound_slot IS 'The remote calls in flight per inbound source and tenant across every replica, one leased row per call. Our own design.';

CREATE TABLE inbound_cursor (
    source_id uuid REFERENCES inbound_source ON DELETE CASCADE,
    tenant text,
    -- The poll's place in the source's reconcile list.
    poll integer CONSTRAINT inbound_cursor_poll_check CHECK (poll >= 0),
    -- The instant the next poll reads changes since.
    cursor timestamptz NOT NULL,
    polled_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (source_id, tenant, poll)
);

COMMENT ON TABLE inbound_cursor IS 'The since-cursor of each reconciliation poll per inbound source and tenant, moved only under the lease generation of the source''s recovery lane. Our own design.';

CREATE TABLE fhir_subscription_state (
    source_id uuid REFERENCES inbound_source ON DELETE CASCADE,
    -- The subscription reference as its notifications name it.
    subscription text,
    tenant text NOT NULL,
    -- The status code the newest notification carried.
    status text NOT NULL,
    -- The highest event number stored.
    last_event bigint NOT NULL DEFAULT 0 CONSTRAINT fhir_subscription_state_last_check
        CHECK (last_event >= 0),
    -- The event numbers no notification delivered, which $events recovers.
    gap_from bigint,
    gap_to bigint,
    handshake_at timestamptz,
    updated_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (source_id, subscription),
    CONSTRAINT fhir_subscription_state_gap_check
        CHECK ((gap_from IS NULL) = (gap_to IS NULL) AND (gap_from IS NULL OR gap_from <= gap_to))
);

COMMENT ON TABLE fhir_subscription_state IS 'Each FHIR Subscription an inbound source notifies: its status, the highest event stored and the gap $events recovers (Subscriptions R5 Backport STU 1.1.0, errors.html). Our own design.';
