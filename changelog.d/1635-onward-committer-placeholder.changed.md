- Under `[cdr.credentials.onward]` the facade lets the CDR derive the
  committer from the exchanged token: a single write sends no
  `openehr-audit-details` header, and a transaction's CONTRIBUTION carries
  the placeholder committer, a `PARTY_IDENTIFIED` whose one `DV_IDENTIFIER`
  names the token's `iss` and `sub`, in its `audit` and every version's
  `commit_audit`.
