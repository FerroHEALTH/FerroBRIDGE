- Every span of time and amount of data in the bridge's and the console's
  configuration is a family `Duration` (`"30s"`, `"5min"`) or `ByteSize`
  (`"1MiB"`), and its key names no unit. `[server]` takes the
  `ferrohealth-server` 0.7 keys: `request_timeout`, `header_read_timeout`,
  `idle_timeout`, `body_limit`, `drain_delay`, `shutdown_timeout`,
  `retry_after`, `compression_min_size` and `tls.reload_interval`. The
  bridge's own keys follow: `[cdr]`, `[terminology]`, `[fhir_outbound]` and
  `[facade.references]` `timeout`; `[cdr.retry]` and `[terminology.retry]`
  `initial_backoff` and `max_backoff`; `[hl7v2]` `idle_timeout`,
  `frame_timeout` and `frame_limit`; `[change_feed]` `poll_interval`,
  `backoff` and `max_backoff`; `[events]` `confirm_timeout`; `[audit]`
  `max_size` and `write_timeout`; and the console's
  `[bridge] timeout`. A file or an environment that sets an old key is
  refused as an unknown key; rewrite it in the new form.
