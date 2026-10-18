<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# Pinned version matrix

This file is the single source of truth for every version pin in FerroBRIDGE.
When it and a file that repeats a pin disagree, that is drift. Fix the
disagreement; never let either side silently win.
`scripts/checks/versions.sh` enforces the cross-file agreement it can reach and
skips loudly for the files that do not exist yet, so the guard is useful on a
tree with no Cargo workspace and grows teeth as files appear.

No specification governs this file; it is FerroBRIDGE's own design.

## Specifications

The ground for each pin is the pin table in `docs/architecture.md` §2, which
records why the value is what it is. The guard compares the first token of each
`Pin` cell below with the first token of the same row there.

| Item | Pin | Repeated in |
|---|---|---|
| FHIRconnect | v1.0.0 | `docs/architecture.md`, later the `fhirconnect-*` crates |
| FHIR | R4 (4.0.1) | `docs/architecture.md`, later the facade and terminology crates |
| OMOCL | v1.0.0 | `docs/architecture.md`, later the `omocl-*` crates |
| OMOP CDM | v5.4 | `docs/architecture.md`, `tools/omop-cdm-codegen` and the `omop-cdm` crate constant, and its embedded DDL |
| openEHR ITS-REST | 1.1.0 | `docs/architecture.md`, the `ferrobridge-server` CDR module constant |

## Corpora and machine-readable inputs

A corpus is pinned by commit or immutable tag, never by a moving tag or a
`latest` URL, and vendored by a committed `scripts/vendor/*.sh` with a
`PROVENANCE.md` (`.claude/rules/vendored-inputs.md`). Each script below reads
its pin from this table, and `scripts/checks/versions.sh` reads each vendored
`PROVENANCE.md` back and fails when it names a different commit or tag.

