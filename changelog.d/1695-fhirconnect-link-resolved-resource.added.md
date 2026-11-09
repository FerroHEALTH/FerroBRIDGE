- A FHIRconnect `link` whose FHIR side is the resource an enclosing
  `reference` mapping entered (`$fhirRoot` inside it, the shape of the
  published KDS `KDS_composition.yml` extensions) now runs in both
  directions. Going into openEHR the entered resource is handed to the
  `LinkedCompositions` seam under the literal it was entered by, with no
  second fetch; going out the link target becomes the reference the
  enclosing mapping writes, and no copy of the resource is created, declared
  as `SkipReason::LinkedResource` (`skipped-linked-resource` in the facade's
  loss log). Any other `link` side is refused when the program compiles, as
  `fc-link-side`, naming file and line, where it was refused on the first
  run before. `fhirconnect` adds `resolve::program::mapping::LinkSide`,
  `ResolveCode::LinkSide`, `EngineError::LinkedTwice` and
  `SkipReason::LinkedResource`; a consumer that matches one of those enums
  exhaustively adds the variant.
