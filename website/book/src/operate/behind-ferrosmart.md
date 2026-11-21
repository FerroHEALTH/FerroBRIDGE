<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# Running behind FerroSMART

FerroSMART is the FerroHEALTH SMART App Launch 2.x authorization server. It
logs a person in at any OpenID Connect provider upstream, for login only, and
issues plain RFC 9068 access tokens for the FerroHEALTH products. FerroBRIDGE
then runs as a resource server: an app takes its token from FerroSMART and
calls the facade with it, and the bridge never sees the provider's tokens.
FerroSMART's architecture describes the whole deployment in
[§13.21 A product behind FerroSMART](https://github.com/FerroHEALTH/FerroSMART/blob/main/docs/architecture.md#1321-a-product-behind-ferrosmart);
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

The bridge trusts FerroSMART as one named issuer table under the `ferrosmart`
preset of `ferrohealth-smart`:

```toml
[auth.issuer.ferrosmart]
issuer = "https://smart.example.org"
preset = "ferrosmart"

[auth.issuer.ferrosmart.profile]
audience_delegates = true
```

The preset gives the issuer its kind, `persons`, and its audience is the
facade's `base_url`, so neither needs a key. It keeps every `rfc9068` default
but these: a token's `typ` is `at+jwt` alone (RFC 9068 §2.1), `scope` is a
required claim, since FerroSMART writes the granted scopes into every token,
the `patient` claim is the launch context (`patient_context`), and a token
whose client is FerroSMART itself, its patient picker, is the person with no
app grant (`issuer_client_is_person`). What it admits:

| Token | Read as | Admitted when |
|---|---|---|
| A person's token through a SMART app (`sub` is FerroSMART's identifier of the person, `client_id` the app) | the person, acting through the app | the app is a delegate: its `client_id` is in `delegates`, or `audience_delegates` is on and the token's `aud` names the facade |
| A person's token through FerroSMART's own picker (`client_id` is FerroSMART) | the person | always, with no delegate entry |
| A patient's own token through a SMART app: `fhirUser` is `Patient/<x>` (relative, or an absolute URL ending in `/Patient/<x>`), the `patient` claim is `x`, its scopes are `patient/` grants with no `user/` or `system/` one, and it carries no `act` | the patient, the person's own access | the app is a delegate |
| A SMART Backend Services token (`sub` equals `client_id`, RFC 9068 §2.2) | a machine | always, with the `system/` scopes it carries |
| A person's token through any other app | | never: `401` |
| A token with no `scope`, or a `typ` other than `at+jwt` | | never: `401` |

With `audience_delegates = true`, every app you register at FerroSMART with
the facade among its `resources` is admitted, and FerroSMART issues a token
for the facade to no other app. To admit only the apps you list, leave it off
and name each by its `client_id`:

```toml
[auth.issuer.ferrosmart.profile]
delegates = [{ client = "growth-chart" }]
```

Use one table for FerroSMART: a `persons` issuer already reads its machines'
client credentials tokens, and two issuer tables naming one issuer refuse the
start. Every other key of the issuer table (`algorithms`, `clock_skew`,
`jwks_max_age` and the rest) and every profile key is on the
[`[auth]`](configuration.md#auth) page.

The launch context FerroSMART puts in the token (`patient`, `encounter` and
`fhirUser`) needs no key. A `patient/` scope holds the caller to the patient
its `patient` claim names, as the [facade's Security section](../integrate/fhir-facade.md#security)
describes. The bridge reads that claim from no issuer whose profile leaves
`patient_context` off, the same rule FerroEHR reads.

A patient's own token is the person's own access (Regulation (EU) 2025/327
Art 9). The bridge reads it as `ferrohealth-smart` classifies it
(`Caller::is_the_person`), so a professional who holds `patient/` scopes
stays a professional held to the launch context. For the patient's own
token, the facade's access record carries the binding `self`. With
[`[cdr.endpoint.credentials.onward]`](configuration.md#cdrendpointcredentialsonward)
naming FerroSMART, the bridge exchanges the patient's token there unless a
`patients` issuer is declared. To read every FerroSMART token as a
professional's, set `fhir_user_patient = false` in the issuer's `profile`
table.

## The discovery document

An app that starts at the bridge reads `GET /fhir/.well-known/smart-configuration`
and must find FerroSMART there. With FerroSMART as the bridge's one `persons`
issuer, the [described-issuer rule](configuration.md#the-described-issuer)
picks it, and the document names FerroSMART's `authorization_endpoint` and
`token_endpoint`. With other issuers beside it, set
`[auth] described_issuer = "ferrosmart"`, the name of its table. The document and
`GET /fhir/metadata` answer any origin with CORS, and the REST routes answer
the origins [`[facade.cors] allowed_origins`](configuration.md#facadecors)
lists, so list the origin of every browser app.

No specification governs this deployment shape: it is FerroHEALTH's own
design, built from SMART App Launch 2.2.0, RFC 9068 and OpenID Connect.
