<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# Searching the facade

**Audience:** integrators who search the facade, and mapping authors who
declare what it searches.

How a search declaration beside a context makes a resource type searchable, and
what a search answers.

<!-- toc -->

## Search

FHIRconnect states that it "does not focus specifically on AQL and FHIRsearch",
so no specification governs FHIR search over an openEHR CDR. The surface below
is FerroBRIDGE's own design, built on the R4 search page for everything the
client sees.

### Declaring search beside a context

A context answers search only when a search declaration names it. The
declaration is a YAML file in the mapping directory whose name ends in
`.search.yml` or `.search.yaml`; the FHIRconnect loader skips those files, and
the facade reads them at boot after the mapping set compiles.

```yaml
context: condition.problem-list        # metadata.name of the context mapping
patient:
  parameters: [patient, subject]       # the names that address the EHR's subject
  required: true                       # a search without one is a 400
order:                                 # the total order every page is read in
  - v/commit_audit/time_committed/value
  - v/uid/value
parameters:
  code:
    type: token
    node: { class: EVALUATION, archetype: openEHR-EHR-EVALUATION.problem_diagnosis.v1 }
    path: data[at0001]/items[at0002]/value      # the coded text, relative to the node
    systems:
      "http://snomed.info/sct": SNOMED-CT
  onset-date:
    type: date
    node: { class: EVALUATION, archetype: openEHR-EHR-EVALUATION.problem_diagnosis.v1 }
    path: data[at0001]/items[at0077]/value/value
  recorded:
    type: date
    path: context/start_time/value             # no node: read under the composition
  category:
    type: token
    fixed:                                     # the coding the mapping writes as a constant
      system: "http://terminology.hl7.org/CodeSystem/condition-category"
      code: problem-list-item
include:                                       # the reference parameters _include may name
  - evidence-detail
```

A parameter names the node it is read under with `node`, the RM class and
the archetype id of an archetype root, and gives its `path` relative to that
node. A parameter with no `node` is read under the composition. A `token`
names the coded value itself, a `DV_CODED_TEXT`, a `DV_TEXT` or a
`CODE_PHRASE`, and the facade reads its code and its system from that one
value, so `[system]|[code]` never pairs the system of one diagnosis with the
code of another. A `string` or a `date` names a data value, with any
attribute below it. A `date` may also name `time/value` under an `ACTION`
and `data[atNNNN]/origin/value` under an `OBSERVATION`: the RM defines both
instants, each once, and the Web Template leaves them out.

A value under a node that repeats inside its archetype, such as the data of
an `OBSERVATION`'s events, binds that node with `contains`, its RM class and
at-code, and gives the path relative to it:

```yaml
    node:
      class: OBSERVATION
      archetype: openEHR-EHR-OBSERVATION.laboratory_test_result.v1
      contains: { class: EVENT, node: at0002 }
    path: data[at0003]/items[at0005,'Labortest-Bezeichnung']/value
```

The query reads `CONTAINS OBSERVATION[<archetype>] CONTAINS EVENT n0[at0002]`
(QUERY AQL 1.1.0 §3.13.2.2), so the entry matches when any of its events
does, and the bound node is never one inside an archetype slotted below the
root. A path names the Web Template's name predicate where the template
gives the node a name.

A `token` whose value the mapping writes as a constant, such as the category
of every observation a context maps, has no openEHR path to compare. Declare
it with `fixed`, the one coding every resource of the context carries, and no
`node` or `path`: a value that matches the coding adds no condition, and any
other value answers an empty `searchset` with no query to the CDR.

`include` lists the codes of the type's standard R4 reference parameters a
search may name in `_include`. The facade stores nothing, so it includes the
resources the match's own mapping creates beside it and the match references
through that parameter's R4 expression, each once with
`search.mode = include`. The `CapabilityStatement` names each as
`searchInclude`.

The facade writes the query itself:
`SELECT e/ehr_id/value, v/uid/value, c FROM EHR e CONTAINS VERSION v[LATEST_VERSION] CONTAINS COMPOSITION c`,
restricted to the context's template through
`c/archetype_details/template_id/value`. Each occurrence of a parameter read
under a node binds that node in the `FROM`, as
`CONTAINS EVALUATION n0[openEHR-EHR-EVALUATION.problem_diagnosis.v1]`, so
the conditions of one occurrence read one node, while a repeated parameter
(an AND) may match a node of its own. Every search value travels as a bound
AQL parameter and never as query text.

The declaration is checked when the server starts, against the template the
context maps, and any of these refuses the start with the file named: an
unknown key, a context no loaded mapping compiles, a parameter type other
than `token`, `string` or `date`, a name under the `_` prefix R4 reserves, a
node the template does not hold, a path the template does not hold, a path
that crosses a node which may occur more than once below the bound one (bind
that node instead), a token over a value that is not coded, a path that is not
one AQL path, a query that does not parse as AQL, two declarations of one
resource type over one template whose contexts do not each start at an
archetype of their own, two declarations of one type that state
different parameters or includes, a `fixed` coding beside a `node` or a
`path` or on a type other than `token`, an `include` code that is no standard
R4 reference parameter of the type, and a parameter that takes the code of a standard R4
search parameter of the resource type with another type (`code` on a
`Condition` is a `token`; `patient` and `subject` are references). R4 says
"standard parameters cannot be redefined". A parameter that takes a standard
code names that parameter's canonical as `definition` in the
`CapabilityStatement`, and a code R4 does not define for the type carries
none. `order` defaults to the commit time and then the version
uid; set it when your CDR orders better by something else, and end it with a
key that is unique per composition.

