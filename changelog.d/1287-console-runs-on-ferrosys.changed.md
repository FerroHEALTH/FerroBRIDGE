- **The operator console runs on the FerroSYS crates** (#1287): its
  configuration through `ferrohealth-config` and `ferrohealth-secret` (every
  `FERROBRIDGE_CONSOLE__` variable keeps its name, `--set` joins them,
  `RUST_LOG` is the alias of the new `[telemetry] filter`), its log through
  `ferrohealth-telemetry`, and its security headers, same-origin check,
  health routes, drain, healthcheck and subcommands through
  `ferrohealth-server`. `config check` prints the running configuration with
  every secret as `***`; `healthcheck` asks `/health/readiness`; `GET /health`
  answers `{"state":"up"}`; a `_file` secret loses its trailing line ending
  alone; the 403 of a cross-site post is in the console's configured
  language.
