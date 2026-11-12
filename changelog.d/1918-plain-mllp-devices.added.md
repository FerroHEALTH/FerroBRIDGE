- The HL7 v2 face accepts plain MLLP from a device that cannot speak TLS,
  such as a point-of-care analyser. A sender declared `origin = "device"`
  with `transport = "plain"` and its `source_addresses` (CIDR networks) is
  served on its own `[hl7v2.plain] bind` listener: a connection from outside
  every plain sender's networks is closed before any byte is read, and a
  message whose MSH-4 names no plain sender of the peer's network is answered
  `AR` and its connection closed. The bridge refuses to start with a plain
  sender that is no device or names no networks, a network that is not in
  CIDR notation, a plain listener on the TLS listener's address, or one side
  of the pair without the other. Each plain sender is logged as a warning at
  boot, `/health/info` shows the plain listener and each sender's transport,
  and the operator API's running configuration names each sender's
  `transport` and `source_addresses`. TLS stays the default, and nothing
  changes for an existing configuration. `ferrobridge-hl7v2` 0.10.1 adds
  `Handler::admits`, which closes a peer before its first byte, and
  `Connection::close`, which ends a connection after its answer.
