- `eehrxf::document::Document::with_mapped_composition` keeps the
  `emptyReason` a mapping run writes on a section, in the new
  `MappedSection::empty_reason` field, and `MappedSection::into_section`
  carries it into the document, which writes the concept as the run wrote
  it (a code of `list-empty-reason`, or LOINC `LA4720-4`, which
  `SectionEmptyReasonEuImaging` also includes). Before, the reason was
  dropped and the document refused the empty section. A consumer that
  writes a `MappedSection` literal adds the field, and
  `Section::empty_because` is no longer a `const fn`.
