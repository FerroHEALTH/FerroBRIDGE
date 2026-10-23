# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1

# The English catalog of the console's own words: the reference catalog and
# the one every lookup falls back to. Each message is one `Msg` of the
# crate's `locale` module; a catalog of another language holds the same
# messages in a directory named by its BCP 47 tag beside this one. The words
# the FerroUI components draw themselves are that library's catalog.

## The sign-in

sign-in = Sign in

## The page for a session store the console cannot read

unreadable-title = Fault
unreadable-heading = The console cannot read its sessions
unreadable-fault = 500: the console cannot read its session store, so it cannot tell whether you are signed in. Ask the console's operator to restart it.

## The page for a path the console does not serve

not-found-title = Not found
not-found-back = Back to the console

business-source-license-1-1 = Business Source License 1.1
licence = Licence
made-by = Made by
version = Version
about = About
bridge = Bridge
operator = Operator
asking-bridge-file = Asking the bridge for the file
check-draft-against-running-set-save = Check a draft against the running set, save it beside the provisioned tree, and reload.
save = Save
file-provisioned-so-console-checks-draft = This file is provisioned, so the console checks a draft of it but never saves over it.
reload = Reload
compiles-files-disk-replaces-running-set = Compiles the files on disk and replaces the running set when they compile.
check = Check
the-file = The file
path-below-fhirconnect-omocl = Path, below fhirconnect/ or omocl/
create-the-file = Create the file
save-draft-reload-set-bridge-refuses = Save this draft and reload the set. The bridge refuses a draft that does not compile.
two-hundred-characters-most = Two hundred characters at most.
why-you-save-one-line = Why you save it, one line
confirm-save-before-sent = Confirm the save before it is sent.
not-saved = Not saved
file-not-utf-8-so-console = The file is not UTF-8, so the console does not open it.
files-disk-do-not-compile-so = The files on disk do not compile, so the running set is unchanged.
files-disk-compiled-replaced-running-set = The files on disk compiled and replaced the running set.
n-409-provisioned-file-holds-path-console = 409: a provisioned file holds this path, and the console never writes over one. Save the draft under a path of its own.
n-412-file-changed-since-you-opened = 412: the file changed since you opened it, so nothing was written. Open the file again to see the current version; your draft stays in the editor.
draft-does-not-compile-set-so = The draft does not compile with the set, so nothing was written and the running set is unchanged.
file-s-sha-256-now = The file's SHA-256 is now{" "}
saved-set-reloaded = Saved, and the set reloaded with it.
saved = Saved
not-stated = not stated
set-does-not-compile-draft-save = The set does not compile with this draft; a save would be refused.
set-compiles-draft = The set compiles with this draft.
mappings = Mappings
capability = Capability
dashboard = Dashboard
mapping-file = Mapping file
mapping-files = Mapping files
asking-the-bridge = Asking the bridge
discovery-document-json = The discovery document as JSON
scopes = Scopes
capabilities = Capabilities
token-endpoint = Token endpoint
authorization-endpoint = Authorization endpoint
issuer = Issuer
smart-discovery = SMART discovery
interactions = Interactions
profile = Profile
search-parameters = Search parameters
search-parameter = Search parameter
type = Type
statement-declares-no-security = The statement declares no security.
description = Description
cors = CORS
services = Services
the-capabilitystatement-as-json = The CapabilityStatement as JSON
formats = Formats
status = Status
kind = Kind
fhir-version = FHIR version
software = Software
capabilitystatement = CapabilityStatement
readiness = Readiness
indicators = Indicators
bridge-registers-no-readiness-indicator = The bridge registers no readiness indicator.
state = State
detail = Detail
not-ready = Not ready
ready = Ready
configured = Configured
listener = Listener
none-of-its-own = none of its own
lanes-and-listeners = Lanes and listeners
indicator = Indicator
lane = Lane
pins = Pins
specification-or-crate = Specification or crate
the-mappings-overview = The mappings overview
omocl-files = OMOCL files
fhirconnect-files = FHIRconnect files
templates = Templates
compiler = Compiler
built = Built
commit = Commit
product = Product
warnings = Warnings
errors = Errors
models = Models
contexts-loaded = Contexts loaded
open-editor = Open it in the editor
sha-256 = SHA-256{" "}
file-not-utf-8-so-console-2 = The file is not UTF-8, so the console does not show it: a YAML file is UTF-8 (YAML 1.2.2, character encodings).
diagnostics = Diagnostics
at = {" "}at{" "}
running-load-raised-no-diagnostic-about = The running load raised no diagnostic about this file.
no-diagnostic = No diagnostic
writable = Writable
owner = Owner
language = Language
file = File
not-read = not read
bridge-serves-no-mapping-file = The bridge serves no mapping file.
no-mapping-file = No mapping file
search = Search
models-and-extensions = Models and extensions
resource = Resource
template = Template
none = none
in = {" "}in{" "}
extended-by = extended by{" "}
source = Source
the-cdr = the CDR
template-directory = template directory
omocl = OMOCL
contexts = Contexts
the-omocl-files-are = The OMOCL files are{" "}
running-load-reads-no-template = The running load reads no template.
no-template = No template
context = Context
running-load-compiled-no-context = The running load compiled no context.
no-context = No context
no-file = no file
severity = Severity
code = Code
where = Where
message = Message
mapping = Mapping
no-diagnostics = No diagnostics.
n-401-bridge-did-not-accept-your = 401: the bridge did not accept your sign-in. Sign in again.
reason-must-one-line-most-200 = The reason must be one line of at most 200 printable ASCII characters, so the save is not sent.
resource-type-not-fhir-resource-type = the resource type is not a FHIR resource type name, so it is not asked for
mapping-file-path-not-plain-names = the mapping file path is not plain names below a root, so it is not asked for
pages = Pages
bridge-unknown = Bridge UNKNOWN
bridge-down = Bridge DOWN
bridge-refused-401 = Bridge refused 401
signed-console = Signed in to this console
console = Console
validation-dry-run = Validation dry run
playground = Playground
tools = Tools
new-file = New file
files = Files
overview = Overview
bridge-declares-validate-no-resource-type = The bridge declares $validate on no resource type.
reading-resource-types-bridge-validates = Reading the resource types the bridge validates
validate = $validate
runs-validate-which-commits-nothing-nothing = Runs $validate, which commits nothing. Nothing you enter or the bridge answers is kept.
run-validate = Run $validate
resource-json = Resource, JSON
resource-type = Resource type
runs-transform-against-bridge-s-running = Runs a transform against the bridge's running programs. Nothing you enter or the bridge answers is kept.
transform-playground = Transform playground
run-toopenehr = Run $toopenehr
flat = flat
canonical = canonical
composition-format = Composition format
template-id = Template id
bundle-json = Bundle, JSON
fhir-to-openehr-toopenehr = FHIR to openEHR: $toopenehr
run-tofhir = Run $tofhir
context-patient-reference-such-patient-123 = context.patient, a reference such as Patient/123
template-id-required-flat = Template id (required for FLAT)
composition-canonical-flat-json = Composition, canonical or FLAT JSON
openehr-to-fhir-tofhir = openEHR to FHIR: $tofhir
issues-by-severity = Issues, by severity
validate-answered = $validate answered
the-operationoutcome-as-json = The OperationOutcome as JSON
toopenehr-answered = $toopenehr answered
the-parameters-as-json = The Parameters as JSON
the-composition = The composition
composition = Composition
answer-carries-no-composition = The answer carries no composition.
tofhir-answered = $tofhir answered
the-bundle-as-json = The Bundle as JSON
entry-no-resource = an entry with no resource
resource-s-resourcetype-not-type-chosen = The resource's resourceType is not the type chosen.
resource-not-json = The resource is not JSON.
bundle-not-resource-whose-resourcetype-bundle = The Bundle is not a resource whose resourceType is Bundle.
bundle-not-json = The Bundle is not JSON.
format-not-canonical-flat = The format is not canonical or flat.
composition-empty-enter-canonical-flat-composition = The composition is empty: enter a canonical or FLAT composition.
about-body = { $product } is the operator console of FerroBRIDGE, a bridge between openEHR, HL7 FHIR and the OMOP Common Data Model.
rest-security = REST, { $mode } mode: security
rest-system = REST, { $mode } mode: system
loaded-of = { $loaded } of { $total }
warnings-count = { $warnings ->
        [one] { $warnings } warning
       *[other] { $warnings } warnings
    }
