-- SPDX-FileCopyrightText: Cadasto B.V.
-- SPDX-License-Identifier: BUSL-1.1

-- The mapping revisions of the state store (docs/architecture.md sections 4.10
-- and 7).
--
-- A revision is the whole set of mapping files the operator console owns,
-- each by its wire path and the SHA-256 of its bytes; mapping_blob keeps the
-- bytes once per digest. mapping_running is one row naming the revision every
-- replica runs. A write that moves it notifies ferrobridge_mappings in the
-- same transaction, and each replica compiles that revision and swaps its
-- programs. The provisioned mapping files stay on disk and are in no revision;
-- the first start records the baseline, revision 1 with no file, so an apply of
-- it runs the provisioned tree alone.
--
-- No specification governs how the bridge stores its mapping revisions: our
-- own design.

CREATE TABLE mapping_blob (
    sha256 bytea PRIMARY KEY CONSTRAINT mapping_blob_sha256_check
        CHECK (octet_length(sha256) = 32),
    bytes bytea NOT NULL
);

CREATE TABLE mapping_revision (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    -- The revision that ran when this one was recorded.
    parent bigint REFERENCES mapping_revision (id),
    origin text NOT NULL CONSTRAINT mapping_revision_origin_check
        CHECK (origin IN ('baseline', 'save', 'apply', 'import')),
    author text NOT NULL,
    reason text NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    -- When it first ran; null for an import waiting for an apply.
    applied_at timestamptz,
    -- The revision an apply restored.
    source bigint REFERENCES mapping_revision (id),
    -- Every file, as {"<wire path>": "<lowercase hexadecimal SHA-256>"}.
    files jsonb NOT NULL CONSTRAINT mapping_revision_files_check
        CHECK (jsonb_typeof(files) = 'object'),
    CONSTRAINT mapping_revision_source_check CHECK ((origin = 'apply') = (source IS NOT NULL))
);

CREATE TABLE mapping_running (
    singleton boolean PRIMARY KEY DEFAULT true CONSTRAINT mapping_running_singleton_check
        CHECK (singleton),
    -- Null until the first revision runs.
    revision bigint REFERENCES mapping_revision (id)
);

-- The one row every change locks first, so changes from any replica run one
-- at a time.
INSERT INTO mapping_running (singleton, revision) VALUES (true, NULL);

COMMENT ON TABLE mapping_revision IS 'Every set of console mapping files the bridge recorded. No specification governs the store: our own design.';
COMMENT ON TABLE mapping_running IS 'The revision every replica runs; moving it notifies ferrobridge_mappings. No specification governs the store: our own design.';
