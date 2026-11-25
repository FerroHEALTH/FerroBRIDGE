- `eehrxf` 0.32.0: `Form` has the field `legal_authentication`, and
  `SectionRule` the variant `Entered`, a section that must hold an entry.
  `Document::into_bundle` refuses a document whose form requires a legal
  authentication when its metadata names none.
