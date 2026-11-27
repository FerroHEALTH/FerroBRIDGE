- `eehrxf`: `Translator::with_designations` gives each coding a translation
  adds the terminology server's designation in a requested BCP 47 language,
  through `CodeSystem/$lookup`, as its `display` or in the R4 `translation`
  extension of its display; a refused lookup fails the translation.
