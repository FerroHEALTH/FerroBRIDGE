- SMART Health Links behind `[shl]`: a person with a `patient/` token creates
  a one-time share of their patient summary at `POST /shl`, lists their
  active links at `GET /shl` and revokes one at `DELETE /shl/{handle}`; a
  receiver resolves it without an account by the manifest request or, under
  `U`, the direct file request (SMART Health Cards and Links 1.0.0). The file
  is a compact JWE (`dir`, `A256GCM`) under a key only the link carries; the
  store keeps the ciphertext and the passcode's Argon2id hash, and deletes the
  link at expiry, revocation, lockout or last use. A link expires after 24
  hours by default and 7 days at most, answers once by default, takes 5 wrong
  passcodes over its life, and hands out single-use file locations that lapse
  within the hour. Each creation, answer with its `recipient`, refused
  passcode, revocation and end is a BALP `AuditEvent` in the access record,
  and the `CapabilityStatement` names where links are created.
