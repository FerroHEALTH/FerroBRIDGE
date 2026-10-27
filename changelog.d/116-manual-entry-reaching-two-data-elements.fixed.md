- **A `manual` entry whose openEHR paths reach two data elements writes
  both.** The engine merged every path of one entry into the element the
  last path reached, so an entry that set the `current_state` and the
  `careflow_step` below one `ism_transition` wrote only the careflow step,
  with no diagnostic, and the build then defaulted the state. Each data
  element the paths reach is now created, as FHIRconnect's manual mappings
  page says ("Full FHIR or openEHR elements are created from all given
  paths").
