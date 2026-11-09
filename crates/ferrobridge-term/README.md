<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# ferrobridge-term

The FerroBRIDGE FHIR terminology client for `$lookup`, `$translate`,
`$validate-code`, `$expand` and `$subsumes`.

Part of [FerroBRIDGE](https://ferrobridge.eu), a pure-Rust bridge between
openEHR and two interoperability targets: HL7 FHIR through the FHIRconnect
specification, and the OMOP Common Data Model through the OMOCL specification.

## What it does

Five operations on one configured server:

- `CodeSystem/$lookup` resolves a code's display; `lookup_at` asks for it as
  the code system stood on a `date`.
- `ConceptMap/$translate` maps a code into another code system.
- `ValueSet/$validate-code` tests membership of a value set, at an optional
  `date`.
- `ValueSet/$expand` lists the codes of a value set, with `valueSetVersion`,
  `date`, `filter`, paging (`offset` and `count`) and `includeDesignations`.
  The answer keeps the nesting of `expansion.contains`, and each entry
  carries its `system`, `version`, `code`, `display`, `abstract`, `inactive`
  and designations.
- `CodeSystem/$subsumes` tests how two codes relate, answered as
  `SubsumesOutcome`: `Equivalent`, `Subsumes`, `SubsumedBy` or `NotSubsumed`.

`code_system_count` asks `GET [base]/CodeSystem?url=<system>&_summary=count`,
so a caller can tell an unserved code system from an unknown code.

`validate` takes the input forms `$validate-code` declares: a code with its
system, `systemVersion` and `display`, a code with the `context` that implies
its system (`Coded::InContext`, R4 and R4B), a `Coding`, or a
`CodeableConcept`, with an optional `valueSetVersion` (R4 and R4B). `validate_all` answers several validations
by position, one call for one and one `batch` for several.

`batch` sends several of them as one `batch` `Bundle` and answers them by
position; an entry the server refused is that entry's own typed error and
leaves the others alone.

A negative answer an operation states in its own `out` parameters is an
outcome: `LookupOutcome::NotFound`, `TranslateOutcome::NoMatch`,
`ValidateOutcome::Invalid`, and `ExpandOutcome::NotFound` for a value set the
server does not know. An expansion the server could not make, `too-costly`
among them, is a refusal and never an empty expansion. Everything else is a typed error carrying the
upstream status, the body, and any `tx-issue-type` coding the
`OperationOutcome` holds. A missing display is `NotFound`, never an empty
string, so the caller decides between a refusal and a configured fallback.

`translate` returns every match with its equivalence and offers `accepted()`,
which yields the `equivalent` and `equal` matches alone, and the `equivalent`
relationship of R5 and R6. The `$translate`
definition asks the caller to check `match.equivalence` for each match, so a
`wider`, `narrower` or `inexact` answer never reaches a target system as a
translation.

The request and response contracts come from the generated
[`fhir-types`](https://docs.rs/fhir-types) operation modules; this crate models
no FHIR of its own.

## Any request, the `GET` form, and your own transport

`Client::send` sends any `request::Request`: the five operations above,
`CodeSystem/$validate-code` (`Request::CodeSystemValidateCode`), and the
instance-level forms (`ValueSet/[id]/$validate-code`,
`CodeSystem/[id]/$validate-code`, `CodeSystem/[id]/$subsumes`), named with
`request::Target::Instance`. `$expand` also takes an inline value set
(`request::ValueSetSource::Inline`), sent as the `valueSet` resource, and the
inputs `activeOnly`, `excludeNested` and `displayLanguage`.

`Config::with_form(Form::Get)` invokes each operation as a `GET` with its
inputs in the query, which the operations framework allows when no input is
complex; a request with a `Coding`, a `CodeableConcept` or an inline value set
goes as a `POST`.

`invocation::Invocation::new` writes a request (method, path segments, query,
body) without sending it, and `invocation::answer` reads a status and a body
exactly as the client does. A caller that sends through a transport of its
own, such as a server forwarding a signed-in user's token, uses the two and
writes no terminology client of its own.

The answers carry what the operations declare beside the verdict:
`ValidationDetails` (`code`, `system`, `version`, `codeableConcept`,
`normalized-code`, `inactive`, `status`, the unknown systems) and the issues of
a true answer too, every `details.coding` and `expression` of each issue, the
`used-conceptmap` list and each match's `relationship`, `noMap`,
`sourceConcept`, `originMap`, comments and parts, the echoed
`expansion.parameter` and the `valueset-unclosed` mark, and R5's `$lookup`
`definition`.

## Five wire versions

`WireVersion` selects STU3, R4, R4B, R5 or the R6 ballot (6.0.0-ballot5), and
the release decides which generated model writes the request and reads the
answer. Each release is a feature of this crate, `stu3`, `r4`, `r4b`, `r5` and
`r6`, and a release whose feature is off has no `WireVersion` variant, so a
call in it does not compile. The default is `stu3`, `r4` and `r4b`; a
consumer that speaks R5 or the R6 ballot turns `r5` or `r6` on:

```toml
ferrobridge-term = { version = "=0.1.147", features = ["r5", "r6"] }
```

R5 and R6 are off by default because a binary that also enables the
`fhir-types` `resources` feature would compile every resource of those
releases, which a 16 GB hosted runner cannot build in release. R4 to R6 declare the same `in` and `out` parameters outside
`$translate`. `$translate` is spelled three ways: R4 and R4B take `code`,
`system`, `version` and `targetsystem` and answer `match.equivalence`; R5 takes
`sourceCode` and `targetSystem` and answers `match.relationship`; R6 also
renames `system` and `version` to `sourceSystem` and `sourceVersion`. R5 and R6
have no `reverse`, so they refuse it. STU3 (3.0.2) differs:

- STU3 defines no `CodeSystem/$validate-code`, so an STU3 client refuses it
  with `Error::UnsupportedOperation`.

- `$validate-code` names the code system version `version` where R4 names it
  `systemVersion`, and the client sends it under that name. STU3 has no
  `valueSetVersion` and no `activeOnly`.
- `$validate-code` requires a system for a code, where R4 and R4B accept a
  `context` in its place, so an STU3 client refuses `Coded::InContext` with
  `Error::SystemRequired` before anything is sent.
- `$expand` has no `valueSetVersion` either.
- `$translate` has no `url` and no `conceptMapVersion`, so STU3 cannot name
  the concept map to use.
- `$lookup` types `property.value` as `code`, so every property value of an
  STU3 answer is a `valueCode`.
- `$translate` answers the same `match` parts (`equivalence`, `concept`,
  `product`, `source`) with the same `ConceptMapEquivalence` codes, so
  `accepted()` reads both releases the same way.

A request that names a parameter STU3 has no equivalent for (a concept map URL
on `translate`, a `value_set_version` on `validate` or `expand`) is refused with
`Error::UnsupportedParameter` before anything is sent. In a batch the whole
batch is refused. The request is never sent with the parameter dropped,
because that would ask the server a different question.

A server that serves several releases side by side puts the release in its
base path, so the base URL and the wire version are configured together.

## An absent server

`Client::new` and `Client::with_http` are the only ways to get a client, so a
deployment that configures no terminology server holds `Option<Client>` and
the absence is visible at every call site. Keep it that way: a `None` is a refusal to translate, not a
reason to pass a source code through under a target system.

The `Client::translate` documentation carries the worked example.

## Mutual TLS and private trust roots

`Client::with_http` takes an HTTP client you build, so a server that asks for
a client certificate, or whose certificate chains to a private root, is
reached with the `reqwest::Identity` and `reqwest::Certificate` you give that
client. The configured timeout still bounds each request, and the
credentials, the retry budget and the typed errors apply as for
`Client::new`.

## The client credentials token of ferrohealth-smart

Behind the `smart` feature, `smart::SmartToken` turns the `TokenSource` of
`ferrohealth-smart`'s client credentials grant into the client's
`TokenProvider`: each request carries the cached token, and a token the
server refuses with `401` is dropped, so the repeated request carries one
fetched anew (RFC 6750 §3.1). Pass it as `Credentials::OAuth`; no product
writes this adapter itself.

## Status

The crate is on the crate line (`docs/VERSIONS.md`): every change moves it one
patch, a breaking change included, so pin it exactly.

## Licence

Business Source License 1.1 (`LICENSE`): free for every non-production use and
for non-commercial production use; a commercial licence for other production
use; Apache License 2.0 four years after each version.
