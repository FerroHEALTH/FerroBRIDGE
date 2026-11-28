- `eehrxf`: each MyHealth@EU binding names every system its value set draws
  on (`Binding::systems`) apart from the systems a translation is asked for
  (`Binding::targets`). A SNOMED CT allergen now meets `MYHEALTH_EU_CORE`'s
  allergy binding: it is no longer asked for an ATC code or reported unbound,
  and an Orphacode or ICD-O-3 problem is no longer asked for ICD-10.
