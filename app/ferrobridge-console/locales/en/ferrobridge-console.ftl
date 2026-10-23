# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1

# The FerroBRIDGE Console's own words, in English: the reference catalog
# every lookup falls back to.


## Numbers

number-group = ,
number-decimal = .

## The shell

console = Console
operator = Operator
operator-line = Bridge operator
bridge-up = Bridge up
bridge-version = version { $version }
bridge-refused401 = Bridge refused 401
bridge-unknown = Bridge unknown
bridge-unreachable = Bridge unreachable
console-navigation = Console
overview = Overview
dashboard = Dashboard
mappings = Mappings
mapping-set = Mapping set
playground = Playground
bridge = Bridge
fhir-facade = FHIR facade
hl7v2 = HL7 v2
omop-etl = OMOP ETL
connections = Connections
console-group = Console
about = About
nav-errors = { $count ->
        [one] 1 error in the running load
       *[other] { $count } errors in the running load
    }
nav-unknown-down = Not known: the bridge did not answer
nav-unknown-off = Not known: the operator API is off on this bridge
search-label = Search contexts, files and templates
search-placeholder = Context, file or template
search-found = { $count ->
        [one] 1 match for “{ $words }”
       *[other] { $count } matches for “{ $words }”
    }
search-nothing = Nothing found for “{ $words }”
search-scope = The search finds contexts, mapping files and templates, and never takes a patient identifier.
search-every-file = Every mapping file
search-files = Mapping files
search-file-line = Mapping file
search-template-line = Operational template

## Words several pages share

try-again = Try again
cancel = Cancel
close = Close
edit = Edit
check-draft = Check a draft
check = Check
new-file = New file
write-first-file = Write the first file
every-file = Every mapping file
clear-filters = Clear filters
documentation = Documentation
licence = Business Source License 1.1
count-errors = { $count ->
        [one] 1 error
       *[other] { $count } errors
    }
count-warnings = { $count ->
        [one] 1 warning
       *[other] { $count } warnings
    }
noun-errors = { $count ->
        [one] error
       *[other] errors
    }
noun-warnings = { $count ->
        [one] warning
       *[other] warnings
    }
noun-errors-in-set = { $count ->
        [one] error in the set
       *[other] errors in the set
    }
noun-warnings-in-set = { $count ->
        [one] warning in the set
       *[other] warnings in the set
    }
actions-for = Actions for { $name }
state = State
state-on = On
state-off = Off
state-up = Up
state-unreachable = Unreachable
state-not-checked = Not checked
state-configured = Configured
state-not-configured = Not configured
state-compiles = Compiles
state-no-diagnostic = No diagnostic
state-not-utf8 = Not UTF-8
state-from-readiness = From the bridge's readiness indicators

## Faults and refusals

view-signed-out = Sign in to see this page.
view-unreadable = The bridge answered with a body this console cannot read.
view-unreachable = The bridge could not be reached, so this page cannot be shown.
view-unavailable = The console could not serve this page.
bridge-refused-status = The bridge refused the request ({ $status }).
bridge-refused-sign-in = The bridge did not accept your sign-in (401). Sign in again.
invalid-path = The mapping file path is not plain names.
invalid-resource-type = The resource type is not a FHIR resource type.
invalid-reason = The reason must be one line of at most 200 characters.
operator-off = The operator API is off on this bridge, so the mapping set is not shown; the Playground, the FHIR facade and the connections still work.
operator-off-hint = the bridge answers 404 for its operator API.
preference-unknown = The preference or its value is not one the console offers.
not-found-title = Page not found
not-found-back = Back to the console
not-found-body = No page of the console is at this address. The address may be mistyped, or the file or template it names is gone.
back-to-dashboard = Back to the dashboard
unreadable-title = The console cannot read its sessions
unreadable-body = The console cannot tell whether you are signed in, so it shows nothing. Whoever runs the console can restart it.

## The dashboard

