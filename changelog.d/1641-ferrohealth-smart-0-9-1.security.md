- A Basic user of `[[auth.basic.users]]` holding a `system/` scope is
  refused by `ferrohealth-smart` 0.9.1 (GHSA-jhg2-jwjp-hq74, CWE-269,
  CWE-863), which names the user and the scope, and a Basic caller never
  carries a `system/` scope at request time. The bridge's own start-up
  check for the same rule is removed in its favour. Upgrade step: give each
  Basic user the `user/` scopes a person holds, and move each machine on
  Basic to a client credentials client (RFC 6749 §4.4) of a `machines` or
  `persons` issuer.
