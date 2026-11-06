- A Basic credential of `[[auth.basic.users]]` is always a person's, never a
  machine's, and a Basic user listing a `system/` scope refuses the start,
  naming the user. Upgrade step: give each machine on Basic, such as an ETL
  client, a client credentials client (RFC 6749 §4.4) of a `machines` or
  `persons` issuer the section trusts, with the `system/` scopes it needs,
  and keep on Basic only people, with `user/` scopes or the operator scope.
