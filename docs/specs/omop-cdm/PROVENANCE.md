<!-- This file describes vendored third-party material; the bytes beside it
     keep their upstream licence, not the licence of this repository. -->

# Provenance: the OMOP Common Data Model v5.4 definitions, PostgreSQL DDL and SQL scripts

Vendored verbatim by `scripts/vendor/omop-cdm.sh`
(.claude/rules/vendored-inputs.md). Never edit a file here: change the pin in
docs/VERSIONS.md and re-run the script.

- Source: <https://github.com/OHDSI/CommonDataModel>
- Pin: tag `v5.5.0`, which resolves to commit `ba3dc1b651cc243279052bca3d051485c3ec1136`
- Fetched: 2026-11-03
- Upstream licence: `Apache License 2.0`. **The repository has no `LICENSE` file.**
  `DESCRIPTION` is the only place the licence is declared, which is why that
  file is vendored here and the script fails if a `LICENSE` file ever appears.
- Layout: the upstream paths, unchanged
- Files: 8
- Tree digest (sha256 over the sorted per-file `sha256  path` listing,
  `PROVENANCE.md` excluded): `5c310bd11270a964d25a0f4f0e694a80fef0137eb3bf9c9b436ba499e133fcae`

## What is here

The two CSV files are the machine-readable definitions the `omop-cdm` row
types are generated from. The four PostgreSQL files are OHDSI's rendered DDL:
OHDSI renders them from the same CSVs through a dialect layer that sits outside
them, so they are vendored rather than generated, and a test asserts the
generated column set equals the DDL's (docs/architecture.md section 10).

`site/sqlScripts.qmd` is the source of the CDM's SQL scripts page
(<https://ohdsi.github.io/CommonDataModel/sqlScripts.html>). Its condition era
and drug era scripts are OHDSI SQL for SqlRender, so the `omop-cdm` crate
carries their PostgreSQL form under `sql/`, and a test pins the digest of
this file so an upstream change to the scripts is noticed.

| File | sha256 | git blob id |
|---|---|---|
| `inst/csv/OMOP_CDMv5.4_Field_Level.csv` | `1b727775c4bf470274d6bb7b5b20f447fac37184e11ac7c69c1e2d6c3c48d06c` | `a6d0d4b26e86928bcd0aee7f31e7d3b96956f0b4` |
| `inst/csv/OMOP_CDMv5.4_Table_Level.csv` | `7b17597d4f1c02b1e31043f0c414f0112f46735a2313d7f7fc9770a6e2c840d9` | `07711071feec20e5bdc1e8bee51a2f756c0e771b` |
| `inst/ddl/5.4/postgresql/OMOPCDM_postgresql_5.4_ddl.sql` | `c340fc1723cc71c411aa7d5d67844882ec720bed1135eca2b8a9029d0a3ef854` | `162bdc5c863fec8ca32e07814a40a347f267788a` |
| `inst/ddl/5.4/postgresql/OMOPCDM_postgresql_5.4_primary_keys.sql` | `d8f50617f9a698bd9fbfe8d4106d76dcd543949a6976032786f64a91c1941f81` | `9304d020bcb007bca8ad8e57873b76a2b626e8d9` |
| `inst/ddl/5.4/postgresql/OMOPCDM_postgresql_5.4_indices.sql` | `ea23abbb327ee94e974728196cce2b2db466c7ad4bb4313f36fa3ce2388e8776` | `18b739493af0c3bcc90bff728e987a5b06a7bb0b` |
| `inst/ddl/5.4/postgresql/OMOPCDM_postgresql_5.4_constraints.sql` | `b09f54e6f1550c8594941a96798246944c10d42d9d4be8291a5058033be0f91d` | `19818c3cd59fa47d153160197a8be9399040a6b6` |
| `site/sqlScripts.qmd` | `5bbc254456e131a7e1c56bff013d4d79ab0b1ecbec40228746b58e7f7039147a` | `085dee247cc4ddb6be0b2daf93521adfa603daac` |
| `DESCRIPTION` | `88a1f1f5ba02ecc694b327d3a989b3cb97ed1cb6c12092c84094ce49f95f7aed` | `1a5c4f5215981853019f5a3cecf5f7e795d6b225` |
