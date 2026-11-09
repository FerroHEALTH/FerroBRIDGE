- `ferrobridge-term` sends a whole authored `ValueSet` as the `valueSet`
  input of `ValueSet/$expand` (`request::ValueSetSource::Authored`), with its
  enumerated concepts, imported value sets, excludes and `compose.inactive`,
  in every release it speaks, and refuses one the release cannot read with
  `Error::AuthoredValueSet` before sending. `invocation::expand_answer` reads
  an `$expand` answer without the request that asked for it.
