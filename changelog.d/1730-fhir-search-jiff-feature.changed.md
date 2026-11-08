- `fhir-search` 0.1.129 keeps `jiff` behind its default feature `jiff`.
  With `default-features = false` the escape, token, string, prefix and
  date grammar, the calendar check and the range a date's precision implies
  compile with no time library; `SearchDate::instants`, `approximate`,
  `Instants` and `first_day` need the feature. A consumer that reads
  tokens and escapes alone, such as `ihe-iti`'s PIXm and PDQm sides, turns
  it off.
