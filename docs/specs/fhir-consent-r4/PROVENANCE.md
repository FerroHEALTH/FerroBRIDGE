<!-- This file describes vendored third-party material; the bytes beside it
     keep their upstream licence, not the licence of this repository. -->

# Provenance: the FHIR R4 Consent resource

Vendored by `scripts/vendor/consent.sh`, each artefact pinned by its
URL and the sha256 of its bytes. Never edit a file here: change the pins in
the script and the pin-set digest in docs/VERSIONS.md, and re-run the script.

- Pin-set digest (sha256 over the sorted `mode  file  url  sha256` lines of
  the pins): `f53aa42c2df73097ba81d07b9e8620df9d3b59928c2bfb3fbec54eacbaa8ff8e`
- Fetched: 2026-11-14, with the User-Agent `ferrobridge-vendor (scripts/vendor)`
- Artefacts: 24 committed, 0 cache only, 0 needing
  manual retrieval
- Files in this directory: 24 besides this one, each verbatim as the
  publisher serves it
- Tree digest (sha256 over the sorted per-file `sha256  path` listing,
  `PROVENANCE.md` excluded): `69805a836066d06857ff90b99f81d7f1555bc17d20bbf893ca0116190abd4e9f`
- Read by: FerroHEALTH/FerroBRIDGE#1933 (app/ferrobridge-server/src/consent and its tests)

The `Consent` resource of HL7 FHIR R4 (4.0.1), each file whole as the
specification serves it, without the stylesheets, scripts and images the
pages link: the resource page (scope, boundaries, the element table, the
constraints), its formal definitions, its StructureDefinition, the value set
pages of the bindings the mapping reads (`scope`, `category`, `status`,
`provision.type`, `provision.data.meaning`, `provision.action` and the
policy codes of `policyRule`), the examples index, every published Consent
example in JSON, and the licence page that grants the copy. The XML and
Turtle forms of the examples are not taken.

## Artefacts

### `consent.html` (committed)

- Source: <https://hl7.org/fhir/R4/consent.html>
- Version: FHIR R4 (4.0.1)
- sha256: `438c1e3de0759274f2196fa3bb3efd0f56f812543b3a4c7547e86b42f26c6d2c`
- Licence: Creative Commons "No Rights Reserved" (CC0 1.0), as the license.html page of the release states; HL7 keeps the "FHIR" trademark, which this copy does not use as a mark

### `consent-definitions.html` (committed)

- Source: <https://hl7.org/fhir/R4/consent-definitions.html>
- Version: FHIR R4 (4.0.1)
- sha256: `9ddabeb5c4821b9e923dd767aae1106e9daa3a81a56112f9de9be68d9b42507e`
- Licence: Creative Commons "No Rights Reserved" (CC0 1.0), as the license.html page of the release states; HL7 keeps the "FHIR" trademark, which this copy does not use as a mark

### `consent.profile.json` (committed)

- Source: <https://hl7.org/fhir/R4/consent.profile.json>
- Version: FHIR R4 (4.0.1)
- sha256: `c4d00dcd93cfc55242c315f72dfe5a4f25f4a590f162bf20ccad16145972b02d`
- Licence: Creative Commons "No Rights Reserved" (CC0 1.0), as the license.html page of the release states; HL7 keeps the "FHIR" trademark, which this copy does not use as a mark

### `consent-examples.html` (committed)

- Source: <https://hl7.org/fhir/R4/consent-examples.html>
- Version: FHIR R4 (4.0.1)
- sha256: `afcb4440e95d71c310e52bd65d3306e1bc8f332d31cce40cd204dc4fdb5b4c47`
- Licence: Creative Commons "No Rights Reserved" (CC0 1.0), as the license.html page of the release states; HL7 keeps the "FHIR" trademark, which this copy does not use as a mark

### `license.html` (committed)

- Source: <https://hl7.org/fhir/R4/license.html>
- Version: FHIR R4 (4.0.1)
- sha256: `b2aeb02247ca2aa855ed1fce29f62600aeeb8dcc8257ec1e0b40ceaf4fd60a15`
- Licence: Creative Commons "No Rights Reserved" (CC0 1.0), as the license.html page of the release states; HL7 keeps the "FHIR" trademark, which this copy does not use as a mark

### `valueset-consent-scope.html` (committed)

