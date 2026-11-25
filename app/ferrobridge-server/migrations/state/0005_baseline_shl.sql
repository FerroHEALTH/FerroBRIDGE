-- SPDX-FileCopyrightText: Cadasto B.V.
-- SPDX-License-Identifier: BUSL-1.1

-- The SMART Health Links (docs/architecture.md section 4.6).
--
-- A link's manifest id, its handle and each file location's token are stored
-- as their SHA-256, and the handle is also sealed under a [state.seal] key
-- so its owner can list it; none is stored in clear. Each file is its compact
-- JWE, never a plaintext. Every change of one link holds its row, so replicas
-- count its answers and passcode attempts exactly and serve a location once
-- (SMART Health Cards and Links 1.0.0, links-specification). A link's files
-- and locations leave with it.
--
-- No specification governs the storage: our own design.

CREATE TABLE shl_link (
    id_hash bytea PRIMARY KEY,
    handle_hash bytea NOT NULL CONSTRAINT shl_link_handle_hash_key UNIQUE,
    handle_key_id text NOT NULL,
    handle_sealed bytea NOT NULL,
    owner text NOT NULL,
    -- The expiry, in epoch seconds, which the sweep reads.
    expires bigint NOT NULL,
    -- The rules, the passcode hash, the counts and the file entries.
    record jsonb NOT NULL
);
CREATE INDEX shl_link_owner ON shl_link (owner);
CREATE INDEX shl_link_expires ON shl_link (expires);

CREATE TABLE shl_file (
    id_hash bytea NOT NULL REFERENCES shl_link (id_hash) ON DELETE CASCADE,
    position integer NOT NULL,
    jwe text NOT NULL,
    PRIMARY KEY (id_hash, position)
);

CREATE TABLE shl_location (
    token_hash bytea PRIMARY KEY,
    id_hash bytea NOT NULL REFERENCES shl_link (id_hash) ON DELETE CASCADE,
    position integer NOT NULL,
    -- When the location lapses, in epoch seconds.
    lapses bigint NOT NULL
);
CREATE INDEX shl_location_link ON shl_location (id_hash);
CREATE INDEX shl_location_lapses ON shl_location (lapses);

COMMENT ON TABLE shl_link IS 'SMART Health Links, the manifest id and the handle as SHA-256, the handle also sealed. No specification governs the storage: our own design.';
COMMENT ON TABLE shl_location IS 'The file locations of the links, each token as its SHA-256, served once. No specification governs the storage: our own design.';
