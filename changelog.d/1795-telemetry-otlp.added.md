- The bridge and the operator console export their traces and metrics to an
  OpenTelemetry collector under `[telemetry.otlp]`, dialing an `https://`
  collector through the platform verifier with the trust roots, client
  identity and minimum version of `[telemetry.otlp.tls]`.
