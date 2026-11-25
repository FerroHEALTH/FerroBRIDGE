- `eehrxf::document::representation` writes the PDF and narrative-only
  representations of draft EEHRxF Art 3(1)(a) and (b) (initiative 17512,
  2026-11-23): `PdfA` accepts a PDF/A-1a or PDF/A-1b document by its XMP
  identification schema and refuses any other,
  `DocumentMetadata::describe_pdf` embeds its bytes unchanged in a
  `DocumentReference` with their size and SHA-1, and
  `Document::narrative_only` refuses a section with a structured entry.
  `Declarations` is the configuration of the representations a system
  declares per category, held to Annex II. `Header` carries the document
  elements a `DocumentReference` describes.