errors-warnings-count = { $errors ->
        [one] { $errors } error
       *[other] { $errors } errors
    }, { $warnings ->
        [one] { $warnings } warning
       *[other] { $warnings } warnings
    }
line-column = line { $line }, column { $column }
numbered-lines = { $path }, one numbered line per line
bridge-up = Bridge UP
bridge-up-version = Bridge UP · v{ $version }
bridge-refused-code = Bridge refused { $status }
bundle-holds = The Bundle holds { $count ->
        [one] { $count } resource.
       *[other] { $count } resources.
    }
file-editor = { $mode ->
        [new] New mapping file
       *[edit] Edit { $name }
    }
not-found-body = 404: the console has no page at this address.
bridge-refused-status = { $status }: the bridge refused the request.
view-signed-out = sign in to see this view
view-unreadable = the bridge answered { $status } with a body this console cannot read
view-unreachable = the bridge could not be reached
view-plain-post = the tool runs once its page has loaded; reload the page and run it again
view-unavailable = the console could not serve this view
view-fetch = the view could not be fetched: { $cause }
shape-missing = the bridge's answer carries no { $path } of the shape this console reads
theme-invalid = the theme is not light, dark or system
severity-error = error
severity-warning = warning
sign-in-not-configured = sign-in is not configured on this console
session-not-ended = the session could not be ended
sign-out-redirect-unwritten = the sign-out redirect could not be written
sign-in-redirect-unwritten = the sign-in redirect could not be written
no-sign-in-pending = no sign-in is pending
sign-in-state-mismatch = the sign-in state does not match this session
provider-did-not-sign-in = the OpenID Provider did not sign the operator in
redirect-carries-no-code = the redirect carries no code
id-token-did-not-verify = the OpenID Provider's ID Token did not verify
sign-in-not-completed = the sign-in could not be completed at the OpenID Provider
session-not-begun = the session could not be begun
sessions-full = the console holds as many sessions as it may; try again later
sign-in-session-not-begun = a sign-in session could not be begun
foreign-request-refused = the console takes this request only from its own pages
