- Every TLS 1.3 session the bridge and the console take part in offers the
  hybrid key exchange X25519MLKEM768 (RFC 10024) first, then X25519, P-256
  and P-384 for a peer without it: the HTTP listeners and the endpoint
  clients through `ferrohealth-server` 0.12.4, and the MLLP listener, the
  PostgreSQL clients and every other rustls client through rustls's
  `prefer-post-quantum` feature. `docs/post-quantum.md` and
  `conformance/post-quantum.tsv` list every cryptographic use and whether it
  resists a quantum computer, and the README carries a post-quantum badge
  counted from them.
