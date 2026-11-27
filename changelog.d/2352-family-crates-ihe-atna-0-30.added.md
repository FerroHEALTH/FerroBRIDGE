- `[audit.regulatory]` declares the regulatory metadata every access record
  carries (Xt-EHR D5.2 §7.2.7): the legal basis as `legal_basis.lawfulness`
  (GDPR Art 6(1)) and `legal_basis.condition` (Art 9(2)), and the
  `controllers` and `processors` by an identifier the deployment chooses.
  Left empty, a record names none. The server moves to `ihe-atna` and
  `ehds-logging` 0.30.0 and `ihe-iti` 0.27.0, which carry it.
