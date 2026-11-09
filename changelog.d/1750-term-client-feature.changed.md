- `ferrobridge-term` 0.1.151 puts its HTTP client (`reqwest`, the retry
  budget, the credentials and the token provider) behind a default `client`
  feature, composed with the per-release features; `smart` implies it. With
  `default-features = false` the crate keeps the request, concept and outcome
  types and the transport-free writers and readers. A consumer that turns
  default features off and calls a server adds `features = ["client"]`.
