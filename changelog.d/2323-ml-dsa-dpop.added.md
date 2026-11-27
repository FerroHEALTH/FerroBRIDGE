- An ML-DSA private key in `dpop_key_file` (PKCS #8, RFC 9881 §6) signs the
  bridge's DPoP proofs with `ML-DSA-44`, `ML-DSA-65` or `ML-DSA-87`
  (RFC 9964), each proof carrying its `AKP` public key, through
  `ferrohealth-smart` 0.36. The DPoP row of the post-quantum inventory moves
  to safe.
