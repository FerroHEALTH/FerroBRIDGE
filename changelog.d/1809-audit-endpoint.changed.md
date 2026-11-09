- `[audit]` takes its Audit Record Repository as one `endpoint` table,
  `url`, `timeout`, `connect_timeout`, `retry`, `tls` and `credentials`,
  sent through `ihe-atna` 0.10's FHIR Feed over `ferrohealth-smart`'s
  outbound client. `retry` is the tries of one delivery, and its
  `max_backoff` the wait between two rounds; the lane-level `base_url`,
  `timeout`, `retry_max` and `credentials` are gone.
