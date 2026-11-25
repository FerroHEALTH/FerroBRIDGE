- `eehrxf` 0.40: `DocumentMetadata::describe`, `describe_at` and
  `describe_pdf` write one `DocumentReference` that satisfies both HL7
  Europe EU Health Data API `EehrxfMhdDocumentReference` and MyHealth@EU
  DocumentReference-minimal. It claims the EehrxfMhd profile, carries the
  Annex III documentCategory and the Art 14(1) priority category as two
  codings of its one `category`, names the priority category's `Bundle`
  profile as its `format`, and requires a custodian.
  `DocumentMetadata::new` and `from_composition` take the priority category,
  none for a document outside Art 14(1) (#2200).
