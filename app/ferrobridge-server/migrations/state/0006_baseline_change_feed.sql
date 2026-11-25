-- SPDX-FileCopyrightText: Cadasto B.V.
-- SPDX-License-Identifier: BUSL-1.1

-- The change feed (docs/architecture.md section 7, "The state store").
--
-- reader_watermark holds the cursor of each reader of the aql source: the
-- commit instant as the CDR wrote it and the version uids finished at that
-- instant. A write is fenced by the lane lease and only moves forward.
--
-- dead_letter holds the events a consumer parked after its bound of poisoned
-- passes, with the operator's settlement: who decided, retry or discard, and
-- why. poisoned_attempt counts the poisoned passes of each event still in
-- the feed.
--
-- No specification governs the change feed: our own design.

CREATE TABLE reader_watermark (
    reader text PRIMARY KEY CONSTRAINT reader_watermark_reader_check CHECK (reader <> ''),
    -- The instant as the CDR wrote it, which the next query binds unchanged.
    committed_at text NOT NULL,
    finished text[] NOT NULL,
    -- The lease generation of the last write.
    generation bigint NOT NULL
);

COMMENT ON TABLE reader_watermark IS 'The cursor of each reader of the aql change-feed source, moved forward under the lane lease. Our own design.';

CREATE TABLE dead_letter (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    reader text NOT NULL,
    -- The event's position in its stream, as canonical JSON.
    position text NOT NULL,
    -- The event's identifiers and provenance as the feed served them.
    event jsonb NOT NULL,
    reason text NOT NULL,
    attempts integer NOT NULL CONSTRAINT dead_letter_attempts_check CHECK (attempts > 0),
    parked_at timestamptz NOT NULL DEFAULT now(),
    replay_requested_at timestamptz,
    settled_at timestamptz,
    decided_by text,
    decision text CONSTRAINT dead_letter_decision_check CHECK (decision IN ('retry', 'discard')),
    decision_reason text,
    CONSTRAINT dead_letter_position_key UNIQUE (reader, position),
    CONSTRAINT dead_letter_decided_check CHECK (
        (decision IS NULL) = (decided_by IS NULL) AND (decision IS NULL) = (decision_reason IS NULL)
    )
);

CREATE INDEX dead_letter_open ON dead_letter (reader, id) WHERE settled_at IS NULL;

COMMENT ON TABLE dead_letter IS 'The change-feed events parked after their bound of poisoned passes, with the operator settlement that retried or discarded each. Our own design.';

CREATE TABLE poisoned_attempt (
    reader text,
    position text,
    attempts integer NOT NULL CONSTRAINT poisoned_attempt_attempts_check CHECK (attempts > 0),
    PRIMARY KEY (reader, position)
);

COMMENT ON TABLE poisoned_attempt IS 'The poisoned passes of each change-feed event not yet parked. Our own design.';