dashboard-subtitle = The bridge at { $bridge }.
stat-contexts = Contexts loaded
stat-contexts-value = { $programs } of { $contexts }
stat-omocl = { $count } OMOCL
stat-running-load = Running load
stat-clean = Clean
stat-upstreams = Upstreams reachable
stat-upstreams-value = { $up } of { $total }
stat-all-up = All up
stat-unreachable = { $count } unreachable
stat-operator-off = operator API off
stat-not-known = not known
attention = Needs attention
attention-caption = What needs attention
attention-what = What
attention-kind = Kind
attention-state = State
attention-why = Why
kind-connection = Connection
kind-probe = Probe
upstream-cdr = openEHR CDR
upstream-terminology = Terminology server
upstream-cdm = OMOP CDM database
indicator-hl7v2 = HL7 v2 listener
indicator-identity-store = Identity store
flow = Flow
flow-label = The bridge as a flow: callers, faces, the mapping set and the upstreams
flow-list-label = The bridge as a flow, lane by lane; each card opens its page
flow-callers = Callers
flow-faces = Faces
flow-mapping-set = Mapping set
flow-upstreams = Upstreams
flow-clients = FHIR clients
flow-clients-line = SMART App Launch
flow-senders = HL7 v2 senders
flow-senders-line = over MLLP
flow-run = ETL run
flow-run-line = by your scheduler
flow-operations = Operations
flow-facade-on = On · /fhir
flow-operations-on = On · $tofhir, $toopenehr
flow-hl7-on = On
flow-etl-on = Batch · its own process
flow-listening = On · { $listen }
flow-listener-down = Listener down
flow-off = Off
flow-upstream-up = Up
flow-no-answer = No answer
flow-no-answer-label = no answer
flow-not-configured = Not configured
flow-hub-line = { $contexts ->
        [one] { $contexts } context
       *[other] { $contexts } contexts
    } · { $omocl } OMOCL
flow-hub-unknown = – · needs the operator API
flow-foot = { $faces ->
        [one] { $faces } face
       *[other] { $faces } faces
    } on · { $up } of { $total ->
        [one] { $total } upstream
       *[other] { $total } upstreams
    } up.

## The mapping set

mapping-set-subtitle = What the bridge runs, and the files it runs from.
set-more-label = More actions for the mapping set
reload-set = Reload the set
reload-set-note = Compiles every file on disk, and swaps only when it compiles
reload-dialog-title = Reload the set?
reload-dialog-body = The bridge compiles every file on disk and swaps in the new set only when it compiles. A request in flight finishes on the old set; an error changes nothing.
running-compiles = The running set compiles
running-compiles-warnings = The running set compiles, with warnings
running-not-compiles = The running set does not compile
reloaded = Reloaded: the set compiled and replaced the running one
reload-refused = Not reloaded: the files on disk do not compile, so the running set is unchanged
reload-diagnostics = Diagnostics of the reload
running-contexts = Running contexts
set-empty = No mapping file yet, so the facade serves no resource type.
tab-contexts = Contexts
tab-omocl = OMOCL
tab-templates = Templates
tab-files = Files
tab-diagnostics = Diagnostics
no-contexts = The running set has no context yet.
no-omocl = The running set has no OMOCL file.
no-templates = The running set compiled against no template.
no-diagnostics = No errors or warnings in the running load.
no-file-matches = No mapping file matches these filters.
open-program = Open its program
try-in-playground = Try in the Playground
col-context = Context
col-resource-type = Resource type
col-template = Template
col-models = Models
col-model = Model
col-search = Search
col-owner = Owner
col-state = State
col-actions = Actions
col-file = File
col-source = Source
col-contexts = Contexts
col-path = Path
col-kind = Kind
col-diagnostics = Diagnostics
col-language = Language
models-count = { $count ->
        [one] 1 model
       *[other] { $count } models
    }
search-declared = declared
source-directory = the template directory
source-cdr = the CDR
owner-provisioned = provisioned
owner-provisioned-title = Read from the provisioned mapping directory; the console checks a draft of it but never saves over it
owner-console = console
owner-console-title = Saved by the console under [operator] mapping_dir
filter-any-language = Any language
filter-any-owner = Any owner
find-files = Path or file name
chip-language = Language: { $value }
chip-owner = Owner: { $value }
chip-path = Path: { $value }

## One file

file-sections = { $name } sections
tab-source = Source
tab-program = Program
saved-by-console = saved by the console
provisioned = provisioned
context-subtitle = Context of { $resource } on { $template }, { $owned }.
model-subtitle = Model, { $owned }.
omocl-subtitle = OMOCL mapping, read by the next ETL run, { $owned }.
file-subtitle = Mapping file, { $owned }.
provisioned-notice = Provisioned, so read-only here: check a draft against the set, or save it under a path of its own.
file-not-utf8 = The console does not show this file: the bridge reads mapping files as UTF-8, and this one is not. Convert it where it is provisioned, then reload the set.
no-file-diagnostics = No diagnostics: the running load raised none about this file.
short-digest = SHA-256 { $digest }
digest-title = SHA-256 of the bytes the bridge serves
no-program = The running set compiled no program from this context.
fact-profile = Profile
compiled-program = The compiled program
no-search = No search file names this context, so it adds no search parameter to { $resource }.
where-it-runs = Where it runs
used-by = Used by
used-by-none = No context reads with this model.

## One template

