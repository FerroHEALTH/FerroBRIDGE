- `eehrxf`: each MyHealth@EU binding names every system its value set draws
  on (`Binding::systems`) apart from the systems a translation is asked for
  (`Binding::targets`). A bound concept that already carries a code in a
  member system is asked of `ValueSet/$validate-code` whether it is in the
  value set. A SNOMED CT non-drug allergen, or an Orphacode rare disease the
  value set holds, is no longer asked for an ATC or ICD-10 code nor reported
  unbound; a SNOMED CT drug allergen still gains its ATC coding.
