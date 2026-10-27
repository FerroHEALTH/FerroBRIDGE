- **The HL7 Europe Medication Prescription and Dispense 1.0.0 round trip**
  (#116). Two FerroBRIDGE contexts map FerroBRIDGE's medication template, an
  ADL 2 template over seven published CKM archetypes, to
  `MedicationRequest-eu-mpd` and `MedicationDispense-eu-mpd`, each with a
  referenced `Medication-eu-mpd` and the dosage as a `Dosage-eu-mpd`; the
  dispense is the medication ACTION at its careflow step "Prescription
  dispensed". `scripts/vendor/ckm-medication.sh` vendors the archetypes at
  the CKM mirror commit `docs/VERSIONS.md` pins, and the suites build the
  operational template at test time. `fhirconnect` 0.1.196 (with `ferrobridge-hl7v2` 0.1.152 and `openehr-mapping-core` 0.1.131) takes
  `openehr-adl` as a test dependency, and the bridge moves to the
  `openehr-*` crates at 0.0.100, whose ADL 2 Web Template builder reads the
  ADL 1.4 term constraints of the CKM sources.
