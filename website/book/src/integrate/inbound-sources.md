<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# Inbound sources

A source is a system that sends patient events to the
[inbound intake](inbound.md). Each one is a versioned document in the state
store, written through the operator API
([`/operator/sources`](../operate/operator-api.md#inbound-sources)), so you
add, change, disable or delete a source with no restart and no new port, and
every replica takes the change up. A source is data: the bridge carries no
code for any one source. No specification governs a bridge's source
registry: this is FerroBRIDGE's own design.

## Kinds

| Kind | Sends | Proves itself by |
|---|---|---|
| `webhook` | A JSON notification to `POST /inbound/{source}` | A verifier over the raw body ([Receive and verify](inbound.md#receive-and-verify)) |
| `fhir-subscription` | R4 Subscription notifications ([FHIR Subscription notifications](inbound.md#fhir-subscription-notifications)) | A header or the client certificate |
| `mllp` | HL7 v2 messages over MLLP ([The HL7 v2 face](../operate/hl7v2-face.md)) | Its route: the certificate subject or the network |

## The webhook document

| Part | What it declares |
|---|---|
| `verify` and `keys` | How a notification is verified, and the [named secrets](../operate/secrets.md) it is verified with |
| `events` | How a notification is read: the tenant, the entity, the action and the de-duplication key ([Read the event](inbound.md#read-the-event)) |
| `tenants` | Each customer the source serves, with its local identifier system, its `base` and its `credentials` |
| `limits` | The body size, the remote calls in flight per tenant, and the waiting items before the intake answers `503` |
| `recovery` | The redelivery call and the reconciliation polls ([Recover what an outage lost](inbound.md#recover-what-an-outage-lost)) |
| `resolve` | How the entity is read as an R4 `Patient` ([The resolution](#the-resolution)) |

## The resolution

A notification names only the entity that changed, so a webhook source
declares under `resolve` how the worker reads it and renders it as an R4
`Patient`. No specification governs an inbound resolution: it is
FerroBRIDGE's own design.

```json
{
  "tenants": [{
    "tenant": "t1",
    "identifier_system": "urn:oid:2.16.840.1.113883.2.4.3.11.999.1",
    "base": "https://api.example/t1",
    "credentials": {"secret": "ons-t1-client"}
  }],
  "resolve": {
    "calls": [
      {"path": "/v0/administration/clients/{entity}",
       "headers": {"X-Field-Whitelist": "id,firstName,lastName,dateOfBirth,gender,deathDate"}},
      {"path": "/v0/administration/clients/{entity}/bsn"}
    ],
    "removed_on": 404,
    "map": "https://example.org/StructureMap/ons-client-patient",
    "answer_limit": 262144
  }
}
```

- Each call is a `GET` of a path below the tenant's `base`. Only
  `{entity}` is replaced, percent-encoded (RFC 3986 §2.1), so a write whose
  call would leave the base's scheme, host or path is refused. A `base`
  carries no credentials, query or fragment.
- A call presents the tenant's client certificate, the PEM certificate and
  key in the secret `credentials` names. A credential is never written in
  the document: a call that names an `Authorization`, `Proxy-Authorization`,
  `Cookie` or `X-Api-Key` header is refused.
- Each answer is read as JSON up to `answer_limit` bytes, 256 KiB when left
  out. The answers are the sources of the map's first group, in call order,
  and its one target is the `Patient`.
- `removed_on` on the first call reads as the entity removed: the item
  applies as a remove.
- A `5xx`, `429`, `423` or no answer runs the item again. Any other status
  parks it as `source-answered-<status>`, and an answer past the limit or not
  JSON parks it too.
- `map` is the canonical URL of a FHIR Mapping Language map of the running
  mapping revision. A write naming a map the revision does not run is
  refused, and an item whose map a later revision dropped parks as
  `map-unknown`.

A source without `resolve` parks every item as `resolution-undeclared`.

### Logical models

A map reads the source's JSON through logical models, one
`StructureDefinition` of `kind = logical` per JSON file under the
`structuremap` root of the mapping revision, beside the `.map` files. Every
replica loads them at boot and on every reload. When the source publishes an
OpenAPI document, generate a model from one of its schemas:

```sh
ferrobridge mapping logical-model --openapi ons-openapi.json \
  --schema Client --url https://example.org/StructureDefinition/Client \
  > client.json
```

The job reads no configuration and writes the model to stdout. Save it
through the operator console or `PUT /operator/mappings/structuremap/client.json`.

## Source templates

A template is a source document ready to create through
`POST /operator/sources`, shipped inert (`enabled: false`) under
`inbound-templates/`. Each one is the source's published behaviour written
as data: no source has code of its own in the bridge.

### Nedap Ons

`inbound-templates/nedap-ons.json` reads the Ons API webhooks
(<https://ons-api.nl/english/technical/Webhooks.html>):

- One URL per stage serves every customer, so the tenant is the
  notification's `customerCode`.
- The verifier is the HMAC-SHA512 of the body with the connector secret, in
  lowercase hex in `X-Signature-SHA512`. The NOP pair Ons sends when you
  configure the URL is answered as a probe: `200` for the good signature and
  `401` for the bad one.
- A `client` `CREATE` or `UPDATE` is an upsert and a `DELETE` a remove; every
  other model and event is ignored. A notification carries no event id, so
  the de-duplication key covers `customerCode`, `modelType`, `eventType`,
  `id` and `timestamp`.
- The resolution reads `/v0/administration/clients/{id}` and its `/bsn` below
  the tenant's `base` with an `X-Field-Whitelist` of the identity fields, and
  a `404` reads as removed. Ons allows 4 parallel requests per certificate,
  the template's `limits.concurrency`.
- `recovery.redeliver` is `PUT https://api.ons.io/webhooks/redeliver` with
  the environment certificate, and `recovery.reconcile` polls
  `/v0/xstream/clients/updates` and `/v0/xstream/clients/deletes` with
  `since` every 15 minutes, each answer's `clients` array read by `id`.
- The logical models are generated from `components/schemas/Client` and
  `components/schemas/Bsn` of the Ons API OpenAPI document
  (<https://ons-api.nl/assets/openapi.json>), and the gender ConceptMap maps
  the four values of `components/schemas/ons_administration.Gender`.

Before you create a source from it:

1. Put the connector secret and each certificate in the `[secrets]`
   directory under the names the template uses
   (`nedap-ons-connector-secret`, `nedap-ons-environment-certificate`) and
   one per tenant for its `credentials`.
2. Load the FHIR package `terminology/nedap-ons-values/` into the
   terminology server: the map translates the client's gender through its
   ConceptMap.
3. Save the map `inbound-templates/nedap-ons/client-patient.map` and the
   logical models `client.json` and `bsn.json` under the `structuremap` root
   of the mapping revision.
4. Add each customer as a tenant with its `identifier_system`, `base`
   (`https://api.ons.io` for production, `https://api-staging.ons.io` for a
   customer's test environment) and `credentials`, set `enabled`, and create
   the source.

