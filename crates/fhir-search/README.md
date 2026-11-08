<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# fhir-search

The FHIR R4 search value grammar
([search.html](https://hl7.org/fhir/R4/search.html) §3.1.1.4): the escaped
list of alternatives, the token forms, the string matches and the date with
its prefix and the range its precision implies.

Part of [FerroBRIDGE](https://ferrobridge.eu), a pure-Rust bridge between
openEHR and two interoperability targets: HL7 FHIR through the FHIRconnect
specification, and the OMOP Common Data Model through the OMOCL specification.
The FerroHEALTH FHIR servers read their search values through this one crate.

## What it does

A server percent-decodes a parameter's value and reads it in two steps:

- `escape::split` cuts the value into its alternatives at every `,` no `\`
  escapes. `escape::unescape` removes the escapes of one part and refuses a
  `\` before any character other than `\`, `,`, `$` and `|`, as
  §3.1.1.4.19 requires ("The parameter value `xx\xx` is illegal").
  `escape::escape` writes a literal value a server reads back unchanged.
- `token::Token` reads `[code]`, `[system]|[code]`, `|[code]` and
  `[system]|`, and `token::OfType` reads the `:of-type` form
  `system|code|value` (§3.1.1.4.10).
- `string::StringMatch` is the default, `:contains` and `:exact` match, and
  `string::fold` is the form the first two compare: case, combining marks,
  punctuation and repeated whitespace removed (§3.1.1.4.8).
- `prefix::Prefix` holds the nine prefixes of §3.1.1.4.5, `ap` included.
- `date::DateValue` reads a prefix and a date to any precision from the year,
  with the time in minutes, seconds or any number of fraction digits and an
  optional offset (§3.1.1.4.7). `SearchDate::range` gives the half-open
  range its precision implies, written at that precision, and
  `SearchDate::instants` gives the instants of that range, with a date that
  has no offset read in the server's zone. `SearchDate::approximate` widens
  the range by the recommended 10% of its gap to now, for `ap`.

The default feature `jiff` carries the instants, `approximate` and
`first_day`. With `default-features = false` everything else compiles with
no time library, for a consumer that reads tokens and escapes alone.

No error and no `Debug` output carries a value: a code may be an identifier
and a date a birth date.

```rust
use fhir_search::date::DateValue;
use fhir_search::escape::split;
use fhir_search::prefix::Prefix;
use fhir_search::token::{System, Token};

let tokens: Vec<Token> = split(r"http://loinc.org|8480-6,a\,b", ',')
    .iter()
    .map(|part| Token::parse(part))
    .collect::<Result<_, _>>()?;
assert_eq!(tokens[0].system(), System::Is("http://loinc.org"));
assert_eq!(tokens[1].code(), Some("a,b"));

let date = DateValue::parse("ge2026-12")?;
assert_eq!(date.prefix(), Prefix::Ge);
let range = date.date().range();
assert_eq!(range.low().to_string(), "2026-12");
assert_eq!(range.high().map(ToString::to_string).as_deref(), Some("2027-01"));
```

## Licence

Business Source License 1.1 (`LICENSE`): free for every non-production use and
for non-commercial production use; a commercial licence for other production
use; Apache License 2.0 four years after each version.
