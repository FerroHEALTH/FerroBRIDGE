-- SPDX-FileCopyrightText: Cadasto B.V.
-- SPDX-License-Identifier: BUSL-1.1

-- The runtime settings registers of the state store (docs/architecture.md
-- section 7, "The state store").
--
-- Operational policy the operator changes at runtime is a register: every
-- write appends one version with its committer, naming the version it
-- replaces, and notifies ferrobridge_settings in the same transaction, so a
-- change applies on every replica without a restart. A register with no
-- version holds its default. Rows are never updated or deleted.
--
-- No specification governs the register mechanics: our own design on family
-- precedent.

CREATE TABLE settings_register (
    register text NOT NULL CONSTRAINT settings_register_register_check
        CHECK (register IN ('audit-categories')),
    version bigint NOT NULL CONSTRAINT settings_register_version_check CHECK (version > 0),
    document jsonb NOT NULL,
    committer text NOT NULL,
    committed_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (register, version)
);

COMMENT ON TABLE settings_register IS 'One row per version of each runtime settings register, append-only. No specification governs the registers: our own design.';
