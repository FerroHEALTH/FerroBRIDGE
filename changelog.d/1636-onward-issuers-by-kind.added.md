- The FHIR facade carries a patient onto the CDR through an onward issuer of
  `kind = "patients"`, and each `[[cdr.onward]]` table adds a `persons`,
  `federation` or `patients` issuer beside `[cdr.credentials.onward]`, so
  every caller is exchanged at the issuer of its kind. Two issuers of one
  kind refuse the start.
