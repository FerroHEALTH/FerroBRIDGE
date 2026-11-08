- Breaking for a `ferrobridge-hl7v2` consumer: `map::map` takes an
  `Option<map::translate::Translator>` in place of an
  `Option<&ferrobridge_term::client::Client>`; pass
  `Some(Translator::Server(&client))` for the behaviour of before. A table
  map's `TableGroup` carries its elements (`TableElement`, `TableTarget`).
