- A received discharge, imaging or medical test report's composition names
  the elements it does not carry in its composition-level `Comment`
  (`context/other_context`), where the record's reader sees them, as well as
  in its feeder audit: one comment opening with `COMMENT_LEAD` lists their
  FHIRPaths, never their values. FerroBRIDGE's discharge report template
  gains that `Comment`; the imaging and medical test report templates use the
  one their report result archetype carries. `Project::note` declares the
  comment, and compiling a shipped project against a template without it is
  refused with `MappingError::NoNote` or `MappingError::NoteNotText`.
  When a discharge report holding that comment is sent, the run leaves the
  comment unread and `Project::slice_of` places it in the hospital course
  section, whose narrative the document assembly writes.
