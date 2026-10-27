- The bridge and `ferrobridge-term`'s `smart` feature take
  `ferrohealth-smart` 0.7.1 in place of 0.7.0: every earlier release can
  render a configured URL's credentials in full (CWE-532,
  GHSA-vphj-3f8c-5hpx). `ferrobridge-term` 0.1.137,
  `ferrobridge-hl7v2` 0.1.152 and `ferrobridge-validator` 0.1.137 carry it, so
  a product pinning `ferrobridge-term` can take the patched crate.
