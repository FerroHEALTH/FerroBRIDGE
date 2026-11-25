- `eehrxf::document::metadata` writes the draft EEHRxF Annex III metadata
  (initiative 17512, 2026-11-23): `Document::with_metadata` puts the Table 1
  document metadata on the `Composition` (version, status, language,
  document category, attestation, legal authentication, period, event
  category, source), and `DocumentMetadata::describe` writes the
  `DocumentReference` describing the document in the shape MyHealth@EU
  DocumentReference-minimal requires. `DocumentMetadata::from_composition`
  reads the version, language and period from an openEHR composition. With
  metadata set, every data group takes the document's date and language
  where it carries none, and a data group with no subject, a document with
  no author or a second value where FHIR R4 holds one is refused with a
  typed error.
