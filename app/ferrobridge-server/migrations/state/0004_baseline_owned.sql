-- SPDX-FileCopyrightText: Cadasto B.V.
-- SPDX-License-Identifier: BUSL-1.1

-- The resources the facade keeps itself (docs/architecture.md section 4.6):
-- SMART App State (Basic) and SMART launch Tasks, each under its type and id
-- with its version. A deleted resource keeps its row with no body, so a later
-- request about its id is told it is gone; a version 0 row stands for an id
-- a conditional write holds before the resource exists. A conditional write
-- holds the row from its read to its write.
--
-- No specification governs the storage: our own design.

CREATE TABLE owned_resource (
    resource_type text NOT NULL,
    id text NOT NULL,
    version bigint NOT NULL CONSTRAINT owned_resource_version_check CHECK (version >= 0),
    body jsonb,
    PRIMARY KEY (resource_type, id)
);

COMMENT ON TABLE owned_resource IS 'The resources the facade keeps itself, versioned, a deletion leaving a row with no body. No specification governs the storage: our own design.';