- Source: <https://hl7.org/fhir/R4/valueset-consent-scope.html>
- Version: FHIR R4 (4.0.1)
- sha256: `e0ed4a9ceb5c3a3e8b08c5d01fafe7927af5f5b530e90bfe1dff1cb986c3eb9e`
- Licence: Creative Commons "No Rights Reserved" (CC0 1.0), as the license.html page of the release states; HL7 keeps the "FHIR" trademark, which this copy does not use as a mark

### `valueset-consent-category.html` (committed)

- Source: <https://hl7.org/fhir/R4/valueset-consent-category.html>
- Version: FHIR R4 (4.0.1)
- sha256: `abef5c333602abd495aea3c2f6b9bf0af41138a1f3edf2ff2cc849522e86c809`
- Licence: Creative Commons "No Rights Reserved" (CC0 1.0), as the license.html page of the release states; HL7 keeps the "FHIR" trademark, which this copy does not use as a mark

### `valueset-consent-state-codes.html` (committed)

- Source: <https://hl7.org/fhir/R4/valueset-consent-state-codes.html>
- Version: FHIR R4 (4.0.1)
- sha256: `28d193618ffaa5cc1651e5ce8a4ccece7e3eb71fff2e001e373a516aa8e2f75c`
- Licence: Creative Commons "No Rights Reserved" (CC0 1.0), as the license.html page of the release states; HL7 keeps the "FHIR" trademark, which this copy does not use as a mark

### `valueset-consent-provision-type.html` (committed)

- Source: <https://hl7.org/fhir/R4/valueset-consent-provision-type.html>
- Version: FHIR R4 (4.0.1)
- sha256: `5690d59a40c30335f2669da6790a730c283859a69b3ddf365f4ab33c799ca9cb`
- Licence: Creative Commons "No Rights Reserved" (CC0 1.0), as the license.html page of the release states; HL7 keeps the "FHIR" trademark, which this copy does not use as a mark

### `valueset-consent-data-meaning.html` (committed)

- Source: <https://hl7.org/fhir/R4/valueset-consent-data-meaning.html>
- Version: FHIR R4 (4.0.1)
- sha256: `24c717e06ab1e6f46b3335b91a6029383ba44eb19d2742b99a5506afdf76bbf0`
- Licence: Creative Commons "No Rights Reserved" (CC0 1.0), as the license.html page of the release states; HL7 keeps the "FHIR" trademark, which this copy does not use as a mark

### `valueset-consent-action.html` (committed)

- Source: <https://hl7.org/fhir/R4/valueset-consent-action.html>
- Version: FHIR R4 (4.0.1)
- sha256: `517798619f2f9b72b391b938ae499cdecfd14585cf25f8e6a6fa180215769ed5`
- Licence: Creative Commons "No Rights Reserved" (CC0 1.0), as the license.html page of the release states; HL7 keeps the "FHIR" trademark, which this copy does not use as a mark

### `valueset-consent-policy.html` (committed)

- Source: <https://hl7.org/fhir/R4/valueset-consent-policy.html>
- Version: FHIR R4 (4.0.1)
- sha256: `8106d77f42fbaea38af9ee15a7b7b38a4e77e92318507e162fae9bc1da9d3841`
- Licence: Creative Commons "No Rights Reserved" (CC0 1.0), as the license.html page of the release states; HL7 keeps the "FHIR" trademark, which this copy does not use as a mark

### `consent-example.json` (committed)

- Source: <https://hl7.org/fhir/R4/consent-example.json>
- Version: FHIR R4 (4.0.1)
- sha256: `464abff4f0bda68b6247e17acb5bad76ab9d1701e20b68357b67bdace62c6924`
- Licence: Creative Commons "No Rights Reserved" (CC0 1.0), as the license.html page of the release states; HL7 keeps the "FHIR" trademark, which this copy does not use as a mark

### `consent-example-Emergency.json` (committed)

- Source: <https://hl7.org/fhir/R4/consent-example-Emergency.json>
- Version: FHIR R4 (4.0.1)
- sha256: `f13c1ff607d9b120f0b18cb8f9c880e63be55c0df18965d4d8db554218a06868`
- Licence: Creative Commons "No Rights Reserved" (CC0 1.0), as the license.html page of the release states; HL7 keeps the "FHIR" trademark, which this copy does not use as a mark

### `consent-example-Out.json` (committed)

- Source: <https://hl7.org/fhir/R4/consent-example-Out.json>
- Version: FHIR R4 (4.0.1)
- sha256: `eaf2721c7e37e664aca46ea327c504e62213b87845668c978eec5a5535122d2e`
- Licence: Creative Commons "No Rights Reserved" (CC0 1.0), as the license.html page of the release states; HL7 keeps the "FHIR" trademark, which this copy does not use as a mark

