- Breaking: the issuer keys `kind` (except `kind = "federation"`),
  `preset`, `profile_file` and `scopeless_subjects`, the whole `[auth.idp]`
  table, and the profile keys `first_party_clients`, `delegates`,
  `audience_delegates`, `issuer_client_is_person`, `patient_context`,
  `fhir_user_patient`, `representation`, `act_represents_patient`, `guest`
  and every vendor profile key (`app_only`, `client_claims`,
  `person_equality` and the rest) are removed from `[auth.issuer.<name>]`,
  `[cdr.endpoint.credentials.onward]` and `[[cdr.onward]]`, and a
  configuration that sets one is refused as an unknown key. The `persons`,
  `patients` and `machines` issuer kinds go with them.
