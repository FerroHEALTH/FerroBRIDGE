<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# Provenance: the FHIR STU3 examples pass list

`pass-list.txt` records which resources of HL7's `hl7.fhir.r3.examples` package
(3.0.2) round-trip through the `fhir-types` model of FHIR STU3, one file name
per line, and the resource count on its `total` line. It holds no copy of any
input. The package is fetched at build time into
`tools/ferrobridge-testkit/vendor/hl7.fhir.r3.examples/` by
`scripts/vendor/fhir-packages.sh --build-time`, which writes the committed
`PROVENANCE.md` there, and is never committed.

A case passes when the strict JSON codec decodes the file, the encoder gives
back the same document, and that document goes through the XML codec and
back unchanged, compared as the lexical document model (object members in
order of name, every number in its written text). Every resource of the
package is a case, the DataElements and the conformance resources included.
19 files begin with a UTF-8 byte order mark, which the suite strips before
parsing and the fetched files keep.

The list is written by
`fhir_examples::conformance_the_fhir_stu3_examples_hold_their_pass_list` in
`tools/ferrobridge-testkit/tests/it/fhir_examples.rs`, under
`scripts/checks/conformance.sh --update`, which fetches the package first.
Never edit the list by hand.
