- `ferrobridge-term` 0.2.0 builds on `ferrohealth-smart` 0.15, whose
  `TokenSource` its `smart` feature takes, so a consumer requires
  `ferrobridge-term = "0.2"` and moves to `ferrohealth-smart` 0.15 in the
  same change. `ferrobridge-validator` 0.2.0, `eehrxf` 0.2.0 and
  `ferrobridge-hl7v2` 0.3.0 take the minor step with it, since each requires
  `ferrobridge-term` exactly.