| Item | Pin | Repeated in |
|---|---|---|
| FHIRconnect REST API chapter (draft, unmerged) | `SevKohler/FHIRconnect-spec` pull request #93 at head `2bf2a2fe91bae2ae659cda1665826567ea81b4af` | `docs/architecture.md` §2 and §4.7, `scripts/vendor/fhirconnect.sh`, `docs/specs/fhirconnect/draft-rest-api/PROVENANCE.md`, `crates/fhirconnect/tests/fixtures/draft-rest-api/PROVENANCE.md` |
| FHIRconnect specification source | `SevKohler/FHIRconnect-spec` commit `195b07fdb4c78da0432fdd1e9dbd127b81be6165` | `docs/architecture.md` §2, `scripts/vendor/fhirconnect.sh`, `docs/specs/fhirconnect/PROVENANCE.md` |
| FHIRconnect mapping library (corpus, never an oracle) | `SevKohler/FHIRconnect-mapping-lib` commit `6bd4c19a2f96821c04fbeed3c6f6c190fd85825b` | `docs/architecture.md` §2, `scripts/vendor/fhirconnect-mapping-lib.sh`, `docs/specs/fhirconnect-mapping-lib/PROVENANCE.md` |
| OMOCL corpus | `SevKohler/OMOCL` commit `c082db8ed81a062a574a2c058366045f600c2ed7` (grammar `OMOCL/v1.0.0`; the git tag `v1.0.0` carries pre-grammar files) | `docs/architecture.md` §2, `scripts/vendor/omocl.sh`, `docs/specs/omocl/PROVENANCE.md` |
| OMOP CDM definitions and PostgreSQL DDL | `OHDSI/CommonDataModel` tag `v5.4.3` | `docs/architecture.md` §2, `scripts/vendor/omop-cdm.sh`, `docs/specs/omop-cdm/PROVENANCE.md`, `tools/omop-cdm-codegen` and the banner of every file it emits |
| OHDSI Data Quality Dashboard | `OHDSI/DataQualityDashboard` tag `v2.9.0` | `docs/architecture.md` §2 and §11, `scripts/vendor/dqd.sh`, `docs/specs/dqd/PROVENANCE.md`, `tools/omop-cdm-codegen` and the banner of the `dqd` module it emits, the `DQD_TAG` constant of `omop-cdm` |
| openEHR ITS-REST OpenAPI | `openEHR/specifications-ITS-REST` tag `Release-1.1.0`, modules EHR, Query, Definition, and the Simplified Formats and Simplified Data Template sources | `docs/architecture.md` §2, `scripts/vendor/its-rest.sh`, `docs/specs/its-rest/PROVENANCE.md` |
| KDS Diagnose operational template (fixture) | `openFHIR/openfhir` commit `5e4d68007518fcddae1907b333a86f495791aa53` path `core/src/test/resources/kds/diagnose/KDS_Diagnose.opt` sha256 `752483d90c4ba0f0d1e67baacceb67c1f6f698607823f2eebaa876f4f32bd870` | `scripts/vendor/kds-diagnose-opt.sh`, `tools/ferrobridge-testkit/fixtures/opt/kds/PROVENANCE.md` |
| KDS Person operational template (fixture) | `openFHIR/openfhir` commit `5e4d68007518fcddae1907b333a86f495791aa53` path `core/src/test/resources/kds/person/KDS_Person.opt` sha256 `ef559dde455a36b6282a6f4b2e3ecd4f2e6130acd8be638ce73088739e0bf6c8` | `scripts/vendor/kds-person-opt.sh`, `tools/ferrobridge-testkit/fixtures/opt/kds-person/PROVENANCE.md` |
| HL7 v2 definitions (v2ig source of truth, never committed) | `HL7/v2ig` commit `3adcdbfff654ccbff5cd33aa34bd909a087e8e19` path `input/sourceOfTruth` files `1694` digest `9a1fb2b974b69f575bcd9b30fa66d2ff7f13ef7869c1350eca7462067e974f93` | `scripts/vendor/v2ig.sh`, `tools/fhir-codegen/vendor/hl7-v2ig/PROVENANCE.md`, the `codegen-drift` job of `.github/workflows/ci.yml` (its cache key) |
| HL7 v2 legacy tables (NIST IGAMT export, never committed) | `usnistgov/igamt-hl7Tools-service` commit `83322dffdac18cb129a1bd80149f5439cf5686fd` path `src/main/resources/hl7db` files `169` digest `967715c682b43383ba28894e171fd1e5c2a5980b16b5676a1c60f3fd0a5c2c92` | `scripts/vendor/v2-legacy.sh`, `tools/fhir-codegen/vendor/hl7-v2-legacy/PROVENANCE.md`, the `codegen-drift` and `test` jobs of `.github/workflows/ci.yml` and the coverage job of `.github/workflows/sonar.yml` (their cache key) |
| HL7 v2 samples: Microsoft FHIR-Converter | `microsoft/FHIR-Converter` commit `70fd328e05019142f616a660cf65c6034baaa3c9` files `272` digest `8ba5602d3efc0f1161c1329887e3a9178983e1d2ed76f4353968aacd90c1420c` | `scripts/vendor/hl7v2-samples.sh`, `crates/ferrobridge-hl7v2/vendor/fhir-converter/PROVENANCE.md` |
| HL7 v2 samples: CDC ReportStream data tests | `CDCgov/prime-reportstream` commit `5f58b4c574fa64a70b362f7881f035ec6110a933` files `732` digest `857645f53e03efed22325254f1757773c3812f58f3b9b13f8cda796ba5415f3c` | `scripts/vendor/hl7v2-samples.sh`, `crates/ferrobridge-hl7v2/vendor/reportstream/PROVENANCE.md` |
| HL7 v2 samples: HL7 v2-to-FHIR benchmark messages | `HL7/v2-to-fhir` commit `873b331b3890c8bc5d62ef9b4dabb41801aac70d` files `15` digest `dd2831ec1bdef559421849bbe171213e8aa0761eb0105b362c969f4ebc23d8e6` | `scripts/vendor/hl7v2-samples.sh`, `crates/ferrobridge-hl7v2/vendor/v2-to-fhir/PROVENANCE.md` |
| HL7 v2 samples: NIST LRI (build time, never committed) | `usnistgov/hit-mu-tools-resource-bundles` commit `060f7af14daa359937f326c361bb9610b7a8ba48` (branch `lri-r2`) files `117` digest `666bafb90db5eeec7f727617fcbbe51b778065f93bc726f2746f524d70de7ec4` | `scripts/vendor/hl7v2-samples.sh --build-time`, `crates/ferrobridge-hl7v2/vendor/nist/PROVENANCE.md`, the `test` and `conformance` jobs of `.github/workflows/ci.yml` (their cache key) |
| HL7 v2 samples: NIST LOI (build time, never committed) | `usnistgov/hit-mu-tools-resource-bundles` commit `2c5e502787cd5e0fe0f347a7c00d5507a9f8035a` (branch `loi-r1`) files `65` digest `712d023d5a7ceb21ba520f513490fe1a29603436ed4756f48646d5922702f549` | `scripts/vendor/hl7v2-samples.sh --build-time`, `crates/ferrobridge-hl7v2/vendor/nist/PROVENANCE.md`, the same cache key |
| HL7 v2 samples: NIST syndromic surveillance (build time, never committed) | `usnistgov/hit-mu-tools-resource-bundles` commit `0d7a2c7b714188878b030068a94b94d1db6cb30a` (branch `ss-r2`) files `24` digest `aca07c08f2718139456f712ce8e45fc42cadba9c9fed665fd7ef5465c30af4c6` | `scripts/vendor/hl7v2-samples.sh --build-time`, `crates/ferrobridge-hl7v2/vendor/nist/PROVENANCE.md`, the same cache key |
| HL7 v2 samples: AIRA MQE (build time, never committed) | `immregistries/mqe` commit `8b8a4274e1830cdb78ff5c8627763e1625caeac7` path `examples` files `9` digest `1fbda5ede288bdaf9ea53b16ded2149f11627cf4b0b39d2837299ab5803972b7` | `scripts/vendor/hl7v2-samples.sh --build-time`, `crates/ferrobridge-hl7v2/vendor/aira-mqe/PROVENANCE.md`, the same cache key |
| FHIRPath test suite (R4) | `FHIR/fhir-test-cases` commit `9b57d9564f3f7e0343c020942155df13e56ef7a7` path `r4/fhirpath/tests-fhir-r4.xml` and the inputs it names files `13` digest `af9848ea9d046b288289adf64f199d4b4d0bdfebf6c000496b6e7f69a4b0dc48` | `scripts/vendor/fhirpath-tests.sh`, `docs/specs/fhirpath-tests/PROVENANCE.md`, `conformance/fhirpath-r4/PROVENANCE.md` |
| HL7 v2+ licence page | `HL7/v2plus` commit `1a8fbb7e198047d71b9b13ceb582aaded4fa19f3` path `license.html` sha256 `48e2d1108ab38faafb9993342a9626b52d673ce2e683ef123716950564d76efd` | `scripts/vendor/v2ig.sh` and `scripts/vendor/v2-legacy.sh` (quoted verbatim in `tools/fhir-codegen/vendor/hl7-v2ig/PROVENANCE.md` and `tools/fhir-codegen/vendor/hl7-v2-legacy/PROVENANCE.md`) |
| UCUM essence | `ucum-org/ucum` tag `v2.2` commit `d060c6d3dfe3b0deb196c190b8024d95385e0b92` path `ucum-essence.xml` sha256 `dfccea1b5dc284245ebae97edd1dc03c45864da4e87df55bc9851797b4fd0b61` | `scripts/vendor/ucum.sh`, `crates/ferrobridge-fhirpath/vendor/ucum/PROVENANCE.md` (embedded whole in the `ferrobridge-fhirpath` crate) |
| FerroPULSE report envelope v2 | `FerroHEALTH/FerroPULSE` commit `322fcecd54144e9194bd9b0d7a240d2635633b10` path `schemas/report-envelope.v2.json` sha256 `09048000b428cbb010d165b52a4579f60672c471f167a3b58410226dddfec9c2`, with the examples under `schemas/examples/report-envelope.v2/` | `scripts/vendor/ferropulse-schemas.sh`, `app/ferrobridge-server/tests/fixtures/ferropulse/PROVENANCE.md` (the usage report contract is `docs/report-v2.md` at the same commit) |

