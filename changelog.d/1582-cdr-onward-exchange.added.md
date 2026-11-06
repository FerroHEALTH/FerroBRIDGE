- `[cdr.credentials.onward]`: each FHIR facade request reaches the CDR under
  the person who made it. The bridge exchanges the caller's access token per
  request for a token of the CDR's issuer that names the same person, through
  OAuth 2.0 Token Exchange (RFC 8693, `audience` under the Keycloak profile)
  or the Entra ID on-behalf-of JWT bearer grant (RFC 7523), and checks the
  issued token's issuer, audience, lifetime, actor and person before it uses
  it. The token is cached under the caller's token and the target within its
  expiry. A machine keeps the bridge's own client credentials token; a
  patient, another party's organisation, a guest or a Basic user is refused
  with `403`. A failed exchange fails the request with the token endpoint's
  `400` or `403`, a `401` with the claims challenge for `interaction_required`,
  or a `502`, and the bridge's own token is never sent in its place.
