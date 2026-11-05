<!-- This file describes vendored third-party material; the bytes beside it
     keep their upstream licence, not the licence of this repository. -->

# Provenance: the openEHR Reference Model class definitions, Release 1.1.0

Vendored verbatim by `scripts/vendor/openehr-rm.sh`. Never edit a file
here: change the pin in docs/VERSIONS.md and re-run the script.

- Generation: the released RM 1.1.0, FerroEHR's `spec_profile = "stable"`
  and its generated `openehr_rm::v1_1`. The development generation is
  vendored beside this tree in `../development/`.
- Source: <https://github.com/openEHR/specifications-RM>, published at
  <https://specifications.openehr.org/releases/RM/Release-1.1.0/>
- Pin: tag `Release-1.1.0`, which resolves to commit `355eb63e201c6b805c44c8809c70d63f79d54f92`; the
  `manifest.json` of that commit dates release `1.1.0` `2020-09-29`
- Fetched: 2026-11-05, each file from raw.githubusercontent.com at the commit
  and checked against the git blob id the commit records
- Upstream licence: Creative Commons Attribution-ShareAlike 3.0 Unported,
  the repository's `LICENSE` file at the commit, vendored beside this
  file. The files are unmodified.
- Layout: the upstream paths of the commit, unchanged
- Files: 6
- Tree digest (sha256 over the sorted per-file `sha256  path` listing,
  `PROVENANCE.md` excluded): `d3eddc481eac97288500fde9b9911e01fa1239e7c44856a67eb7eae6710f48fc`
- Read by: FerroHEALTH/FerroBRIDGE#1582 (the facility, the composer and
  the `feeder_audit` of an HL7 v2 commit, and the committer placeholder)

## What is taken

The class definitions `docs/UML/classes/event_context.adoc`,
`feeder_audit.adoc`, `feeder_audit_details.adoc` and
`party_identified.adoc`, which the RM specifications include, with the
component manifest. The specification chapters, the other classes and the
UML diagrams are not taken.

| File | sha256 | git blob id |
|---|---|---|
| `LICENSE` | `3f941b3b89cf7b8370ceb83cc76d2120d471b58735d8ca60238a751a48d7f72f` | `604209a804632b6e5274005a22293976d07c3099` |
| `docs/UML/classes/event_context.adoc` | `a18f4c40c0cd13f37f4a1124fc2efcde9bc44dbe4515811f5ffa834a53b9475e` | `11ab08e98bf927ddce2931cbfc7ce9e3f696a833` |
| `docs/UML/classes/feeder_audit.adoc` | `34f8f8fa9d7a0b116b58a2ea6a68f3c20c93282f5abbf7ee0eef3e8fe0858ec1` | `5654ca430fec501c8b59dcdfc3bcb992b6290751` |
| `docs/UML/classes/feeder_audit_details.adoc` | `189b5f16302d11aa56c5b2ee8d5614df59718a045605406eb1d6b3857d691aa3` | `201b102e35626cf65505b4489e5acb1bc458e763` |
| `docs/UML/classes/party_identified.adoc` | `17f30acaa146de7dc4fcd6c3801a577c956fa1930f4aef4377b663db9b155efa` | `918834d83a8c553aca7b4959cebcca412021aad1` |
| `manifest.json` | `02dad4ee33ffd19d49d3bb301eca15c44e918fcd41cbd689381422fd07eff7c7` | `347651d4a600d8084a4bd297a867f02dad6e83e9` |
