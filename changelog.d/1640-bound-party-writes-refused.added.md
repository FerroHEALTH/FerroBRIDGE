- `[facade] binding_identifier_issuers`: a create, update or delete of a party
  resource whose `PERSON` carries a binding key, an identifier of one of the
  listed issuers, is refused with `403 forbidden` before the bridge writes to
  the CDR. A professional bound to a login is provisioned through the CDR's
  SCIM service, never through the facade.
