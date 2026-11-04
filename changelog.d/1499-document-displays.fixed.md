- `eehrxf` writes a `display` on every coding of a document it assembles:
  `Composition.type`, each section's `code`, each section's empty reason and
  the LOINC categories of a form. A laboratory result's category, written by
  the shipped laboratory project, carries one too. Without these, a receiver
  with no terminology server refused the document. Each display comes from
  the vendored European material: the MyHealth@EU Master Value Sets
  Catalogue 9.1.0 first, then an HL7 Europe guide's value set or code
  system, then an English example of a guide, and FHIR R4
  `list-empty-reason` for an empty reason. `document::display::display`
  answers it. The six section codes none of that material names (18842-5,
  48767-8, 57080-4, 77599-9, 81338-6, LP173421-1) and the imaging priority
  category still carry no display.
