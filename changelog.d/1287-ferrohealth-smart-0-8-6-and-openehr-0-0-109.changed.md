- **`ferrohealth-smart` 0.8.6 and the `openehr-*` crates 0.0.109** (#1287).
  The workspace requires `ferrohealth-smart` as the caret `0.8.6` and the
  nine `openehr-*` crates at 0.0.109, the newest releases on crates.io on
  2026-10-27, and still takes `openehr-sdt` with `flat` alone. An
  `INTERVAL_EVENT`'s `math_function` now reads back through FLAT with the
  openEHR terminology's text for its code, which the `fhirconnect` round-trip
  law records as a change.
  `openehr-mapping-core` 0.1.134, `fhirconnect` 0.1.202, `ferrobridge-term`
  0.1.141, `ferrobridge-hl7v2` 0.1.160 and `ferrobridge-validator` 0.1.141
  follow their requirements.