### `consent-example-grantor.json` (committed)

- Source: <https://hl7.org/fhir/R4/consent-example-grantor.json>
- Version: FHIR R4 (4.0.1)
- sha256: `7e5fd4946f33c84555f8f7878623bfd7bc65efd5e3da29f3aad324c213fa4573`
- Licence: Creative Commons "No Rights Reserved" (CC0 1.0), as the license.html page of the release states; HL7 keeps the "FHIR" trademark, which this copy does not use as a mark

### `consent-example-notAuthor.json` (committed)

- Source: <https://hl7.org/fhir/R4/consent-example-notAuthor.json>
- Version: FHIR R4 (4.0.1)
- sha256: `b3be9411b1fe9ec38a82bbdcf075076abb9c4053c746088328b472dc39b2f00d`
- Licence: Creative Commons "No Rights Reserved" (CC0 1.0), as the license.html page of the release states; HL7 keeps the "FHIR" trademark, which this copy does not use as a mark

### `consent-example-notOrg.json` (committed)

- Source: <https://hl7.org/fhir/R4/consent-example-notOrg.json>
- Version: FHIR R4 (4.0.1)
- sha256: `d5ce88400d638badd7c9eb2bb63d637644499750cf02343c0cdce71a6f68c24d`
- Licence: Creative Commons "No Rights Reserved" (CC0 1.0), as the license.html page of the release states; HL7 keeps the "FHIR" trademark, which this copy does not use as a mark

### `consent-example-notThem.json` (committed)

- Source: <https://hl7.org/fhir/R4/consent-example-notThem.json>
- Version: FHIR R4 (4.0.1)
- sha256: `c1fe61d49df3053ac0e1e074d0d2b86b79dff5a9eb797b46eeab1f8c78ea77fb`
- Licence: Creative Commons "No Rights Reserved" (CC0 1.0), as the license.html page of the release states; HL7 keeps the "FHIR" trademark, which this copy does not use as a mark

### `consent-example-notThis.json` (committed)

- Source: <https://hl7.org/fhir/R4/consent-example-notThis.json>
- Version: FHIR R4 (4.0.1)
- sha256: `e1ac7fa8868d4928ed596ebe7a39ad83697f878ed04c15e12d5e9df43397dff7`
- Licence: Creative Commons "No Rights Reserved" (CC0 1.0), as the license.html page of the release states; HL7 keeps the "FHIR" trademark, which this copy does not use as a mark

### `consent-example-notTime.json` (committed)

- Source: <https://hl7.org/fhir/R4/consent-example-notTime.json>
- Version: FHIR R4 (4.0.1)
- sha256: `f29b02ce05901311a4112aae201384d62307489e26ce38dff9602a55c1fdebe8`
- Licence: Creative Commons "No Rights Reserved" (CC0 1.0), as the license.html page of the release states; HL7 keeps the "FHIR" trademark, which this copy does not use as a mark

### `consent-example-pkb.json` (committed)

- Source: <https://hl7.org/fhir/R4/consent-example-pkb.json>
- Version: FHIR R4 (4.0.1)
- sha256: `3369a161d5e0e819335c13cf538d60dde6cc3d2193c31592c405dfa63754dfd4`
- Licence: Creative Commons "No Rights Reserved" (CC0 1.0), as the license.html page of the release states; HL7 keeps the "FHIR" trademark, which this copy does not use as a mark

### `consent-example-signature.json` (committed)

- Source: <https://hl7.org/fhir/R4/consent-example-signature.json>
- Version: FHIR R4 (4.0.1)
- sha256: `90da67cb25f3b7dadbaee177e9d1af6fda6f7ab1f08eaeac649ae85a989abf42`
- Licence: Creative Commons "No Rights Reserved" (CC0 1.0), as the license.html page of the release states; HL7 keeps the "FHIR" trademark, which this copy does not use as a mark

### `consent-example-smartonfhir.json` (committed)

- Source: <https://hl7.org/fhir/R4/consent-example-smartonfhir.json>
- Version: FHIR R4 (4.0.1)
- sha256: `0834eccf9de9b717d3cba6b807090fe9420946e5a60aa7b1f7c64c2deb2414ea`
- Licence: Creative Commons "No Rights Reserved" (CC0 1.0), as the license.html page of the release states; HL7 keeps the "FHIR" trademark, which this copy does not use as a mark
