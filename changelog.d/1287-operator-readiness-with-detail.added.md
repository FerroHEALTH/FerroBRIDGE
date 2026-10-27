- **`GET /operator/readiness`** behind the operator scope: the readiness
  document with why each indicator is down, which the open route no longer
  names. The operator console reads it, and falls back to the open route
  while the operator API is off.
