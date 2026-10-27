- **`ferrohealth-smart` 0.8** (caret `0.8.3`) for the workspace and
  `ferrobridge-term`'s `smart` feature (`ferrobridge-term` 0.1.140, with
  `ferrobridge-hl7v2` 0.1.159 and `ferrobridge-validator` 0.1.140), so a
  product on the 0.8 line hands its `TokenSource` to the terminology client.
  This breaks the `smart` feature's API for a consumer still on
  `ferrohealth-smart` 0.7: it moves to 0.8 with this release (#1285).
