-- SPDX-FileCopyrightText: Cadasto B.V.
-- SPDX-License-Identifier: BUSL-1.1

-- The inbound sources (docs/architecture.md section 7, "The state store").
--
-- inbound_source is the head row of each source the inbound engine serves,
-- and inbound_source_version keeps every version with its committer, a
-- deletion included, so the record of who configured each flow outlives the
-- source (GDPR Art 30). inbound_tenant and inbound_route_mllp are derived
-- from the head document in the transaction that writes it. A write notifies
-- ferrobridge_sources with '<id>:<version>' when it commits. A source names
-- its secrets; their values never reach a row.
--
-- No specification governs a bridge's source registry: our own design.

CREATE TABLE inbound_source (
    id uuid PRIMARY KEY DEFAULT uuidv7(),
    name text NOT NULL CONSTRAINT inbound_source_name_key UNIQUE
        CONSTRAINT inbound_source_name_check CHECK (name ~ '^[A-Za-z0-9_.-]{1,128}$'),
    kind text NOT NULL CONSTRAINT inbound_source_kind_check
        CHECK (kind IN ('webhook', 'fhir-subscription', 'mllp')),
    enabled boolean NOT NULL,
    version bigint NOT NULL CONSTRAINT inbound_source_version_check CHECK (version > 0),
    document jsonb NOT NULL,
    updated_by text NOT NULL,
    updated_at timestamptz NOT NULL DEFAULT now()
);

COMMENT ON TABLE inbound_source IS 'The head version of each inbound source, written through the operator API. Our own design.';

CREATE TABLE inbound_source_version (
    source_id uuid,
    version bigint CONSTRAINT inbound_source_version_version_check CHECK (version > 0),
    name text NOT NULL,
    kind text NOT NULL,
    enabled boolean NOT NULL,
    -- Null on the version that records a deletion.
    document jsonb,
    deleted boolean NOT NULL DEFAULT false,
    committer text NOT NULL,
    committed_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (source_id, version),
    CONSTRAINT inbound_source_version_document_check CHECK ((document IS NULL) = deleted)
);

COMMENT ON TABLE inbound_source_version IS 'Every version of every inbound source with its committer, deletions included (GDPR Art 30). Our own design.';

CREATE TABLE inbound_tenant (
    source_id uuid REFERENCES inbound_source ON DELETE CASCADE,
    tenant text CONSTRAINT inbound_tenant_tenant_check CHECK (tenant <> ''),
    identifier_system text NOT NULL,
    -- The name of the secret the tenant's credentials are read from.
    credentials_ref text,
    enabled boolean NOT NULL,
    PRIMARY KEY (source_id, tenant)
);

COMMENT ON TABLE inbound_tenant IS 'The tenants of each inbound source, derived from its head document. Our own design.';

CREATE TABLE inbound_route_mllp (
    sending_application text,
    sending_facility text,
    source_id uuid NOT NULL REFERENCES inbound_source ON DELETE CASCADE,
    certificate_subject text,
    source_cidrs cidr[] NOT NULL DEFAULT '{}',
    PRIMARY KEY (sending_application, sending_facility),
    CONSTRAINT inbound_route_mllp_bound_check
        CHECK (certificate_subject IS NOT NULL OR cardinality(source_cidrs) > 0)
);

CREATE INDEX inbound_route_mllp_source ON inbound_route_mllp (source_id);

COMMENT ON TABLE inbound_route_mllp IS 'The MLLP routes by MSH-3 and MSH-4 to their source, each bound to a certificate subject or peer networks. Our own design.';
