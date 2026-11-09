- A FHIR `Duration` whose value has a fraction in any unit but the seconds
  (`1.5 d`) is refused going into openEHR, where it was written as the
  duration `P1.5D`, which openEHR BASE does not admit. A visit start or end
  written in the ISO 8601 basic form is read, and a facade `If-Match` that
  names an openEHR version of a composition whose id is an upper-case UUID
  is compared in the spelling the CDR issued.
