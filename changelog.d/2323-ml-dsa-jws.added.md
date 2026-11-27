- The bridge verifies and signs ML-DSA JWS (RFC 9964): the facade and the
  operator API take an access token signed with `ML-DSA-44`, `ML-DSA-65` or
  `ML-DSA-87` from an issuer whose `algorithms` list names it and whose key
  set publishes an `AKP` key, a `private_key_jwt` client signs its
  assertions with an ML-DSA key from its PEM file, and the operator console
  accepts an ID Token signed with any of the three. Three rows of the
  post-quantum inventory move to safe.
