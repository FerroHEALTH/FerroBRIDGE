-- SPDX-FileCopyrightText: Cadasto B.V.
-- SPDX-License-Identifier: BUSL-1.1

-- The identity map (docs/architecture.md section 9).
--
-- Every row is written once and never changed: a writer inserts with ON
-- CONFLICT DO NOTHING and reads the row that stands, so two replicas agree.
-- A person's identifier and every Resource.identifier value are stored as
-- their HMAC-SHA-256 under the [state.seal] lookup key, and the person an
-- EHR was recorded for is sealed under a [state.seal] key, so no identifier
-- value is stored in clear (GDPR Art 5(1)(c) and Art 32).
--
-- No specification governs the storage: our own design.

CREATE TABLE identity_meta (
    singleton boolean PRIMARY KEY DEFAULT true CONSTRAINT identity_meta_singleton_check
        CHECK (singleton),
    -- The identity derivation the rows were written by.
    derivation integer NOT NULL,
    -- The fingerprint of the lookup key the digests were taken under.
    lookup_key bytea NOT NULL
);

CREATE TABLE person_ehr (
    namespace text NOT NULL,
    id_hmac bytea NOT NULL,
    ehr_id text NOT NULL,
    PRIMARY KEY (namespace, id_hmac)
);

CREATE TABLE ehr_subject (
    ehr_id text PRIMARY KEY,
    key_id text NOT NULL,
    person_sealed bytea NOT NULL
);

CREATE TABLE external_internal (
    resource_type text NOT NULL,
    external_id text NOT NULL,
    internal_id text NOT NULL,
    PRIMARY KEY (resource_type, external_id)
);

CREATE TABLE composition_binding (
    resource_type text NOT NULL,
    internal_id text NOT NULL,
    binding jsonb NOT NULL,
    PRIMARY KEY (resource_type, internal_id)
);

CREATE TABLE consumed_source (
    resource_type text NOT NULL,
    source_id text NOT NULL,
    -- The meta.versionId, or the empty string for none.
    version_id text NOT NULL,
    ehr_id text NOT NULL,
    versioned_object_uid text NOT NULL,
    internal_id text NOT NULL,
    context text NOT NULL,
    PRIMARY KEY (resource_type, source_id, version_id)
);

CREATE TABLE committed_source (
    resource_type text NOT NULL,
    source_id text NOT NULL,
    version_id text NOT NULL,
    ehr_id text NOT NULL,
    contribution_uid text NOT NULL,
    PRIMARY KEY (resource_type, source_id, version_id)
);

CREATE TABLE resource_identifier (
    resource_type text NOT NULL,
    -- The Identifier.system, or the empty string for none.
    system text NOT NULL,
    value_hmac bytea NOT NULL,
    internal_id text NOT NULL,
    PRIMARY KEY (resource_type, value_hmac, system, internal_id)
);

COMMENT ON TABLE person_ehr IS 'The EHR of each person, keyed by the HMAC of the person identifier. No specification governs the storage: our own design.';
COMMENT ON TABLE ehr_subject IS 'The person each EHR was first recorded for, sealed. No specification governs the storage: our own design.';
COMMENT ON TABLE resource_identifier IS 'Each Resource.identifier a committed resource carries, its value as an HMAC, for the identifier search. No specification governs the storage: our own design.';
