- The `fhir-search` crate, the one FHIR R4 search value grammar
  (search.html §3.1.1.4) the FerroHEALTH FHIR servers read search values
  with: the split at unescaped `,`, `|` and `$`, `unescape` refusing an
  escape of any other character, `escape`, the four token forms and
  `:of-type`, the default, `:contains` and `:exact` string matches with the
  fold they compare, the nine prefixes, and the date to any precision with
  the range its precision implies, its instants in a caller's zone and the
  recommended `ap` margin. The facade reads its search values through it, so
  a token with more than one unescaped `|`, an unknown letter prefix and a
  year `0000` are refused as the grammar says, a date with more than nine
  fraction digits is read exactly, and a date in the year 9999 is read with a
  range open at the end of the calendar.
