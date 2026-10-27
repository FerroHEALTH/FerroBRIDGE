- **The operator console signs operators in through `ferrohealth-smart`'s
  OpenID Connect relying party** (`oidc::RelyingParty`), in place of its own
  authorization request, PKCE, code exchange, ID Token check, refresh and
  end-session request. The `[oidc]` keys, the sign-in and sign-out routes
  and the sign-in page's states are unchanged.
