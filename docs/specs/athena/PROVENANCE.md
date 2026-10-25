<!-- This file describes vendored third-party material; the bytes beside it
     keep their upstream licence, not the licence of this repository. -->

# Provenance: the OHDSI Athena application's vocabulary download writer

Vendored verbatim by `scripts/vendor/athena.sh`
(.claude/rules/vendored-inputs.md). Never edit a file here: change the pin in
docs/VERSIONS.md and re-run the script.

- Source: <https://github.com/OHDSI/Athena>
- Pin: commit `2aab8cac738d407211d9f5582827262899dc7603`
- Fetched: 2026-10-25
- Upstream licence: `Apache-2.0`, as the notice at the head of every vendored
  file states. **The repository has no `LICENSE` file**, so only files that
  carry the notice are vendored, and the script fails if one stops carrying it
  or if a `LICENSE` file appears.
- Layout: the upstream paths, unchanged
- Files: 17
- Tree digest (sha256 over the sorted per-file `sha256  path` listing,
  `PROVENANCE.md` excluded): `1658ae49197a067ce964bf30a20e8db48431125712829c132b6b266e4d5bef07`

## What is here, and what it is

This is the source of the application that writes the download
<https://athena.ohdsi.org> hands an account holder. It is the export format
as the writer states it at the pinned commit, read from code, **never a
specification**: OHDSI publishes no format document, and the CDM says only
"please visit athena.ohdsi.org" about the vocabulary's distribution. No
download was observed for this record; a deployment that meets a download
which disagrees with it reports it on issue #88.

What the files state:

- **File set.** One file per saver, named by its `fileName()`:
  `CONCEPT.csv`, `VOCABULARY.csv`, `DOMAIN.csv`, `CONCEPT_CLASS.csv`,
  `CONCEPT_RELATIONSHIP.csv`, `RELATIONSHIP.csv`, `CONCEPT_SYNONYM.csv`,
  `CONCEPT_ANCESTOR.csv` and `DRUG_STRENGTH.csv`, the nine CDM vocabulary
  tables Athena serves (`SOURCE_TO_CONCEPT_MAP` is a site's own table and is
  not in it). Beside them `CONCEPT_CPT4.csv` (the CPT4 concepts with an
  empty `concept_name`, as `ConceptCPT4V5Saver` selects them), and, when
  the Athena database holds the tables, `CONCEPT_METADATA.csv`,
  `CONCEPT_RELATIONSHIP_METADATA.csv` and `PACK_CONTENT.csv`, which are
  not CDM v5.4 tables.
- **Header row.** The first line holds the column names exactly as the
  database driver reports them (`ResultSetCsvValues.columnNames`).
- **Delimiter.** The `csv.separator` property, a tab in the production
  properties: line 96 of
  <https://github.com/OHDSI/Athena/blob/2aab8cac738d407211d9f5582827262899dc7603/properties/prod/application.properties>.
  That file carries no licence notice, so it is cited here, not vendored.
- **Quoting.** None. `AthenaCSVWriter.writeNext` joins the values with the
  delimiter and escapes nothing: a value holding a tab or a line break
  would split its row, which a reader can only refuse by the field count.
- **Line end and encoding.** `\n` after every row, UTF-8.
- **Null.** An empty field.
- **Date format.** `yyyyMMdd` (`ResultSetCsvValues.DATE_FORMAT`).

