- The `eehrxf` HDR and imaging projects list in `Project::left_out` every
  element of their entries in FerroBRIDGE's templates that no profile
  element carries, as the `euvac` project does: 52 elements of the
  problem or diagnosis, the procedure and the medication order, and 28 of
  the imaging examination result. Each list is held to a run over a
  composition generated from the template with one occurrence of every
  node. `Project::slice_of` names the form section whose entry holds a
  left-out element, and `eehrxf::mapping::left_out_in` reads a run's
  left-out elements for one section, so a written discharge report names
  each one in the narrative of the section its entry fills, by openEHR path
  and never by value.
