# fhir-search

The FHIR R4 search value grammar, hand-written. The oracle is FHIR R4
search.html §3.1.1.4 (<https://hl7.org/fhir/R4/search.html>): §3.1.1.4.19 for
the escape, §3.1.1.4.10 for tokens, §3.1.1.4.8 for strings, §3.1.1.4.5 for
prefixes and §3.1.1.4.7 for dates, with the FHIR R4 `dateTime` regex
(datatypes.html) for the lexical bounds of each date part. The facade,
`ihe-atna` (FerroAUDIT) and `ihe-iti` (FerroPIX) read search values through
this crate only; a rule they need goes here, never into a consumer.

## The crate reads values, it does not search

It parses and gives ranges and instants. What a server matches against, and
what it refuses beyond the grammar (the facade's `ap`, AQL `LIKE` wildcards),
stays in the server.

## No value leaves through an error or `Debug`

A token's code may be an identifier and a date a birth date. Errors carry a
kind and at most a byte offset; `Debug` of `Token`, `OfType` and `SearchDate`
shows the form, never the value. `Display` of a date is the explicit way out.

## Exactness

Lexical bounds are exact at any precision (fraction digits are incremented as
text). Instants are at nanosecond resolution, both bounds rounded up, which is
exact for nanosecond targets. Past the year 9999 a high bound is `None`.