| File | sha256 | git blob id |
|---|---|---|
| `src/main/java/com/odysseusinc/athena/service/impl/AthenaCSVWriter.java` | `a2f1b25702445e1749f23d24883ae9f477229b0bcb8407d35d4ca20457f2ada8` | `e8ba9f8887db532c3dba802342829b83a6bbb62c` |
| `src/main/java/com/odysseusinc/athena/service/impl/ResultSetCsvValues.java` | `1d111eb9aa6214095fc014217277094137b5e6c65bd48afc321e29a45d0f4a2c` | `55810517250748d75f6560378f79c869dd32f444` |
| `src/main/java/com/odysseusinc/athena/service/saver/CSVSaver.java` | `fa6ca39dbebdda7733cb97e609ad254cfc1eb47907712c760c56fc75b15424ce` | `fb6cd75e931d495daacea7cbc24999226523b29f` |
| `src/main/java/com/odysseusinc/athena/service/saver/common/ConceptAncestorSaver.java` | `4c657c690709ff92e8cae965cb6f67170055e25122c491cdbfa52f81fc26387f` | `815e573d7e4494caec01f8712b4968cdec86cc11` |
| `src/main/java/com/odysseusinc/athena/service/saver/common/ConceptRelationshipSaver.java` | `c992a91684b9b5ed702234f9bd1f675a918bc3a8593aff83d2c5e53f99b1850f` | `e1a0c7fc6571a753618d6c0162d0fca866bdf1c1` |
| `src/main/java/com/odysseusinc/athena/service/saver/common/ConceptSynonymSaver.java` | `7fbb3834febdcd405de121685179fe1d8320dcedfab04791fa37debbb2d32f48` | `37442e60d5d3dde4c0d93eb9ea691da5b61798ed` |
| `src/main/java/com/odysseusinc/athena/service/saver/common/DrugStrengthSaver.java` | `1306f276ae90906bfa306140b4763570dba4af84035ec9009ea5a51e2af226fe` | `18a43442c5249f697872b2039736c47341977103` |
| `src/main/java/com/odysseusinc/athena/service/saver/common/RelationshipSaver.java` | `c627adaa422c5ae238993ef5e78a80b600f10c299377ac665ee87f28aa13a511` | `94a5df6fcef68ba410ce0793d261bbfe177ea7f8` |
| `src/main/java/com/odysseusinc/athena/service/saver/common/VocabularySaver.java` | `e9e7cec00607c81c228708d6a0886ad32ba1c29bef55b79a16e8f516a6b3832f` | `e64c3782057dedd5ee2b82e38f1f32dd506ac8dd` |
| `src/main/java/com/odysseusinc/athena/service/saver/v5/ConceptV5Saver.java` | `7ac0c0108d19a129fcf326d7b80556d5b1166c8685ce6a2081c1557016681645` | `d466d3ef42bb38020444006b73c858516334bb10` |
| `src/main/java/com/odysseusinc/athena/service/saver/v5/ConceptCPT4V5Saver.java` | `7730a63cbaa783823b0c9468376699491066cff8a3341077cab5709c093e149b` | `d266dd0f5f5ea76d73077d778603e9b941fe667e` |
| `src/main/java/com/odysseusinc/athena/service/saver/v5/ConceptClassV5Saver.java` | `5ca0ca5a16f25c25406950a9965ec8e6d2df3cf4ecb5bfaa99e09a3b6a68271d` | `e2d16f4dd6f86c48089bdcca4659f5728eef6e19` |
| `src/main/java/com/odysseusinc/athena/service/saver/v5/DomainSaver.java` | `bfbc2ac64ce496ee063ad2a45677007eae9d84c23a723ff666db14b3db775dff` | `e69e23716acd0baa63280ec28cb8fcab99bd1965` |
| `src/main/java/com/odysseusinc/athena/service/saver/v5/ConceptMetadataV5Saver.java` | `42dcc2429ccc4e6245132de41d86cb5776ca19f248f2fdfce7abee8dfe6e01b8` | `b44e30551e702af870a2f74339ab32758ac9525f` |
| `src/main/java/com/odysseusinc/athena/service/saver/v5/ConceptRelationshipMetadataV5Saver.java` | `2836314ace6b04bfbe4a7e61a164105be3ebe88dc564201a45dedf75064fca87` | `8ea20428e60609e85766615bd7ed412ca9b9fa63` |
| `src/main/java/com/odysseusinc/athena/service/saver/v5/PackContentV5Saver.java` | `5f7fc1aee5f8a351d73a3b0cb9f3d5392e999a0e6459ae90c6f3aa765a7999e8` | `09bcb94f921684a12dda0c5de525da62a7a2bbe5` |
| `src/main/java/com/odysseusinc/athena/service/saver/v5/InvalidConceptCPT4V5Saver.java` | `c08b2c6996aeb9f3308bc346720d1887a6633d8497157eaba636b879d8b0c27d` | `2998e483d29ec7616ca5c2a67dc37b42c82b8ed2` |
