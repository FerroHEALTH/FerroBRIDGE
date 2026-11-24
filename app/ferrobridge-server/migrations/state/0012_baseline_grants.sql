-- SPDX-FileCopyrightText: Cadasto B.V.
-- SPDX-License-Identifier: BUSL-1.1

-- The grants of the state store (docs/architecture.md section 7, "The state
-- store").
--
-- The bridge owns every relation it creates. No table is readable by
-- PUBLIC. When the read-only role ferrobridge_state_reader exists, it reads
-- every relation of the store's schema, the migration bookkeeping included,
-- and the default privileges extend the same grant to every table a later
-- file creates; without the role the block is skipped with a NOTICE.
--
-- No specification governs database roles: our own design.

DO $$
DECLARE
    store_schema text := current_schema();
BEGIN
    EXECUTE format('REVOKE ALL ON ALL TABLES IN SCHEMA %I FROM PUBLIC', store_schema);
    IF EXISTS (SELECT FROM pg_roles WHERE rolname = 'ferrobridge_state_reader') THEN
        EXECUTE format('GRANT USAGE ON SCHEMA %I TO ferrobridge_state_reader', store_schema);
        EXECUTE format('GRANT SELECT ON ALL TABLES IN SCHEMA %I TO ferrobridge_state_reader',
            store_schema);
        EXECUTE format('ALTER DEFAULT PRIVILEGES IN SCHEMA %I GRANT SELECT ON TABLES TO ferrobridge_state_reader',
            store_schema);
    ELSE
        RAISE NOTICE 'role ferrobridge_state_reader does not exist; its grants are skipped';
    END IF;
END
$$;
