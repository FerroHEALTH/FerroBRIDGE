<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# Provenance: the agreement corpus

- `cases/`: synthetic FHIR R4 resources invented for the corpus, no patient
  data. Each file is one case: a conforming resource, or one that breaks one
  invariant, one required binding, one profile cardinality, one slice or one
  entry profile. The EU cases claim `patient-eu-core` of `hl7.fhir.eu.base`
  2.0.1 or `bundle-eu-eps` of `hl7.fhir.eu.eps` 1.0.0-ballot in
  `meta.profile`, so both validators hold them to the claim.
- `reference/`: the outcome the HL7 FHIR Validator 6.10.4 gives each case,
  written by `scripts/vendor/hl7-validator-agreement.sh`, which runs
  `validator_cli.jar` offline with `-version 4.0.1 -ig
  hl7.fhir.eu.eps#1.0.0-ballot -ig hl7.fhir.eu.base#2.0.1 -tx n/a
  -no-http-access -show-message-ids` over `cases/` and keeps each outcome's
  error and fatal issues as their severity, location, rule and text. The
  jar, the packages and the raw `OperationOutcome` stay in the ignored
  `target/hl7-validator/`.
- Never edit a reference outcome by hand: rerun the script.