## openEHR model crates (crates.io)

The openEHR RM, the OPT 1.4 and Web Template codecs, the canonical JSON and
FLAT codecs, the ITS-REST data types and the AQL parser come from four
published crates, consumed by version like any other dependency
(`docs/architecture.md` §3). The minor line is pinned here; the manifest
carries the exact patch.

| Item | Pin | Repeated in |
|---|---|---|
| `openehr-base` | 0.0.87 | `docs/architecture.md`, later the root `Cargo.toml` `[workspace.dependencies]` |
| `openehr-rm` | 0.0.87 | `docs/architecture.md`, the root `Cargo.toml` `[workspace.dependencies]` |
| `openehr-its` | 0.0.87 | `docs/architecture.md`, the root `Cargo.toml` `[workspace.dependencies]` |
| `openehr-sdt` | 0.0.87 | `docs/architecture.md`, the root `Cargo.toml` `[workspace.dependencies]` (the Simplified Data Template engines, split out of `openehr-its` at 0.0.68) |
| `openehr-query` | 0.0.87 | `docs/architecture.md`, the root `Cargo.toml` `[workspace.dependencies]` |
| `openehr-am` | 0.0.87 | `docs/architecture.md`, the root `Cargo.toml` `[workspace.dependencies]` (the AOM2 OPT2 types an ADL 2 template decodes into) |
| `openehr-adl` | 0.0.87 | the root `Cargo.toml` `[workspace.dependencies]`, test-only: the ADL 2 test fixtures of `openehr-mapping-core` are compiled from their `.adls` sources |

## Third-party crates

A crate reaches this table only when `docs/architecture.md` §2 grounds the
choice by name, so the ground and the pin never drift apart. Everything else
stays in the root `Cargo.toml` `[workspace.dependencies]` alone, per §Rust
dependency pins below.