template-from-directory = An operational template, read from the template directory.
template-from-cdr = An operational template, fetched from the CDR.
template-contexts = Contexts that name it
template-no-contexts = No context names this template.

## The editor

edit-title = Edit · { $name }
editor-title = { $mode ->
        [new] New mapping file
       *[edit] Edit
    }
draft-title = { $mode ->
        [new] New mapping file
       *[edit] Check a draft
    }
new-mapping-file = New mapping file
new-file-subtitle = Written beside the provisioned files once the set compiles with it.
edit-subtitle = Your draft, opened at SHA-256 { $digest }.
edit-saved-subtitle = The file as saved, SHA-256 { $digest }.
provisioned-editor-notice = This file is provisioned, so the console never saves over it.
the-file = The file
where-to-save = Where to save it
path = Path
new-path = New path
new-path-help = Below fhirconnect/ or omocl/, ending in .yml.
save-as-help = A path of its own, below fhirconnect/, ending in .yml.
draft-of = The draft of { $name }
lead-fhirconnect = YAML · FHIRconnect
lead-omocl = YAML · OMOCL
editor-facts = UTF-8 · LF
not-checked-yet = Not checked yet: Check compiles the whole set with this draft, and writes nothing.
draft-no-diagnostics = No diagnostics for this draft.
check-clean = The set compiles with this draft: { $warnings ->
        [one] 1 warning
       *[other] { $warnings } warnings
    }.
check-errors = The set does not compile with this draft: { $errors ->
        [one] 1 error
       *[other] { $errors } errors
    }. Fix them to save.
save-blocked = The set does not compile with this draft
confirm-save = Save and reload the set? The bridge compiles the set with the draft again and writes the file only when it compiles; a refused draft changes nothing.
saved-report = Saved, and the set reloaded with it. SHA-256 now { $digest }
saved-line = Saved and reloaded.
open-file = Open the file
save-refused-compile = Not saved: the set does not compile with your draft, so nothing was written and the running set is unchanged
save-conflict = The file changed since you opened it, so nothing was written. Open the current version and apply your change to it.
save-provisioned-path = Not saved: a provisioned file holds this path.
choose-other-path = Choose a path no provisioned file holds.
save-and-reload = Save and reload
create-and-reload = Create and reload
save-under-new-path = Save under the new path
discard = Discard
go-back = Go back
confirm-the-save = Confirm the save
save-bar = Save
reason = Reason
reason-placeholder = Why you save it
reason-help = One line, at most 200 characters
reason-missing = Give a reason to save.
reason-too-long = Keep the reason to 200 characters at most.
reason-characters = Use plain letters, digits and punctuation.
path-missing = Give the file a path.
path-steps = Remove the empty, . or .. step: a file stays inside the console's mapping directory.
path-root = Put the file below fhirconnect/ or omocl/.
path-extension = End the path in .yml.

## The playground

playground-subtitle = Runs the bridge's operations on what you enter, and keeps nothing.
tab-to-fhir = openEHR → FHIR
tab-to-openehr = FHIR → openEHR
tab-validate = Validate a resource
runs-operation = Runs { $operation }
convert-to-fhir = Convert to FHIR
convert-to-openehr = Convert to openEHR
converting = Converting
validate-run = Validate
validating = Validating
template = Template
template-from-composition = From the composition
template-chosen-by-mapping = Chosen by the mapping
patient = Patient
resource-type = Resource type
answer-format = Composition format (answer)
canonical = Canonical
flat = FLAT
composition = Composition
bundle = Bundle
resource = Resource
outcome = Outcome
composition-label = Composition, canonical or FLAT JSON
bundle-label = Bundle, JSON
resource-label = Resource, JSON
convert-to-see-bundle = Convert to see the Bundle.
convert-to-see-composition = Convert to see the composition.
validate-to-see = Validate to see the outcome.
answered = Answered
refused = Refused
not-run = Not run
operations-off = Conversions are off on this bridge.
validate-none = The bridge declares $validate on no resource type.
composition-empty = The composition is empty: enter a canonical or FLAT composition.
format-not-known = The format is not canonical or FLAT.
bundle-not-json = The Bundle is not JSON.
bundle-not-bundle = The JSON is not a resource whose resourceType is Bundle.
resource-not-json = The resource is not JSON.
resource-type-mismatch = The resource's resourceType is not the type chosen.

## The FHIR facade

