- The HL7 v2 face serves MLLP over TLS and requires a client certificate
  that chains to `[hl7v2.tls] client_ca_file`. Each `[[hl7v2.senders]]`
  entry names the `certificate_subject` its certificate carries, and a
  message is accepted only on a connection whose certificate names the
  sender its MSH-4 names; otherwise it is answered `AR`. The bridge refuses
  to start without `[hl7v2.tls]` `certificate_file`, `key_file` and
  `client_ca_file`, without a sender, or with a sender that names no
  `certificate_subject`. Upgrade step: issue each sender a client
  certificate, add its DNS name as `certificate_subject`, and configure
  `[hl7v2.tls]`. `ferrobridge-hl7v2` gains `mllp::listener::serve_tls`,
  `mllp::tls` and `Connection::certificate`.
