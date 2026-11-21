---
name: postgresql-19
description: Every PostgreSQL the project tests against or documents is the latest release, PostgreSQL 19.0 since 2026-11-21, and 19 is the minimum the bridge starts against; owner rulings 2026-09-12 and 2026-11-21
metadata:
  type: project
---

<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

Owner ruling 2026-09-12: the CDM database in tests, the container harness, the
quickstart `compose.yaml` and the documentation use the latest PostgreSQL
release, pinned by tag and digest in `docs/VERSIONS.md`. The "PostgreSQL 16"
the second research pass wrote into the architecture and into #73 and #91 had
no decision behind it and is corrected.

Owner ruling 2026-11-21: every FerroHEALTH product moves to PostgreSQL 19 and
drops 18, with no upgrade path (nothing is in production). The CDM clients of
`omop-cdm` read `server_version_num` when they connect and refuse a server
below `connection::MINIMUM_SERVER_VERSION` (190000).

**How to apply:** one pin row in `docs/VERSIONS.md`, one constant in the
testkit that `versions.sh` checks, and every issue or page naming a PostgreSQL
version names that row. Bump it when a new release ships, as a pin change; a
new major also moves `MINIMUM_SERVER_VERSION`.
