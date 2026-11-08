- `openehr_mapping_core::index::WebTemplateIndex::build_party` and
  `accept_party` validate the party with
  `openehr_sdt::rm_instance::validate_party` (the template, the RM
  invariants and the RM-mandated openEHR terminology), as a composition is
  validated, and refuse one that breaks them with the new
  `PartyError::Unvalidated`, which carries the validation's messages. The
  FHIR facade answers a served party its template refuses with a `502`, and
  `$toopenehr` refuses a mapped party that does not validate. A consumer
  that matches `PartyError` exhaustively adds the variant.
