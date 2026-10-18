<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# Provenance: the profile validator agreement pass list

`pass-list.txt` records which cases of the profile validator's agreement
corpus pass, one case id (the file name without `.json`) per line, and the
number of cases on its `total` line. The cases are the synthetic resources
under `crates/ferrobridge-validator/tests/agreement/cases/`, and the outcomes
they are compared with are the HL7 FHIR Validator 6.10.4's, recorded under
`crates/ferrobridge-validator/tests/agreement/reference/` by
`scripts/vendor/hl7-validator-agreement.sh` at the pins of the "HL7 FHIR
Validator" table of `docs/VERSIONS.md`.

- Written by: `agreement::conformance_the_validator_agrees_with_the_hl7_validator_on_its_corpus`
  in `crates/ferrobridge-validator/tests/it/agreement.rs`, under
  `scripts/checks/conformance.sh --update`.
- A case passes when the error and fatal findings of `ferrobridge-validator`
  equal the reference's, each compared as its location and its rule: the key
  of a failed invariant, the slice a slice cardinality names, or no rule
  beyond the location. Warnings and information are not compared.
- A case on which the two disagree fails, and the test requires it to be
  adjudicated against the specification in its `ADJUDICATED` table, with the
  adjudication as the failure reason. The reference is prior art, never the
  oracle.
- Never edit the list by hand.
