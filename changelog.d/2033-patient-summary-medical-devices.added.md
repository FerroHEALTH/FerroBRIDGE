- The patient summary carries medical devices: FerroBRIDGE's patient summary
  template adds a medical devices section (`EVALUATION.device_summary.v0`,
  vendored from CKM), and `eehrxf` 0.25.0's EPS project maps each medical
  device summary to a `deviceUseStatement-eu-eps` and a `device-eu-eps`
  `Device` with its SNOMED CT type. `Patient/$summary` fills the EPS medical
  devices section from the record, and a record with no device answers it
  `nilknown`.
