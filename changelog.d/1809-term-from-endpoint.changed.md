- `ferrobridge-term` builds on `ferrohealth-smart` 0.16. Behind the `smart`
  feature, `Client::from_endpoint` builds a client from a product's
  `[<lane>.endpoint]` table (smart's `EndpointSection`) through smart's
  `OutboundClient`: the table's timeouts, no redirect, the TLS the product's
  `TlsBuilder` builds, the retry, and the credential (HTTP Basic, or the
  client credentials token renewed once on a `401`).
  `Client::with_http` is removed: a consumer that handed in its own
  `reqwest` client for mutual TLS or private roots names them in the
  table's `tls` and builds through `from_endpoint`. `Credentials::Header`
  carries an `Authorization` value as it stands, and `Error::Endpoint` is the
  outbound client's refusal of the table.
