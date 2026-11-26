- Breaking: `eehrxf` 0.42 writes a document's language as one RFC 5646
  language-region tag built from the openEHR composition's `language` and
  `territory` (`DocumentLanguage::new`, `DocumentLanguage::of_composition`),
  for example `en-GB`, never a bare language.
  `Document::with_language` takes a `DocumentLanguage`, and a form whose
  `Composition` profile binds the language `required` takes only the
  `HeldLanguage` that the terminology server's `ValueSet/$validate-code`
  gives (`DocumentLanguage::held`, `Document::with_held_language`). A refused
  or unreachable terminology answer refuses the language. `Form::language`
  is a `Language` naming whether the profiles require the language and the
  value set they bind it to. `DocumentMetadata::with_language` is removed:
  the metadata, the data groups and the description (`Header::with_language`)
  carry the document's one language.
