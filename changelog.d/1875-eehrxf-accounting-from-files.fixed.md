- `eehrxf`: a mapping compiled from files (`Mapping::compile`,
  `Mapping::compile_template`) now declares what the shipped project that
  compiles its contexts declares, as `Mapping::compile_project` does. A
  gateway that writes the shipped `IMAGING` files to disk and compiles them
  now tells the sender that a received report's performing organization
  (`DiagnosticReport.performer`) is not carried; before, such a mapping
  declared none of the project's elements. `Mapping::accounting` still
  replaces the list.