tab-overview = Overview
tab-capability = Capability
tab-security = Security
facade-on-subtitle = Serves FHIR R4 over the CDR at /fhir.
facade-off-subtitle = Off, so the bridge serves no FHIR route.
facade-off = FHIR clients reach nothing; the mapping set still compiles, and the Playground runs while conversions are on.
fact-base = Base
fact-resource-types = Resource types
fact-fhir = FHIR
fact-lane = Lane
fact-issuer = Issuer
fact-capabilities = Capabilities
what-it-serves = What it serves
serves-validation = Validation
serves-validation-value = $validate on { $count } resource types
serves-search = Search
serves-search-value = { $count } resource types answer a search
operations-value = $tofhir and $toopenehr; neither commits
try-them = Try them
in-playground = in the Playground
identity = Identity
capability-intro = From the CapabilityStatement at /fhir/metadata. A resource type opens the program that serves it.
no-resource-types = The CapabilityStatement declares no resource type.
col-profiles = Profiles
col-interactions = Interactions
col-search-params = Search parameters
col-operations = Operations
what-facade-declares = What the facade declares
smart-discovery = SMART discovery
no-smart = The facade checks no tokens, so it answers no SMART discovery document.

## HL7 v2

hl7v2-on-subtitle = Takes HL7 v2 messages over MLLP and commits each as one transaction.
hl7v2-off-subtitle = Off, so no MLLP listener runs.
hl7v2-off = A sender's connection is refused.
listener = Listener
fact-address = Address
fact-listener = Listener
mapping = Mapping
fact-guide = Guide
fact-acknowledgment = Acknowledgment
acknowledgment-value = original mode, from the commit's outcome
fact-commit = Commit
commit-value = each message as one transaction

## OMOP ETL

omop-subtitle = A batch that maps compositions from the CDR into the OMOP CDM database.
omop-off = No ETL is configured on this bridge.
what-arun-does = What a run does
run-reads = Reads
run-reads-value = compositions from the openEHR CDR, over AQL
run-maps = Maps
run-maps-value = with the OMOCL files
run-writes = Writes
run-database = Database
runs = Runs
how-it-runs = How it runs
run-reports = Run reports
run-reports-value = printed by each run

## Connections

connections-subtitle = The upstreams the bridge reaches.
col-connection = Connection
col-used-by = Used by
cdr-subtitle = The CDR the bridge reads and writes.
terminology-subtitle = The FHIR terminology server the bridge looks codes up in.
cdm-subtitle = The PostgreSQL database the ETL writes.
readiness = Readiness
uses-it = uses it
used-by-no-lane = No lane that is on uses it.
upstream-not-configured = The bridge is not configured to reach it.
no-answer-detail = It does not answer.

## About

about-subtitle = The console, the bridge it reaches, and who makes them.
about-bridge = Bridge
about-build = Build
about-console-settings = This console
about-bridge-address = Bridge address
about-locale = Default language
about-info-title = The bridge's info
about-info-line = GET /health/info, as the bridge answers it.

## Your preferences

your-preferences = Your preferences
accent-indigo = Indigo

## Sign-in

sign-in = Sign in
sign-in-title = Sign in to the console
sign-in-with-provider = Sign in with your identity provider
sign-in-redirecting = Taking you to your identity provider…
sign-in-note = You sign in at your organisation's identity provider; the console never sees your password.
sign-in-caption = ferrobridge-console, for the operators of FerroBRIDGE. It holds no clinical data.
signed-out = You are signed out of the console.
sign-in-expired = Your session ended. Sign in again to go on.
sign-in-unavailable = Sign-in is not set up yet. Whoever runs this console adds an identity provider to its configuration file.
sign-in-unavailable-detail = No [oidc] section in the console's configuration (503).
sign-in-rejected = Not signed in: your identity provider did not sign you in, so nothing was kept. If it keeps happening, ask your provider's administrator.
sign-in-refused = Not signed in: the sign-in could not be completed at your identity provider. Try again; if it keeps failing, the provider may be down.
sign-in-full = The console has reached its session limit, so it cannot start yours. Try again in a few minutes.

## Plain answers of the sign-in routes

sign-in-not-configured = sign-in is not configured on this console
session-not-ended = the session could not be ended
sign-out-redirect-unwritten = the sign-out redirect could not be written
sign-in-redirect-unwritten = the sign-in redirect could not be written
no-sign-in-pending = no sign-in is pending
sign-in-state-mismatch = the sign-in state does not match this session
provider-did-not-sign-in = For the administrator: the identity provider refused the sign-in (401).
redirect-carries-no-code = the redirect carries no code
id-token-did-not-verify = For the administrator: the ID token the provider sent did not pass the console's checks (401).
sign-in-not-completed = The console could not exchange the code for tokens (502).
session-not-begun = the session could not be begun
sessions-full = Sessions end after a time without a request (503).
sign-in-session-not-begun = a sign-in session could not be begun
foreign-request-refused = the console takes this request only from its own pages
