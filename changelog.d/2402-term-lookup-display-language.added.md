- `ferrobridge-term`: `Client::lookup_in` sends `CodeSystem/$lookup` with the
  `displayLanguage` input. The facade asks for a coding's designation in a
  requested language through it, so a server that returns designations only
  on request answers in that language.
