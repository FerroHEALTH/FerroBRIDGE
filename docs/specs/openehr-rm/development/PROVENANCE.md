<!-- This file describes vendored third-party material; the bytes beside it
     keep their upstream licence, not the licence of this repository. -->

# Provenance: the openEHR Reference Model class definitions, development

Vendored verbatim by `scripts/vendor/openehr-rm.sh`. Never edit a file
here: change the pin in docs/VERSIONS.md and re-run the script.

- Generation: the development RM 1.2.0, FerroEHR's default
  `spec_profile = "development"` and its generated `openehr_rm::v1_2`.
  The released generation is vendored beside this tree in
  `../release-1.1.0/`.
- Source: <https://github.com/openEHR/specifications-RM>, published at
  <https://specifications.openehr.org/releases/RM/development/>
- Pin: commit `09e481e9f2ed9e3f9d5b9257bd68c0679d80c7cc` of the `master` branch, committed 2026-11-02T00:52:44Z,
  the commit FerroHEALTH/FerroEHR vendors and generates from. The
  `manifest.json` of that commit lists no 1.2.0 release: the generation is
  unreleased.
- Fetched: 2026-11-05, each file from raw.githubusercontent.com at the commit
  and checked against the git blob id the commit records
- Upstream licence: Creative Commons Attribution-ShareAlike 3.0 Unported,
  the repository's `LICENSE` file at the commit, vendored beside this
  file. The files are unmodified.
- Layout: the upstream paths of the commit, unchanged. This line names each
  class file by its package (`org.openehr.rm.common.party_identified.adoc`
  where Release 1.1.0 has `party_identified.adoc`).
- Files: 6
- Tree digest (sha256 over the sorted per-file `sha256  path` listing,
  `PROVENANCE.md` excluded): `f74963daff1f8c1de06e107239df4e19f17d1deac9ab09492566c26bf3076cd5`
- Read by: FerroHEALTH/FerroBRIDGE#1582 (the facility, the composer and
  the `feeder_audit` of an HL7 v2 commit, and the committer placeholder)

## What is taken

The class definitions
`docs/UML/classes/org.openehr.rm.composition.event_context.adoc`,
`org.openehr.rm.common.feeder_audit.adoc`,
`org.openehr.rm.common.feeder_audit_details.adoc` and
`org.openehr.rm.common.party_identified.adoc`, which the RM
specifications include, with the component manifest. The specification
chapters, the other classes and the UML diagrams are not taken.

## How it differs from Release 1.1.0

Compared byte for byte with `../release-1.1.0/`:

- `EVENT_CONTEXT`, `FEEDER_AUDIT` and `FEEDER_AUDIT_DETAILS`: the same
  bytes (the same git blob id) under the package-qualified file name.
- `PARTY_IDENTIFIED`: the description line ends in one trailing space,
  from the class definitions regenerated from the BMM (SPECRM-129); the
  attributes, the functions and `Basic_validity` are the same text.
- `LICENSE`: the same bytes.
- `manifest.json`: release 1.0.2 is dated `2008-12-31` where Release
  1.1.0 dates it `2008-12-20`; no other field differs.

| File | sha256 | git blob id |
|---|---|---|
| `LICENSE` | `3f941b3b89cf7b8370ceb83cc76d2120d471b58735d8ca60238a751a48d7f72f` | `604209a804632b6e5274005a22293976d07c3099` |
| `docs/UML/classes/org.openehr.rm.common.feeder_audit.adoc` | `34f8f8fa9d7a0b116b58a2ea6a68f3c20c93282f5abbf7ee0eef3e8fe0858ec1` | `5654ca430fec501c8b59dcdfc3bcb992b6290751` |
| `docs/UML/classes/org.openehr.rm.common.feeder_audit_details.adoc` | `189b5f16302d11aa56c5b2ee8d5614df59718a045605406eb1d6b3857d691aa3` | `201b102e35626cf65505b4489e5acb1bc458e763` |
| `docs/UML/classes/org.openehr.rm.common.party_identified.adoc` | `5d3a96e4c440b56c1666268899eafd50c685120684b40288f01eba18c11778fb` | `e9904c585953ef6f81894e705d26884507ef34c7` |
| `docs/UML/classes/org.openehr.rm.composition.event_context.adoc` | `a18f4c40c0cd13f37f4a1124fc2efcde9bc44dbe4515811f5ffa834a53b9475e` | `11ab08e98bf927ddce2931cbfc7ce9e3f696a833` |
| `manifest.json` | `34a1f45e6276b4c4616e15208e9555091b27057c609282600aad02c0d5e8f98d` | `3f7c0d95a92b630f3a2824a4660aeedaccb2bbd5` |
