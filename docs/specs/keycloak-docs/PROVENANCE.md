<!-- This file describes vendored third-party material; the bytes beside it
     keep their upstream licence, not the licence of this repository. -->

# Provenance: the Keycloak token exchange documentation

Vendored verbatim by `scripts/vendor/keycloak-docs.sh`. Never edit a file
here: change the pin in docs/VERSIONS.md and re-run the script.

- Source: <https://github.com/keycloak/keycloak>, rendered as the Keycloak 26.8.2
  guides at <https://www.keycloak.org/documentation>
- Pin: tag `26.8.2`, which resolves to commit `e514445f9dcdabc5bdcf67cab56c1e49655de067` (2026-11-05T15:27:24Z), the
  release FerroHEALTH/FerroAUTH vendors
- Fetched: 2026-11-05, each file from raw.githubusercontent.com at the commit
  and checked against the git blob id the commit records
- Upstream licence: the repository's `LICENSE.txt`, the Apache License,
  Version 2.0, January 2004, vendored beside this file. Its §4 permits
  reproducing and distributing copies of the work in any medium, with a
  copy of the licence given to every recipient. The files are unmodified.
- Layout: the upstream paths, unchanged
- Files: 3
- Tree digest (sha256 over the sorted per-file `sha256  path` listing,
  `PROVENANCE.md` excluded): `15158948f28b4bc436076165ce20d4bf82a18c391cb11c32035f8583b919f9f4`
- Read by: FerroHEALTH/FerroBRIDGE#1582 (the `token_exchange` onward
  grant and its Keycloak example profile)

## What is taken

- `docs/guides/securing-apps/token-exchange.adoc`: "Configuring and using
  token exchange", the standard (V2) and legacy (V1) exchange, delegation
  and the `audience` parameter. In Keycloak 26 the chapter lives in the
  securing applications guide.
- `docs/guides/templates/features.adoc`: the `techpreview` and
  `techpreviewdeprecated` macros, whose text the chapter's preview notices
  render.

The guides' images and every other chapter are not taken.

| File | sha256 | git blob id |
|---|---|---|
| `LICENSE.txt` | `cfc7749b96f63bd31c3c42b5c471bf756814053e847c10f3eb003417bc523d30` | `d645695673349e3947e8e5ae42332d0ac3164cd7` |
| `docs/guides/securing-apps/token-exchange.adoc` | `99d6d660675f2d4d35cbdc0825ffab7f5aeeb39d970c580340b1d472a72d7afa` | `8b6b4f866dbfe084d062723687eadd2ec71e5abe` |
| `docs/guides/templates/features.adoc` | `46d92510c4e2cafc8780518f1932784d29359a1c8088a8b643bc63c044d0dc03` | `9a6a96620d13ee8a9ba715240f3607e02e1e1576` |
