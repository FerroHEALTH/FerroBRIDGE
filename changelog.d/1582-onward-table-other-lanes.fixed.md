- An `onward` table under `[terminology.credentials]`, `[audit.credentials]`
  or `[fhir_outbound.credentials]` refuses the start, and so does
  `[cdr.credentials.onward]` on a server whose `[auth]` is off. Before, the
  table loaded and was ignored.
