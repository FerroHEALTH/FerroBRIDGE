- A `fhirconnect` context refuses an extension's `overwrite` or `appendTo`
  that addresses a name two sibling methods share, as
  `fc-duplicate-method-name`, even when no slot of the context reaches the
  model the extension extends. Before, such an edit was refused or accepted
  depending on whether the context reached its model.
