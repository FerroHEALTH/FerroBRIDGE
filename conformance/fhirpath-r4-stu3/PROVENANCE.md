<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# Provenance: the FHIRPath 3.0.0 STU function pass list

`pass-list.txt` records which tests of the HL7 FHIRPath test suite for FHIR R4
pass among those whose expression calls a function FHIRPath 3.0.0 adds as
Standard for Trial Use (<http://hl7.org/fhirpath/STU3/>), one `group/test` id
per line (a repeated test name takes a `#n` suffix), and the number of such
tests on its `total` line. It holds no copy of the suite. The suite is the
vendored `r4/fhirpath/tests-fhir-r4.xml` with its input resources, at the pin
`docs/VERSIONS.md` records and `docs/specs/fhirpath-tests/PROVENANCE.md`
describes.

- Written by: `conformance::conformance_fhirpath_r4_stu3` in
  `crates/ferrobridge-fhirpath/tests/it/conformance.rs`, under
  `scripts/checks/conformance.sh --update`.
- A test belongs here when its expression names one of the functions
  `ferrobridge_fhirpath::eval::STU3_FUNCTIONS` lists followed by `(`, never by
  the test's name. Every other test of the suite belongs to `fhirpath-r4`.
- The tests run with `Environment::with_fhirpath_stu3_functions`. A function
  of the list the evaluator does not implement yet fails its tests as
  undefined, and every failing test records its reason in the result the gate
  reads.
- A test passes on the same rules as in `fhirpath-r4`.
- Never edit the list by hand.
