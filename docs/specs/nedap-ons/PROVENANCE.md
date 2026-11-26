<!-- This file describes vendored third-party material; the bytes beside it
     keep their upstream licence, not the licence of this repository. -->

# Provenance: the Nedap Ons API OpenAPI document and webhooks specification

Vendored by `scripts/vendor/nedap-ons.sh`, each artefact pinned by its
URL and the sha256 of its bytes. Never edit a file here: change the pins in
the script and the pin-set digest in docs/VERSIONS.md, and re-run the script.

- Pin-set digest (sha256 over the sorted `mode  file  url  sha256` lines of
  the pins): `2d9dd4153ee61b77cca437635fa627197ffa6b7d0353bf8a67271b3f9adbeca1`
- Fetched: 2026-11-26, with the User-Agent `ferrobridge-vendor (scripts/vendor)`
- Artefacts: 0 committed, 2 cache only, 0 needing
  manual retrieval
- Files in this directory: 0 besides this one, each verbatim as the
  publisher serves it
- Tree digest (sha256 over the sorted per-file `sha256  path` listing,
  `PROVENANCE.md` excluded): `e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855`
- Read by: FerroHEALTH/FerroBRIDGE#2065 (inbound-templates/nedap-ons/ and tests/it/inbound/nedap_ons.rs)

The Ons API OpenAPI document and the Ons API webhooks specification, as
`ons-api.nl/assets` serves them. ons-api.nl states no licence for either,
so both are fetched into `.vendor-cache/nedap-ons/` and never committed.
The template's logical models under `inbound-templates/nedap-ons/` are
generated from `components/schemas/Client` and `components/schemas/Bsn` of
the OpenAPI document with `ferrobridge mapping logical-model`.

## Artefacts

### `openapi.json` (cache only, not redistributed)

- Source: <https://ons-api.nl/assets/openapi.json>
- Version: OpenAPI 3.0.3, info.version 0.0.0, fetched 2026-11-26
- sha256: `00fa408a23d22e15714b4990cdd5a27a5297c94e63c0dcf6ebf05a107e0418b2`
- Licence: none stated: ons-api.nl publishes the file with no licence or terms of use, so it is not redistributed
- Not redistributed: the script fetches it into `.vendor-cache/nedap-ons/`, which git
  ignores; this repository carries none of its content.

### `webhooks-spec.yml` (cache only, not redistributed)

- Source: <https://ons-api.nl/assets/webhooks-spec.yml>
- Version: OpenAPI 3.1.0, info.version latest, fetched 2026-11-26
- sha256: `6baa611897ea7420747c9ca29ba91199398b351d28f5006315c622ca94654e2e`
- Licence: none stated: ons-api.nl publishes the file with no licence or terms of use, so it is not redistributed
- Not redistributed: the script fetches it into `.vendor-cache/nedap-ons/`, which git
  ignores; this repository carries none of its content.
