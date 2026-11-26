<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# SMART App State and launch Tasks

**Audience:** integrators building SMART apps and EHR viewers.

The two SMART App Launch 2.2.0 resources the facade keeps in the state store,
because the CDR holds neither.

## SMART App State

With `[facade.app_state]` on, `Basic` at the facade's base is the SMART App
State endpoint of SMART App Launch 2.2.0, Persisting App State: an app keeps
a small amount of its own state at the EHR, such as display preferences or
the keys to an external data set. The CDR holds no `Basic`, so the facade
keeps these resources in the state store, apart from the identity map, so
every replica serves the same ones. The
discovery document lists `smart-app-state`, and the `CapabilityStatement`
instantiates `smart-app-state-server` and lists `Basic` with the
`smart-app-state-basic` profile.

| Interaction | Route | Answer |
|---|---|---|
| create | `POST [base]/Basic` | `201` with `Location`, `ETag: W/"1"` and the resource with its `id` and `meta.versionId` |
| update | `PUT [base]/Basic/{id}` with `If-Match` | `200` with the next `ETag` |
| delete | `DELETE [base]/Basic/{id}` with `If-Match` | `204` |
| search | `GET [base]/Basic?code={system}\|{code}&subject={reference}` | a `searchset` of the matching state |
| search, global | `GET [base]/Basic?code={system}\|{code}&subject:missing=true` | a `searchset` of the state with no subject |

The facade refuses with `400` a body that breaks the profile or the page's
rules: an `id` or `meta.versionId` on a create, a `Basic.code` with other than
exactly one coding naming a system and a code, an extension with a value other
than `valueString` or with nested extensions, a modifier extension, and a
subject that is no absolute reference under the facade's base to a `Patient`,
`Practitioner`, `PractitionerRole`, `RelatedPerson` or `Person`. A body over
256 KiB is a `413`. An update or a delete without `If-Match`, with a stale
version, for an id already deleted, or an update that changes `subject` or
`code`, is a `412`; an id never issued is a `404`. A search takes `code`
once, as `system|code`, and `subject` or `subject:missing` once; another
parameter is a `400`.

Under the guard every App State request needs a token, and the facade decides
the `Basic` permission against the state itself: `c` to create, `u` to update,
`d` to delete, `s` to search. A granular scope counts when it names only
`code` with this state's `system|code`, so
`patient/Basic.cuds?code=https://myapp.example.org|encrypted-phr-access-keys`
reaches that one state code. A `patient/` scope reaches only state whose
subject is `[base]/Patient/{patient}` of the launch context, and never global
state; a `user/` or `system/` scope reaches every subject and global state. A
search with no subject answers the state the token reaches. The store and
these access rules are FerroBRIDGE's own design within the page's
§Security and Access controls.

## SMART launch Tasks

With `[facade.launch_task]` on, `Task` at the facade's base serves the two
launch profiles of SMART App Launch 2.2.0, Task profile for requesting a SMART
launch: `task-ehr-launch` asks the EHR to launch an app with an optional
`appContext`, and `task-standalone-launch` asks for a standalone launch. An
EHR viewer lists the `Task`s for the patient in view and acts on one; for an
EHR launch it registers the launch at the authorization server's launch API
with the `Task`'s `for`, `encounter` and `appContext`. The CDR holds no
`Task`, so the facade keeps them in the state store, shared by every replica.

| Interaction | Route | Answer |
|---|---|---|
| create | `POST [base]/Task` | `201` with `Location`, `ETag: W/"1"` and the `Task` under a new id |
| read | `GET [base]/Task/{id}` | the `Task` |
| search | `GET [base]/Task?patient=…&status=…&code=…` | a `searchset` of the matching `Task`s |

A `Task` conforms to one profile or is refused with `422` naming the element:
`Task.code` carries `launch-app-ehr` or `launch-app-standalone` of
`http://hl7.org/fhir/smart-app-launch/CodeSystem/smart-codes`, exactly one
input of type `smartonfhir-application` carries the app's URL as `valueUrl`,
and at most one input of type `smartonfhir-appcontext` carries a
`valueString`; other inputs stand. A body that is no R4 `Task`, or carries a
modifier extension, is a `400`, and the `id` a create carries is ignored, as
FHIR R4 `http.html` says. `patient` takes `Patient/{id}`, a bare id, or an
absolute reference, matched against `Task.for` with this server's base taken
as relative; `status` and `code` are tokens; each takes a comma-separated
list, and another parameter is a `400`. The `CapabilityStatement` lists `Task`
with both profiles. Under the guard each interaction needs its `Task`
permission, and a `patient/` scope reaches only the `Task`s whose `for` is the
patient of its launch context. The store and these rules are FerroBRIDGE's own
design.