### What a search answers

| Parameter | What it does |
|---|---|
| a declared `token` | `[code]`, `[system]\|[code]`, `\|[code]` and `[system]\|`; `systems` translates a FHIR system URI into the openEHR terminology id, and an unlisted system compares as written |
| a declared `string` | starts-with by default, `:contains` and `:exact` |
| a declared `date` | the range of the value's precision, with the prefixes `eq`, `ne`, `gt`, `lt`, `ge`, `le`, `sa` and `eb` |
| `patient`, `subject` (as declared) | `[id]`, `Patient/[id]`, an absolute URL on this server, or the `:Patient` modifier, resolved through the identity map to the EHR |
| `patient:identifier`, `subject:identifier` | `[system]\|[value]`, the identifier a resource's subject reference carries: the identity map's subject keyed by that system and value, which the facade renders with that `identifier`; a subject rendered as a literal reference carries none and matches nothing, `\|[value]` matches nothing, and `[value]` alone or `[system]\|` is a `400` |
| `patient.identifier`, `subject.identifier`, `subject:Patient.identifier` | with [`[facade.identifier_search]`](../operate/configuration.md#facadeidentifier_search) on: `[value]`, or `[system]\|[value]` where a system query is configured; the identifier is resolved to every demographic party that carries it and each party's EHR, so a bare value two patients carry matches the resources of both (R4 search §3.1.1.4.10) |
| `_id` | a logical id, resolved through the identity map to its composition |
| `_count` | the page size, capped at `[facade] search_count_max`; `0` answers the total alone |
| `_sort` | declared parameters read under the composition, `-` for descending, ahead of the declared order; a parameter read under a node is refused, because it has a value per node |
| `_summary` | `count` answers the total alone; `false` is the default |
| `_total` | `none`, `estimate` or `accurate` |
| `_include` | `[type]:[parameter]` or `[type]:[parameter]:[target type]` for a declared include; `:iterate` and any other include are a `400`, or ignored under `Prefer: handling=lenient` |

A comma inside a value is an OR and a repeated parameter an AND. An empty
parameter is ignored. A subject or an id resolves to its person through the
identity map and, on a miss, the party read and ITI-83. One that names no
person matches nothing, so the answer is an empty `searchset`, never a `404`,
and no query reaches the CDR. A subject is read from the map and never echoed
from the request.

A parameter the type does not declare is a `400` with `not-supported` naming
it. Send `Prefer: handling=lenient` and the facade ignores it instead, leaves it
out of the `self` link and reports it in an `OperationOutcome` entry with
`search.mode = outcome`. A modifier the facade does not support is a `400` in
both modes, because R4 makes that a SHALL. So are a malformed value, the `ap`
date prefix, and a string pattern carrying `*` or `?`, which AQL `LIKE` reads
as wildcards with no escape (search it with `:exact`). The one supported
chain is `identifier` on a subject parameter: any other chain is a parameter
this server does not support. A modifier on the chain is a `400`, and a
`subject:Group.identifier` names a target that is never an EHR subject, so it
matches nothing. The `CapabilityStatement` states the chain in the
`documentation` of each subject parameter, and only while it is on.

### The searchset

The answer is a `searchset` Bundle with an `id` and `meta.lastUpdated`. Each
entry is one composition, with a unique `fullUrl` under the configured base and
`search.mode = match`, mapped back through the context's program as a read
maps it. `link[self]` names the parameters the search used with the page size,
and `link[next]` appears when another page follows. Its `_cursor` is opaque:
follow the link as answered, because a cursor pasted onto another search is a
`400`.

Each entry of the context's archetype in a composition is a resource of its
own. A query that binds a node answers a row per matching node, and the
search answers the entries that hold the matched nodes; a search that binds
no node, such as `patient` alone, answers every entry of each matching
composition. The rows of one composition sit together under the order, and
an entry the page already holds is passed over. Each page reads one row past
its room through the CDR's `offset` and `fetch`, and a `next` link is written
only when another entry is left, its cursor at the first row of that entry's
composition with the number of its entries already answered. `Bundle.total`
counts matching entries, never the page and never the nodes: a first page
that holds every match carries it at no cost, and `_total=accurate` on a
paged search costs one AQL count of the entries, which reads no composition.
The count binds the entry as `CONTAINS <class> x[<archetype>]`, and AQL
`CONTAINS` also finds an entry nested in a `SECTION`, so a search that binds
a node or an `_id`, or a template that nests the entry, reads the rows and
counts each entry once instead. `_total=estimate` always answers from one
count query, and `_total=none` leaves the total out. `_count=0` and `_summary=count` answer the total
alone, with no entries and no `next` link.

A search never answers an empty Bundle for a failed call. A CDR refusal reaches
the wire through the status table above: a `5xx` or a `408` on the query is a
`502` carrying the upstream status, and a `400` on it is a `500`, because the
bridge wrote the AQL. A row the facade cannot read is a `502` naming the column,
never the value.
