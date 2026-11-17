<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# Running behind FerroSMART

FerroSMART is the FerroHEALTH SMART App Launch 2.x authorization server. It
logs a person in at any OpenID Connect provider upstream, for login only, and
issues plain RFC 9068 access tokens for the FerroHEALTH products. FerroBRIDGE
then runs as a resource server: an app takes its token from FerroSMART and
calls the facade with it, and the bridge never sees the provider's tokens.
FerroAUTH's architecture describes the whole deployment in
[§13.21 A product behind FerroSMART](https://github.com/FerroHEALTH/FerroAUTH/blob/main/docs/architecture.md#1321-a-product-behind-ferrosmart);
this page is FerroBRIDGE's side of it.

```text
 browser          SMART app            FerroSMART            IdP (login only)
    | 1. launch      |                      |                        |
    |--------------->| 2. /authorize        |                        |
    |                |  aud = facade base   |                        |
    |<--------------------------------------| 3. OpenID Connect login |
    |---------------------------------------------------------------->|
    | 4. patient picker and consent         |                        |
    |                |<- 5. code, /token -->|                        |
    |                |  RFC 9068 token with the launch context        |
    |                | 6. FHIR call, Bearer token                     |
    |                |--------------------------> FerroBRIDGE /fhir   |
```

## FerroSMART's resource table

FerroSMART issues a token only for a resource it names, so it lists the
facade's base as a FHIR resource:

```toml
[[ferrosmart.resource]]
base = "https://bridge.example.org/fhir"
kind = "fhir"
```

The base is the facade's [`[facade] base_url`](configuration.md#facade), and
it is the `aud` of every token the bridge admits.

## The bridge's issuer table

The bridge trusts FerroSMART as one `[[auth.issuer]]` of kind `persons` under
the `ferrosmart` preset of `ferrohealth-smart`:

```toml
[[auth.issuer]]
issuer = "https://smart.example.org"
kind = "persons"
profile = "ferrosmart"
audience = ["https://bridge.example.org/fhir"]
delegates = [{ client = "growth-chart" }]
```

The preset keeps every `rfc9068` default but two: a token's `typ` is
`at+jwt` alone (RFC 9068 §2.1), and `scope` is a required claim, since
FerroSMART writes the granted scopes into every token. What it admits:

| Token | Read as | Admitted when |
|---|---|---|
| A person's token through a SMART app (`sub` is FerroSMART's identifier of the person, `client_id` the app) | the person, acting through the app | the app's `client_id` is in `delegates` |
| A SMART Backend Services token (`sub` equals `client_id`, RFC 9068 §2.2) | a machine | always, with the `system/` scopes it carries |
| A person's token through any other app | | never: `401` |
| A token with no `scope`, or a `typ` other than `at+jwt` | | never: `401` |

The preset lists no app, because the apps belong to each deployment: list
every SMART app you admit in `delegates` by its `client_id`. Use one
table for FerroSMART: a `persons` issuer already reads its machines' client
credentials tokens, and two `[[auth.issuer]]` tables naming one issuer refuse
the start.

The launch context FerroSMART puts in the token (`patient`, `encounter` and
`fhirUser`) needs no key. A `patient/` scope holds the caller to the patient
its `patient` claim names, as the [facade's Security section](../integrate/fhir-facade.md#security)
describes for any issuer.

## The discovery document

An app that starts at the bridge reads `GET /fhir/.well-known/smart-configuration`
and must find FerroSMART there. With FerroSMART as the bridge's one `persons`
issuer, the [described-issuer rule](configuration.md#the-described-issuer)
picks it, and the document names FerroSMART's `authorization_endpoint` and
`token_endpoint`. With other issuers beside it, set
`[auth] described_issuer = "https://smart.example.org"`. The document and
`GET /fhir/metadata` answer any origin with CORS, and the REST routes answer
the origins [`[facade.cors] allowed_origins`](configuration.md#facadecors)
lists, so list the origin of every browser app.

No specification governs this deployment shape: it is FerroHEALTH's own
design, built from SMART App Launch 2.2.0, RFC 9068 and OpenID Connect.