| Item | Pin | Repeated in |
|---|---|---|
| `serde-saphyr` | 1.3.0 | `docs/architecture.md` §7, the root `Cargo.toml` `[workspace.dependencies]` |
| `jsonschema` | 0.57.0 | `docs/architecture.md` §7, the root `Cargo.toml` `[workspace.dependencies]` (the FHIRconnect mapping schemas) |
| `reqwest` | 0.13.5 | `docs/architecture.md` §7, the root `Cargo.toml` `[workspace.dependencies]` |
| `backon` | 1.6.0 | `docs/architecture.md` §7, the root `Cargo.toml` `[workspace.dependencies]` |
| `url` | 2.5.8 | the root `Cargo.toml` `[workspace.dependencies]` |
| `http` | 1.5.0 | the root `Cargo.toml` `[workspace.dependencies]` |
| `secrecy` | 0.10.3 | the root `Cargo.toml` `[workspace.dependencies]` |
| `tracing` | 0.1.44 | the root `Cargo.toml` `[workspace.dependencies]` |
| `tracing-subscriber` | 0.3.23 | the root `Cargo.toml` `[workspace.dependencies]` (the server console) |
| `axum` | 0.8.9 | `docs/architecture.md` §7, the root `Cargo.toml` `[workspace.dependencies]` |
| `tower-http` | 0.7.1 | `docs/architecture.md` §7, the root `Cargo.toml` `[workspace.dependencies]` |
| `tower` | 0.5.3 | the root `Cargo.toml` `[workspace.dependencies]` |
| `toml` | 1.1.6 | the root `Cargo.toml` `[workspace.dependencies]` (the server configuration file) |
| `uuid` | 1.26.1 | the root `Cargo.toml` `[workspace.dependencies]` (the minted request id) |
| `redb` | 4.3.0 | `docs/architecture.md` §7, the root `Cargo.toml` `[workspace.dependencies]` (the facade identity store) |
| `jiff` | 0.2.37 | `docs/architecture.md` §7, the root `Cargo.toml` `[workspace.dependencies]` (the bridge's own timestamps) |
| `futures-core`, `futures-util` | 0.3.34 | the root `Cargo.toml` `[workspace.dependencies]` |
| `tokio` | 1.53.1 | the root `Cargo.toml` `[workspace.dependencies]` |
| `wiremock` | 0.6.5 | the root `Cargo.toml` `[workspace.dependencies]` (tests only) |
| `testcontainers` | 0.28.0 | the root `Cargo.toml` `[workspace.dependencies]` (tests only) |
| `tokio-postgres` | 0.7.18 | the root `Cargo.toml` `[workspace.dependencies]` (the CDM writer's client, and the tests that read the CDM back) |
| `tokio-postgres-rustls` | 0.14.0 | the root `Cargo.toml` `[workspace.dependencies]` (the CDM writer's TLS connector, on the aws-lc-rs provider) |
| `rustls` | 0.23.45 | the root `Cargo.toml` `[workspace.dependencies]` (the CDM writer's certificate checks after `sslmode`) |
| `webpki-roots` | 1.0.9 | the root `Cargo.toml` `[workspace.dependencies]` (the root set both CDM clients trust when no CA is configured) |
| `sqlx` | 0.9.0 | the root `Cargo.toml` `[workspace.dependencies]` (the concept resolver; the `sqlx-cli` row carries the same version) |
| `rust_decimal` | 1.43.0 | the root `Cargo.toml` `[workspace.dependencies]` (the exact decimal of the OMOCL engine; MIT) |
| `tokio-util` | 0.7.19 | `docs/architecture.md` §7, the root `Cargo.toml` `[workspace.dependencies]` (the MLLP frame codec of the HL7 v2 face) |
| `bytes` | 1.12.1 | `docs/architecture.md` §7, the root `Cargo.toml` `[workspace.dependencies]` (the MLLP codec's buffer) |
| `encoding_rs` | 0.8.42 | `docs/architecture.md` §7, the root `Cargo.toml` `[workspace.dependencies]` (the MSH-18 character sets) |
| `logos` | 0.16.1 | `docs/architecture.md` §7, the root `Cargo.toml` `[workspace.dependencies]` (the v2-to-FHIR condition and target lexers) |
| `chumsky` | 0.13.0 | `docs/architecture.md` §7, the root `Cargo.toml` `[workspace.dependencies]` (the v2-to-FHIR condition and target parsers) |

## Container images (the end-to-end lane)

The end-to-end lane starts real servers in containers behind the
`FERROBRIDGE_E2E` gate (`docs/ci-cd.md`). Every image is pinned by tag AND by
the digest of its image index, because a tag can be re-pointed and a digest
cannot; the digest is what Docker resolves. The one place each pin is written
in code is the matching constant in
`tools/ferrobridge-testkit/src/containers.rs`, and
`scripts/checks/versions.sh` compares the two.

| Item | Pin | Repeated in |
|---|---|---|
| PostgreSQL image | `postgres:18.6@sha256:4ef4dbc939d61acea57712655ddb4b4ab27419c913f94cca0cd57cb3ea3c2280` | the `POSTGRES` constant in `tools/ferrobridge-testkit/src/containers.rs`, the `sqlx-offline` service in `.github/workflows/ci.yml` |
| FerroEHR CDR image | `ghcr.io/ferrohealth/ferroehr:4.3.6@sha256:cd29d6526084c84e9a7fe2c9d7058bd4fe67abbfab51d07b124d84dc9805e5bb` | the `CDR` constant in `tools/ferrobridge-testkit/src/containers.rs` |
| FerroEHR CDR database image | `ghcr.io/ferrohealth/ferroehr-postgres:4.3.6@sha256:fc93ccffba5e1dc266169e3d22b7a2c57cbe35e1266b42ff6c819ca323c204c3` | the `CDR_POSTGRES` constant in `tools/ferrobridge-testkit/src/containers.rs` |
| FerroTERM terminology server image | `ghcr.io/ferrohealth/ferroterm:0.1.6@sha256:7ec4c9622e88e3028faf7c74a2384fb02e57dc3163c282b21274121a26aa3d5b` | the `TERMINOLOGY` constant in `tools/ferrobridge-testkit/src/containers.rs` |

The CDR runs against its own published database image rather than the plain
PostgreSQL above: that image is built from `postgres:18.6` and carries the
login role, the schemas and the extensions the CDR's migrations expect to find,
which a bare PostgreSQL does not have.

### The shipped image and the quickstart

`docker/Dockerfile` and `docker/viewer/Dockerfile` build on one more image,
pinned by the digest of its image index, which is what Docker resolves for a
multi-platform build. `scripts/checks/versions.sh` reads both `FROM` lines
back against this row, and Dependabot (`docker`, over `/docker` and
`/docker/viewer`) proposes the bumps.

| Item | Pin | Repeated in |
|---|---|---|
| Container base image | `gcr.io/distroless/static-debian13:nonroot@sha256:e2e927ec666bae08560abb3c55d0659eceabb657f56b6782ab500a9fc7f555e3` | the `FROM` of `docker/Dockerfile` and of `docker/viewer/Dockerfile`, and each one's `org.opencontainers.image.base.name` label without the digest |

The quickstart `compose.yaml` pulls `ghcr.io/ferrohealth/ferrobridge` and,
under its `viewer` profile, `ghcr.io/ferrohealth/ferrobridge-viewer` at the
release it shipped with, so both tags are the product version below rather
than pins of their own, and the guard compares them. Its CDM service runs the same
`postgres:18.6` image the row above pins for the end-to-end lane, so a
PostgreSQL bump moves one row and both readers follow it.

The terminology server needs no database. It reads the synthetic `CodeSystem`,
`ValueSet` and `ConceptMap` under
`tools/ferrobridge-testkit/fixtures/terminology` from a read-only bind mount
named by `FERROTERM_CODESYSTEMS`, and serves each FHIR release under its own
path prefix (`/r4`, `/r4b`).

## FHIR packages (the `fhir-types` generator input)

The FHIR model is generated by `tools/fhir-codegen` from the HL7 FHIR packages
below, vendored verbatim from the FHIR package registry by
`scripts/vendor/fhir-packages.sh`, which reads its pins from this table. Owner
decision 2026-09-05: the crate and its generator move here from the sibling
terminology server, which then consumes the crate from crates.io. The first
increment of #72 landed both trees, so the `PROVENANCE.md` paths below exist
and the `codegen-drift` CI job reads the packages on every run.
`hl7.fhir.uv.v2mappings` is no input of `fhir-types`: it carries the
v2-to-FHIR ConceptMaps for the HL7 v2 face (#251), and the script takes the
`LICENSE` of its source repository beside it because that licence differs
from the package's own. `scripts/checks/versions.sh` reads the `Version` line
of every `PROVENANCE.md` below back against its row.
`scripts/checks/pin-freshness.sh` reads each version against the registry's
`latest`.

| Item | Pin | Repeated in |
|---|---|---|
| `hl7.fhir.r3.core` | 3.0.2 | `tools/fhir-codegen/vendor/hl7.fhir.r3.core/PROVENANCE.md` |
| `hl7.fhir.r4.core` | 4.0.1 | `tools/fhir-codegen/vendor/hl7.fhir.r4.core/PROVENANCE.md` |
| `hl7.fhir.r4b.core` | 4.3.0 | `tools/fhir-codegen/vendor/hl7.fhir.r4b.core/PROVENANCE.md` |
| `hl7.fhir.r5.core` | 5.0.0 | `tools/fhir-codegen/vendor/hl7.fhir.r5.core/PROVENANCE.md` |
| `hl7.fhir.r6.core` | 6.0.0-ballot5 | `tools/fhir-codegen/vendor/hl7.fhir.r6.core/PROVENANCE.md` |
| `hl7.terminology` (THO) | 7.3.0 | `tools/fhir-codegen/vendor/hl7.terminology/PROVENANCE.md` |
| `hl7.fhir.uv.v2mappings` (the v2-to-FHIR IG) | 1.0.0 | `tools/fhir-codegen/vendor/hl7.fhir.uv.v2mappings/PROVENANCE.md` (FHIR 4.0.1; the package declares CC0-1.0, its source repository `HL7/v2-to-fhir` Apache-2.0) |

## FHIR examples packages (the conformance corpora, build time, never committed)

HL7's examples package per FHIR version is the corpus of the five `fhir-types`
conformance tests and of the R4 facade test (#373, #454). Each is the version
of the core package the table above pins, from the same registry: STU3, R4,
R4B and R5 from packages.fhir.org, the R6 ballot from packages2.fhir.org. At
about 680 MB
unpacked they are fetched at build time by
`scripts/vendor/fhir-packages.sh --build-time` into the ignored
`tools/ferrobridge-testkit/vendor/<package>/package/`, and only each
`PROVENANCE.md` is committed. The Pin cell is the version, the file count and
the tree digest the script checks a fetch or a restored cache against;
`scripts/checks/versions.sh` reads the version and the digest back from each
`PROVENANCE.md`, and `scripts/checks/pin-freshness.sh` reads each version
against the registry's `latest`.

| Item | Pin | Repeated in |
|---|---|---|
| `hl7.fhir.r3.examples` | 3.0.2 files `8289` digest `e35e340680f49f88f6f91808f2691bce0bc20cd3faf2794edd2b98ceddb5803a` | `tools/ferrobridge-testkit/vendor/hl7.fhir.r3.examples/PROVENANCE.md`, the same cache key |
| `hl7.fhir.r4.examples` | 4.0.1 files `5311` digest `51f5c093f9f197fcab683ac1bc3380e44b1c7e37c6c74baeefff7fa3eb8b7c1e` | `tools/ferrobridge-testkit/vendor/hl7.fhir.r4.examples/PROVENANCE.md`, the `test` and `conformance` jobs of `.github/workflows/ci.yml` and the coverage job of `.github/workflows/sonar.yml` (their cache key) |
| `hl7.fhir.r4b.examples` | 4.3.0 files `2842` digest `7c42275e73bff53eef52c9f48e3bd82d3eaef66275ac00975a259828086be6b9` | `tools/ferrobridge-testkit/vendor/hl7.fhir.r4b.examples/PROVENANCE.md`, the same cache key |
| `hl7.fhir.r5.examples` | 5.0.0 files `2824` digest `afddb711d22bbf8c45158cb4be0a8936ecd458df1a2a99cf5debd5274a7bee47` | `tools/ferrobridge-testkit/vendor/hl7.fhir.r5.examples/PROVENANCE.md`, the same cache key |
| `hl7.fhir.r6.examples` | 6.0.0-ballot5 files `2488` digest `4654c0fa42180b2ef3abec208458482cb1092fe3c63ef4ec4661a3feb50a7b3f` | `tools/ferrobridge-testkit/vendor/hl7.fhir.r6.examples/PROVENANCE.md`, the same cache key |

## Profile packages (the profile validator's package set, build time, never committed)

The R4 profile validator runs offline against the packages below, fetched
verbatim from the FHIR package registry at build time by
`scripts/vendor/profile-packages.sh` into the ignored
`docs/specs/fhir-packages/<package>#<version>/package/`, which reads its pins
from this table (#435). At about 570 MB unpacked they are never committed;
only each `PROVENANCE.md` is, and CI restores the trees from a cache keyed on
`scripts/vendor/profile-packages.sh --cache-key`. The three roots are `hl7.fhir.eu.eps` 1.0.0-ballot (its
one published version), `hl7.fhir.eu.base` 2.0.1 and `hl7.fhir.uv.ips` 2.0.0;
every other row is in the set because a vendored package's `package.json`
`dependencies` names it at that version. A package two dependents name at
different versions has a row per version: EPS 1.0.0-ballot depends on EU Base
2.0.0, so both 2.0.0 and 2.0.1 are here, and the five `hl7.terminology.r4`
rows are the five versions the set names. Every package depends on
`hl7.fhir.r4.core` 4.0.1, which has no row here: the set reads the tree the
FHIR packages table above already vendors at
`tools/fhir-codegen/vendor/hl7.fhir.r4.core/`. Each pin is the version, then
the file count and tree digest of the unpacked tree, which the script checks
a download or a cache-restored tree against. `--check` works offline: it reads
every `PROVENANCE.md` against its row, verifies every tree on disk, refuses a
tree with no row, and asserts the closure is complete: every `dependencies`
entry of every package is itself pinned. It reads the dependencies from a
fetched `package.json`, or from the `Declared dependencies` line `--stamp`
wrote into the `PROVENANCE.md` when the tree is not on disk.
`scripts/checks/versions.sh` reads the `Version`, `Files` and digest lines of
every `PROVENANCE.md` back against its row.

No context mapping targets these packages yet (`docs/architecture.md` §4.8
names the candidates and their order; the mapping library's own contexts
target the MII Kerndatensatz modules at 2025.0.0).

| Item | Pin | Repeated in |
|---|---|---|
| `hl7.fhir.eu.eps` | 1.0.0-ballot files `76` digest `d8fe50b7359f698ce513821235502dd5bf43ad2e4e70c25a4dbe12d0b8b39f2e` | `docs/specs/fhir-packages/hl7.fhir.eu.eps#1.0.0-ballot/PROVENANCE.md` (a root) |
| `hl7.fhir.eu.base` | 2.0.1 files `75` digest `05d7ea672cac41eaa5efb7806c6ce9e4638b1604a811a0aea78f84685efff91d` | `docs/specs/fhir-packages/hl7.fhir.eu.base#2.0.1/PROVENANCE.md` (a root) |
| `hl7.fhir.eu.base` | 2.0.0 files `77` digest `0b2e85a498e4f0df5ed8f122d97737c824250cb247792db9e392f4186245ea01` | `docs/specs/fhir-packages/hl7.fhir.eu.base#2.0.0/PROVENANCE.md` |
| `hl7.fhir.uv.ips` | 2.0.0 files `154` digest `86dbd916617c60642e0296c33bea0d1aa6497c4ca46a2db285a0e5c975f28316` | `docs/specs/fhir-packages/hl7.fhir.uv.ips#2.0.0/PROVENANCE.md` (a root) |
| `hl7.fhir.uv.ipa` | 1.1.0 files `53` digest `70a5f0881a37859ef7fefd5502c0db05c6ce6c2d6dfa1e71828785f523ba7021` | `docs/specs/fhir-packages/hl7.fhir.uv.ipa#1.1.0/PROVENANCE.md` |
| `hl7.fhir.uv.smart-app-launch` | 2.0.0 files `8` digest `68fd66519d2ff373d8f9e58bfa62455f3e0aeb95a780f2218e5ffee5c4b49c3e` | `docs/specs/fhir-packages/hl7.fhir.uv.smart-app-launch#2.0.0/PROVENANCE.md` (CC-BY-4.0) |
| `hl7.fhir.eu.extensions.r4` | 1.3.1 files `33` digest `740cad8a7c5a182a4f37fd7877ffbb38f1f9769468a4a5d292fe12a78c26bb18` | `docs/specs/fhir-packages/hl7.fhir.eu.extensions.r4#1.3.1/PROVENANCE.md` |
| `hl7.fhir.eu.extensions.r4` | 1.3.0 files `30` digest `32d6a88e8444b67150d5945926dabd004820dccd5e8140774be79e67c20346e8` | `docs/specs/fhir-packages/hl7.fhir.eu.extensions.r4#1.3.0/PROVENANCE.md` |
| `ihe.pharm.mpd.r4` | 1.0.0-comment-2 files `111` digest `bb5122aaafc17b33a4691308cebb4e91283d45a560e8b9eeac984dd0966a5c32` | `docs/specs/fhir-packages/ihe.pharm.mpd.r4#1.0.0-comment-2/PROVENANCE.md` (CC-BY-SA-4.0) |
| `hl7.fhir.uv.extensions.r4` | 5.3.0 files `1592` digest `39605ce9849da6e22d081c0c07ec3ba190805736e3b5b830ea493fe431b3cb9f` | `docs/specs/fhir-packages/hl7.fhir.uv.extensions.r4#5.3.0/PROVENANCE.md` |
| `hl7.fhir.uv.extensions.r4` | 5.2.0 files `1401` digest `727a50ef9b1c4472653d772ae0b470405b305c2c983c02f1f621ebcc5e05a5b3` | `docs/specs/fhir-packages/hl7.fhir.uv.extensions.r4#5.2.0/PROVENANCE.md` |
| `hl7.fhir.uv.xver-r5.r4` | 0.1.0 files `6139` digest `cb81d3f43fd473471a8a4e5ea0a8066aeeb06bbcf24acb31a24dd85db0ebb0e2` | `docs/specs/fhir-packages/hl7.fhir.uv.xver-r5.r4#0.1.0/PROVENANCE.md` |
| `hl7.terminology.r4` | 7.3.0 files `4191` digest `0c47d902204317908d1103108f554ad5a0abd238466a9f7410b85d269fcb3098` | `docs/specs/fhir-packages/hl7.terminology.r4#7.3.0/PROVENANCE.md` |
| `hl7.terminology.r4` | 7.1.0 files `4118` digest `0f91757bc29290a3c1c090cf3fe57c884f5af91472182157bac7cf4d0ead73c3` | `docs/specs/fhir-packages/hl7.terminology.r4#7.1.0/PROVENANCE.md` |
| `hl7.terminology.r4` | 6.5.0 files `4407` digest `acae644b9b4dba9d388b96b35f452c2a35f8bf199c1d2b570e85634c6972ee09` | `docs/specs/fhir-packages/hl7.terminology.r4#6.5.0/PROVENANCE.md` |
| `hl7.terminology.r4` | 6.3.0 files `4389` digest `67c5e37eee889968f623290df576cf8a24a6de8936c0ad4b14e742b6faf9833f` | `docs/specs/fhir-packages/hl7.terminology.r4#6.3.0/PROVENANCE.md` |
| `hl7.terminology.r4` | 6.2.0 files `4342` digest `6d074864dff5ce089248b6113b6049b0420b8db1ffbb2837bf4e39847d3e28fe` | `docs/specs/fhir-packages/hl7.terminology.r4#6.2.0/PROVENANCE.md` |

## HL7 FHIR Validator (the agreement corpus's reference, regeneration only, never committed)

The profile validator's agreement corpus (#442) compares it with the HL7 FHIR
Validator, prior art and never the oracle. The reference outcomes under
`crates/ferrobridge-validator/tests/agreement/reference/` are committed;
`scripts/vendor/hl7-validator-agreement.sh` regenerates them, reading the pins
below, fetching each into the ignored `target/hl7-validator/` and checking its
sha256 before it runs. The jar comes from the `hapifhir/org.hl7.fhir.core`
GitHub release of that version; each package is a tarball of the FHIR package
registry that the validator loads for FHIR 4.0.1 beyond R4 core and the
profile packages above. Nothing here is read by a build or a test.

| Item | Pin | Repeated in |
|---|---|---|
| `validator_cli.jar` | 6.10.4 sha256 `1106b9d58f9e363e47bea7c4fc065841e5fc91fe9d062775c3bfdd212bd653cc` | read by `scripts/vendor/hl7-validator-agreement.sh` |
| `hl7.fhir.xver-extensions` | 0.1.0 sha256 `f3bb9fa2083402e88a02b41f433655274e8a1cca563211c8f7ba6fd0badf537a` | read by `scripts/vendor/hl7-validator-agreement.sh` |
| `hl7.terminology` | 7.4.0 sha256 `b321ad4ee1c18798abd8692f45bc86550cffac72ae97413125cfb82f0112c72d` | read by `scripts/vendor/hl7-validator-agreement.sh` |
| `hl7.fhir.uv.extensions` | 5.3.0 sha256 `6e3a3f9929e05d2b813a3f98fa1ad2f5122fb6b2819fa73e6c5520002fb2c5b0` | read by `scripts/vendor/hl7-validator-agreement.sh` |
| `hl7.terminology.r5` | 7.1.0 sha256 `473303108b5607aad7b910581739e8bbf3e61a625ed9e739c66e0ca597d4aefe` | read by `scripts/vendor/hl7-validator-agreement.sh` |
| `hl7.fhir.uv.extensions.r5` | 5.2.0 sha256 `e02fc6eff0f37c1611a35aa93ef9aaa3b55ab7371a5c96f4d2a5834106a13170` | read by `scripts/vendor/hl7-validator-agreement.sh` |

## Crate line

The library crates are published to crates.io on one crate version line,
distinct from the product version (owner decision 2026-09-05). The line is
`fhir-types`' own: the sibling published 0.1.97 on 2026-09-12, the first
release from here was 0.1.98, and the line moves one patch with every change
to the crate's packaged content (`.claude/rules/crates-publishing.md`). A
change that breaks a public API is a patch too, so a consumer pins a crate of
the line exactly (owner decision 2026-10-17, #510).
`openehr-mapping-core` and `fhirconnect` joined the line at 0.1.108 with their
first publish (#397). Every other library crate holds its name with a 0.0.0
placeholder (#107), outside the line until its first publish, when it joins
at the line's current value.

| Item | Pin | Repeated in |
|---|---|---|
| `fhir-types` | 0.1.123 | `docs/architecture.md`, `crates/fhir-types/Cargo.toml`, the root `Cargo.toml` `[workspace.dependencies]`, later the `version` of every published `crates/*` manifest |

## Language and runtime

`rust-toolchain.toml` carries the toolchain, and the root `Cargo.toml` carries
the edition, the resolver and the MSRV (#20). The release lane builds every
published binary on this toolchain, with no cache.

| Item | Pin | Repeated in |
|---|---|---|
| Rust toolchain | 1.98.1 | `rust-toolchain.toml` `channel` (stable) |
| Edition | 2024 | root `Cargo.toml` `[workspace.package]` `edition` |
| Cargo resolver | 3 | root `Cargo.toml` `[workspace]` `resolver` |
| MSRV | 1.98 | root `Cargo.toml` `[workspace.package]` `rust-version` |

The deliverable is a server binary, so the MSRV tracks the pinned stable
toolchain.

## Product and citation version

The product version is the workspace `version` in the root `Cargo.toml`, which
every member inherits. The milestone line is 0.0.x, so the first product
version is 0.0.8.

| Item | Pin | Repeated in |
|---|---|---|
| Product version | 0.0.8 | root `Cargo.toml` `[workspace.package]` `version` (#20), `CITATION.cff` `version` (#18), the `ghcr.io/ferrohealth/ferrobridge` image tag default in `compose.yaml` (#22) |

`CITATION.cff` tracks this row exactly, and the guard compares the two whenever
`CITATION.cff` exists. Once the root `Cargo.toml` lands, the guard also compares
its `[workspace.package]` `version` with both.

## Documentation toolchain

The site is an mdBook rendered by `.github/workflows/docs.yml`. Every tool it
installs is pinned here and repeated in the composite action that installs
them, so the book renders the same way in CI as it does on a laptop.

| Item | Pin | Repeated in |
|---|---|---|
| mdBook | 0.5.4 | `.github/actions/docs-toolchain/action.yml` `mdbook-version` |
| mdbook-toc | 0.15.4 | `.github/actions/docs-toolchain/action.yml` `mdbook-toc-version` |
| mdbook-mermaid | 0.17.1 | `.github/actions/docs-toolchain/action.yml` `mdbook-mermaid-version` |

The mermaid browser assets the book loads are vendored from the same
`mdbook-mermaid` release, pinned by commit in
`scripts/vendor/mdbook-mermaid-assets.sh` and recorded in
`website/book/vendor/mermaid/PROVENANCE.md`.

## Licence

| Item | Pin | Repeated in |
|---|---|---|
| Project licence | BUSL-1.1 | `LICENSE`, `NOTICE`, the SPDX header of every first-party file, later the `license` field of every own `Cargo.toml`, the container `image.licenses` label, the README badge |

`LICENSE` names Apache License 2.0 as a licence of its own, as the Change
License four years after each version. `crates/fhir-types` is the one
first-party crate published under Apache-2.0 (`docs/architecture.md` §4.1): it
is generated from the CC0 HL7 FHIR packages and three servers depend on it, so
`scripts/checks/versions.sh` excludes that path and fails on an Apache-2.0 or
MIT claim anywhere else.

Third-party and vendored material keeps its upstream terms, recorded beside the
vendored tree (`.claude/rules/vendored-inputs.md`).

## Rust dependency pins

The root `Cargo.toml` `[workspace.dependencies]` table is the authoritative,
fully pinned third-party crate set. Beyond the openEHR model crates and the
§Third-party crates table above, this file does not duplicate crate versions;
on any discrepancy the manifest wins. A crate joins a member with
`dep.workspace = true`.

## CI tool pins

The tier-1 lanes of `.github/workflows/ci.yml` run four analyzers, each pinned
to an exact version so a CI result matches the local one. `zizmor` and
`shellcheck` are fetched by `taiki-e/install-action`, which verifies the
upstream release checksum; `actionlint` and `hadolint` run from their official
container images, pinned by tag and by digest. The `sqlx-offline` job installs
`sqlx-cli` through the same action, which falls back to `cargo-binstall` because
`sqlx-cli` is not in its own tool list. `sqlx-cli` carries the
`sqlx` crate's version, because the CLI writes the query metadata the crate's
macros read.

| Item | Pin | Repeated in |
|---|---|---|
| `zizmor` | 1.31.1 | `.github/workflows/ci.yml` |
| `actionlint` | 1.7.12 | `.github/workflows/ci.yml` |
| `shellcheck` | 0.11.0 | `.github/workflows/ci.yml` |
| `hadolint` | 2.15.1 | `.github/workflows/ci.yml` |
| `sqlx-cli` | 0.9.0 | `.github/workflows/ci.yml` (the `sqlx-offline` job) |

Keep the locally installed versions on these numbers, so a finding costs a
local run rather than a CI round trip (`.claude/rules/ci-cd.md`).

## Release tool pins

The release lane builds and describes every published artifact with three more
tools, each fetched by the same digest-pinned `taiki-e/install-action`, which
verifies the upstream release checksum. They decide what a consumer can prove
about a binary, so a floating version here would change the contents of a
release without a reviewed change (`docs/release.md`).

| Item | Pin | Repeated in |
|---|---|---|
| `cargo-auditable` | 0.7.5 | `.github/workflows/release-build.yml`, `.github/workflows/release-viewer.yml` |
| `cargo-cyclonedx` | 0.5.9 | `.github/workflows/release-build.yml` |
| `cargo-fuzz` | 0.13.2 | `.github/workflows/fuzz.yml` (the fuzz lane, the one nightly-toolchain job; #155) |
| `syft` | 1.51.1 | `.github/workflows/release-build.yml`, `.github/workflows/release-image.yml`, `.github/workflows/release-viewer.yml` |

`scripts/checks/versions.sh` reads every `tool:` line of the two release
workflows back against these rows, so a bump moves one row and the workflows
follow it.

## Operator console build tools

The `viewer` job of `.github/workflows/ci.yml` builds the operator console's
site bundle (`app/ferrobridge-viewer`, #599) with four tools. cargo-leptos
and the wasm-bindgen CLI are fetched by the digest-pinned
`taiki-e/install-action`; the wasm-bindgen CLI equals the `wasm-bindgen`
crate requirement of the root `Cargo.toml`, because the CLI and the crate
must agree on the bindings they write. The Tailwind CSS standalone CLI and
Binaryen's `wasm-opt` are installed by `scripts/release/viewer-tools.sh`,
each checked against its SHA-256, and `scripts/release/viewer-site.sh`
names the Tailwind release to cargo-leptos.

| Item | Pin | Repeated in |
|---|---|---|
| `cargo-leptos` | 0.3.11 | `.github/workflows/ci.yml` (the `viewer` job), `.github/workflows/release-viewer.yml` |
| `wasm-bindgen` | 0.2.129 | `.github/workflows/ci.yml` (the `viewer` job), `.github/workflows/release-viewer.yml`, the root `Cargo.toml` `[workspace.dependencies]` |
| Tailwind CSS | 4.3.3 | `scripts/release/viewer-tools.sh`, `scripts/release/viewer-site.sh` |
| Binaryen | version_133 | `scripts/release/viewer-tools.sh` |

`scripts/checks/versions.sh` reads each of them back against these rows.

## Operator console browser journeys

The browser journeys (#602) drive the console in headless Chrome over
WebDriver, through chromedriver, with the `thirtyfour` client, a test-only
dependency of the testkit. Chrome and chromedriver come as one Chrome for
Testing release, which `browser-actions/setup-chrome` installs in the
`journeys (browser)` job of `ci.yml`, so the browser and its driver always
match. `thirtyfour` 0.37.5 is the newest release older than two weeks on
2026-10-18, and the Chrome for Testing release is the one FerroFED's journeys
pin, the newest stable one on 2026-10-14. The weekly
`scripts/checks/pin-freshness.sh` reports a newer Stable release.

For a local run, `scripts/release/journey-tools.sh` installs the same
release into `target/journey-tools`, where the journeys look first. It takes
each download URL from the Chrome for Testing JSON API
(`known-good-versions-with-downloads.json`) and checks each zip against the
SHA-256 below before it unpacks it. Chrome for Testing publishes no SHA-256;
each digest is the one FerroFED took on 2026-10-16 of a download whose MD5
equals the `x-goog-hash` the storage bucket serves with it, and the
`mac-arm64` pair verified again here on 2026-10-18.

| Item | Pin | Repeated in |
|---|---|---|
| `thirtyfour` | 0.37.5 | the root `Cargo.toml` `[workspace.dependencies]` |
| Chrome for Testing (Chrome and chromedriver) | 155.0.8059.79 | the `chrome-version` of `.github/workflows/ci.yml`, and `scripts/release/journey-tools.sh` |
| Chrome for Testing, `chrome-linux64.zip` SHA-256 | `bcd9a79130d7e44d70f93be87042e9bcee7fe0f0b3898e06b8c9bed5fb619dcd` | `scripts/release/journey-tools.sh` |
| Chrome for Testing, `chrome-mac-arm64.zip` SHA-256 | `e141a7cbf948780bb7d381860bd5a7944c6736f79b9918b9826fccb199d28ea4` | `scripts/release/journey-tools.sh` |
| Chrome for Testing, `chromedriver-linux64.zip` SHA-256 | `876f0ea4095237bb3e93321d95758d38c9195989935d7bdc9f51d8e26bc6a3a1` | `scripts/release/journey-tools.sh` |
| Chrome for Testing, `chromedriver-mac-arm64.zip` SHA-256 | `ca9a98c74b8d3904be6536ff6d26493abb7d7a476e2b384a75cd55d101bce29d` | `scripts/release/journey-tools.sh` |

`scripts/checks/versions.sh` reads each of them back against these rows.

## GitHub Actions pins

Every `uses:` in `.github/workflows/**` is pinned to a full commit SHA with a
trailing `# vX.Y.Z` comment (`.claude/rules/ci-cd.md`). Dependabot bumps them,
and zizmor checks the form (#16).
