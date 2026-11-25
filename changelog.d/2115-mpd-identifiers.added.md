- FHIRconnect mappings read the identities of a record out of openEHR:
  `LOCATABLE.uid` (and `$composition/uid`) as an `Identifier` naming the
  object across its versions, and an `ACTION`'s
  `instruction_details/instruction_id` as a `Reference` that names the
  prescription by that identifier. The MPD project writes the composition
  uid as `MedicationRequest.groupIdentifier`, the `INSTRUCTION` uid as
  `MedicationRequest.identifier`, the `ACTION` uid as
  `MedicationDispense.identifier`, and the order the `ACTION` carries out as
  `MedicationDispense.authorizingPrescription`, so the draft EEHRxF
  ePrescription and eDispensation corpora hold 25/25 and 15/15 (#2115).
