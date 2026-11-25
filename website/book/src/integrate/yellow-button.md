<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# The Yellow Button API

The xShare Yellow Button lets a person download their health data and share
it once. FerroBRIDGE serves it as REST endpoints behind SMART App Launch, and
a patient app builds the pages on top. This page is the guide for that app:
what to call, with which token, and what comes back.

## Signing the person in

Your app is a SMART client of the authorization server `[auth]` names, such
as FerroSMART. It signs the person in with a standalone patient launch (SMART
App Launch 2.x) and asks for at least these scopes:

```text
openid fhirUser launch/patient patient/Patient.rs patient/*.rs
```

The token response names the person's record in `patient`. Every call below
carries the access token as `Authorization: Bearer <token>`. The server finds
its authorization server in `GET /fhir/.well-known/smart-configuration`.

## Discovery

`GET /fhir/metadata` answers the `CapabilityStatement`. When the server
shares links, `rest[0].extension` carries the URL your app creates them at:

```json
{
  "url": "urn:ferrobridge:extension:smart-health-links",
  "valueUrl": "https://bridge.example.org/shl"
}
```

The `Patient` entry lists the `summary` operation when the patient summary is
served.

## Download

```text
GET /fhir/Patient/{patient}/$summary
Accept: application/fhir+json
```

`{patient}` is the `patient` of the token. The answer is the person's patient
summary, an HL7 IPS 2.0.1 document `Bundle`, validated before it is served.
Pass `profile=http://hl7.eu/fhir/eps/StructureDefinition/composition-eu-eps`
for the HL7 Europe Patient Summary form, or
`profile=http://fhir.ehdsi.eu/core/StructureDefinition/composition-myhealtheu-core`
for that form with the MyHealth@EU Core codings. Save the body as a
`application/fhir+json` file to offer it as a download.

## Share once

```text
POST /shl
Content-Type: application/json

{"passcode": "1234", "label": "For my GP", "exp": 1790000000, "answers": 1}
```

Every member is optional:

| Member | Meaning |
|---|---|
| `passcode` | A passcode the receiver must give, 1 to 128 characters. The link then carries the `P` flag; without one it carries `U` |
| `label` | A short description the receiver sees, at most 80 characters |
| `exp` | The expiry in epoch seconds; 24 hours from now by default, at most 7 days by default |
| `answers` | How many times the link may be opened; 1 by default |
| `profile` | The `Composition` profile of the shared summary, as for Download |

The server writes the patient summary, encrypts it once under a fresh key,
keeps only the ciphertext, and answers `201`:

```json
{
  "handle": "4f0c...",
  "shlink": "shlink:/eyJ1cmwiOi...",
  "payload": {"url": "https://bridge.example.org/shl/m/...", "key": "...", "exp": 1790000000, "flag": "P", "label": "For my GP", "v": 1}
}
```

Show `shlink` as a QR code, or prefix it with a viewer URL ending in `#`. This
answer is the only place the key appears: the server keeps no copy, so a lost
link cannot be recovered. Create a new one.

## List and revoke

```text
GET /shl
DELETE /shl/{handle}
```

`GET /shl` lists the person's active links, oldest first, each with its
`handle`, `label`, `created`, `exp`, `passcode` and `answersLeft`. `DELETE`
revokes one and answers `204`. A link that expired, was used up, locked by
wrong passcodes, or revoked is gone from the list.

## What the receiver does

A receiver needs no account. It follows SMART Health Cards and Links 1.0.0:

- With `P`, it posts `{"recipient": "...", "passcode": "..."}` to the
  payload's `url` and gets a manifest whose file has a `location`, which it
  fetches once within the hour, or an `embedded` file when it sends
  `embeddedLengthMax`.
- With `U`, it fetches `url?recipient=...` and gets the file directly.

The file is a compact JWE (`alg` `dir`, `enc` `A256GCM`), opened with the
payload's `key`. A wrong passcode answers `401` with `remainingAttempts`; after
5 wrong passcodes over the link's life, the link is disabled. A link that is
no longer active answers `404`. Each answer, each refused passcode, the
creation, the revocation and the end of a link are recorded in the access
record, with the `recipient` the receiver named.

## Errors

Every refusal of `/fhir` and of the person's `/shl` routes is a FHIR R4
`OperationOutcome`:

| Status | When |
|---|---|
| `400` | A body or a query the route does not take, or a link outside the limits |
| `401` | No valid access token, with a `WWW-Authenticate` challenge |
| `403` | A token without the patient context or the scope the route needs |
| `404` | Another person's record, or a link that is not one of the person's active links |
| `503` | The access record could not be written, so nothing was done |

The receiver routes answer `401` with `remainingAttempts` as the
specification fixes, and an `OperationOutcome` for every other refusal.
