- RUSTSEC-2026-0440 (`astral-tokio-tar` is unmaintained, with no patched
  version) does not affect FerroBRIDGE: `testcontainers` reaches the crate only
  in the test harness, and the server binary, the `ferrobridge` image and the
  `ferrobridge-console` image do not contain it. `deny.toml` accepts the
  advisory until a `testcontainers` release drops the crate
  (`security/vex/rust-advisories.openvex.json`).
