-- SPDX-FileCopyrightText: Cadasto B.V.
-- SPDX-License-Identifier: BUSL-1.1

-- The foundation of the state store (docs/architecture.md section 7, "The state
-- store").
--
-- This is the baseline of the state set: each area's tables are created
-- once, in their final shape, by one file per area, and the grants come
-- last. A file is edited in place until the release that ships it.
--
-- lane_lease gives each single-consumer lane one leader across replicas: a
-- replica takes a free or expired row and its generation moves on, and every
-- cursor write checks the generation it holds against the row, so a former
-- leader paused past its expiry cannot move a cursor.
--
-- No specification governs where the bridge keeps its state: our own design.

CREATE TABLE lane_lease (
    lane text PRIMARY KEY CONSTRAINT lane_lease_lane_check CHECK (lane <> ''),
    -- The replica holding the lease, and until when; both null while free.
    holder uuid,
    leased_until timestamptz,
    -- The fencing token, moved on by every take.
    generation bigint NOT NULL DEFAULT 0 CONSTRAINT lane_lease_generation_check
        CHECK (generation >= 0),
    CONSTRAINT lane_lease_held_check CHECK ((holder IS NULL) = (leased_until IS NULL))
);

COMMENT ON TABLE lane_lease IS 'One leader per single-consumer lane across replicas, fenced by a generation every cursor write checks. No specification governs leadership: our own design.';
