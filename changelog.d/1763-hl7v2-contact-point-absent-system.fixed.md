- `ferrobridge-hl7v2`'s map writes the guide's data-absent-reason `unknown`
  on `ContactPoint.system` for an XTN with neither XTN-3 nor XTN-4, so the
  `ContactPoint` keeps R4's `cpt-2`. The inverse reads a primitive's
  extensions under its `_name` member, so that `ContactPoint` writes back
  with nothing left over.
