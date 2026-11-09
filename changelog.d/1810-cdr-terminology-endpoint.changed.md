- `[cdr]` and `[terminology]` take their server as one `endpoint` table,
  `url`, `timeout`, `connect_timeout`, `retry`, `tls` and `credentials`,
  the family's outbound endpoint table, beside each lane's own keys
  (`demographic` and `onward`; `wire_version` and `display_fallback`). The
  CDR's engine and the terminology client speak the TLS `endpoint.tls`
  names, so a CDR or a terminology server behind a private root, or one that
  asks for a client certificate, is reached. The lane-level `base_url`,
  `timeout`, `retry` and `credentials` are gone, and the environment
  follows (`FERROBRIDGE__CDR__ENDPOINT__URL`).
