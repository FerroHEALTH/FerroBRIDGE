- `eehrxf`: the `Composition` of every document `Document::into_bundle`
  writes now claims its form's `Composition` profile in `meta.profile`, so a
  received document selects the FHIRconnect context that maps its
  composition.
