- The `eehrxf` `euvac` project declares every element of the template's
  medication action it carries into no `vaccination-myhealtheu-euvac`
  element, as `Project::left_out`, and a written vaccination history names
  each one a composition holds in its immunisation section's narrative, by
  its openEHR path and never by its value: `eehrxf::mapping::left_out`
  reads the paths from a run's warnings, and `Section::with_left_out` adds a
  paragraph for each. The history stays valid against MyHealth@EU
  Vaccination 10.0.0-preview `bundle-euvac-myhealtheu`.
