- The bridge moves to `ferrohealth-smart` 0.9.0, with `ferrohealth-session`
  0.3.0 for the operator console, `ihe-atna` and `ehds-logging` 0.4.0 for
  the access-record lane, `ferrohealth-server` 0.5.1 and FerroUI 0.6.7, so
  one copy of `ferrohealth-smart` builds into each binary. The access record
  carries no party binding, actor chain or representation of the bridge's
  own. `ferrobridge-term` 0.1.143 requires `ferrohealth-smart` 0.9.0 behind
  its `smart` feature, and `smart::SmartToken` takes the 0.9.0 token
  source: a consumer moves `ferrohealth-smart` to 0.9.0 and pins
  `ferrobridge-term = "=0.1.143"` in one change.
