- **The server runs on the FerroSYS crates** (#1287): `ferrohealth-config`,
  `ferrohealth-secret`, `ferrohealth-telemetry`, `ferrohealth-health`,
  `ferrohealth-server` and `ferrohealth-usage-report`, pinned exactly, in
  place of its own configuration loader, console, request log, request id,
  panic catch, health registry, drain and usage report. Every key and
  variable keeps its name. What an operator sees change: a secret renders as
  `***`; a `_file` secret loses its trailing line ending alone and an empty
  file is refused; a `[telemetry] filter` that does not parse refuses the
  start; a refusal names the closest known key and several are reported at
  once; `[server.caller_rate]` and `[telemetry.otlp]` are refused; a
  response is compressed when the client accepts gzip or Brotli. The
  readiness document is the family's: `phase` beside `state`, the indicators
  as a list sorted by name, readiness `503` while the process boots or
  drains, `GET /health` answers the state alone, and the open route names no
  indicator's detail. The request log never logs the client's
  `X-Request-Id`: the line, a panic or refusal body and the CDR calls name the
  server's own id, and the client's is echoed on the response. The panic line
  never names the panic message. The banner is the family's, and
  `/health/info` names the commit in seven characters. The usage report
  leaves out a counter that stayed at 0.
