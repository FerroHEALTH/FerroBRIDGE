<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# Provenance: the FHIRPath R4 test suite pass list

`pass-list.txt` records which tests of the HL7 FHIRPath test suite for FHIR R4
pass among those that call no function FHIRPath 3.0.0 adds (those are the
`fhirpath-r4-stu3` corpus), one `group/test` id per line (a repeated test
name takes a `#n` suffix), and the number of such tests on its `total` line.
It holds no copy of the suite. The
suite is the vendored `r4/fhirpath/tests-fhir-r4.xml` with its input
resources, at the pin `docs/VERSIONS.md` records and
`docs/specs/fhirpath-tests/PROVENANCE.md` describes.

- Written by: `conformance::conformance_fhirpath_r4` in
  `crates/ferrobridge-fhirpath/tests/it/conformance.rs`, under
  `scripts/checks/conformance.sh --update`.
- A test that expects its expression refused passes when the expression does
  not parse, fails the static check against the input's resource type, or
  fails to evaluate. Any other test passes when the evaluation answers its
  outputs, in order unless it says `ordered="false"`.
- The language is FHIRPath Normative Release 2.0.0 with the FHIR R4
  additions, the release FHIR R4 fhirpath.html §2.9 cites, and every failing
  test records its reason in the result the gate reads.
- An input the FHIR specification refuses fails every test that reads it,
  with the rule it breaks: `parameters-example-types.xml` declares the XML
  Schema instance namespace and `xsi:schemaLocation` on its root, which R4
  xml.html §2.6.1.1 forbids in a resource.
- Never edit the list by hand.
