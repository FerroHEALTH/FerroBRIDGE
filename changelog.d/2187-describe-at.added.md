- `eehrxf` 0.34: `DocumentMetadata::describe_at` writes the
  `DocumentReference` that describes a document from its `Header`, this
  metadata and an attachment the caller names, with no written `Bundle`, so a
  document list describes each document before it writes it, in the
  MyHealth@EU DocumentReference-minimal shape `describe` writes (#2187).
