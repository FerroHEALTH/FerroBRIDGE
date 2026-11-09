- `[fhir_outbound]` takes its endpoint as one `endpoint` table, `url`,
  `timeout`, `connect_timeout`, `retry`, `tls` and `credentials`, sent
  through `ferrohealth-smart`'s outbound client: a delivery that got no
  answer or a `5xx` is tried again as `retry` allows, an `https` endpoint
  behind a private root or asking for a client certificate is reached with
  the identity and roots `tls` names, and the lane-level `base_url`,
  `timeout` and `credentials` are gone.
