<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# Writing and loading mappings

Mappings are the product. FerroBRIDGE reads specification-conformant YAML and
never carries a hand-coded converter for one FHIR resource or one OMOP table. A
per-resource converter is refused in review, because it defeats the reason the
project exists. `ferrobridge serve` loads the mapping set at start and refuses
a file it cannot compile there, `ferrobridge mapping check` checks a tree
without serving it, and the [operator API](../operate/operator-api.md) lists,
checks, saves and reloads the files of a running bridge.

<!-- toc -->

## What a mapping set looks like

On the FHIR side you supply three kinds of file, and they compose in this
order:

1. A `model` file per archetype, mapping it to an unprofiled R4 resource.
2. An `extension` file where a profile or a template needs an adaptation.
3. A `context` file as the entry point, naming the profile, the template, the
   archetypes, the extensions, and the `start` mapping.

Beside a context you may add a search declaration, a file whose name ends in
`.search.yml`, stating which FHIR search parameters the facade answers for it
and the AQL path each one compares. It is FerroBRIDGE's own format, because
FHIRconnect leaves search out, and the FHIRconnect loader skips it.
[The FHIR facade](fhir-facade.md#search) describes it.

On the OMOP side you supply OMOCL files: a flat list of records keyed by CDM
target, each record's keys drawn from OMOCL's own column vocabulary (`value`,
`unit`, `concept_id`, `measurement_date`) with ordered `alternatives`.
FerroBRIDGE carries the table that projects each key onto CDM columns, because
OMOCL does not publish one. YAML anchors and aliases are supported, because the
published library uses them.

## Validation happens at load, not at runtime

Every FHIRconnect file is checked three ways. The two published draft-07 JSON
schemas are vendored and run, and the files they reject are recorded, because
those schemas refuse three mapping types the specification defines. Then
FerroBRIDGE's own strict schema runs, which admits exactly what the prose
defines. Then the constraints no schema can express are checked: `targetRoot`
alignment, `appendTo`, `slotArchetype` and `slotContext` resolution, the
sub-context rules (a `scope: sub` context is listed under some context's
`contexts`, the `contexts` are acyclic, and a `slotContext` never writes
`openehr: $reference`), unique mapping names,
the presence of `criteria` where the grammar requires it, and every path
resolving against the template and the FHIR element table. Every OMOCL file is
validated against the schema FerroBRIDGE authors from the grammar tables.

Version agreement is part of loading. The mapping version, the grammar version,
the archetype revision, the template `sem_ver`, and the profile version have to
agree, and a mismatch is refused before any request is served.

## Checking a set before you deploy it

`ferrobridge mapping check` runs the same load without starting a lane, and
reports every diagnostic instead of stopping at the first. It reads the
`[mappings]` section of the configuration: the FHIRconnect tree in
`directory`, compiled context by context against the templates in
`templates` or, without that key, the templates the `[cdr]` serves, and the
OMOCL files in `omocl`, which it loads and validates. OMOCL names no
template, so its files compile when `etl run` first meets a composition, and
the check does not compile them.

```sh
ferrobridge mapping check --config /etc/ferrobridge/ferrobridge.toml
ferrobridge mapping check --format json
```

The text form prints one line per diagnostic,
`file:line:column: severity[code]: message`, then a summary line with the
files read, the contexts compiled and the error and warning counts. The JSON
form prints one document with `clean`, `counts` and a `diagnostics` array
whose objects carry `file`, `line`, `column`, `severity`, `code`, `message`,
`mapping` and `model_path` (the steps into the document model, as an array), the serde form of `openehr-mapping-core`'s `Diagnostic`. The report is the whole of stdout; the check
writes no log line.

| Exit code | Meaning |
|---|---|
| 0 | No diagnostic is an error. Warnings are printed and accepted, as `serve` accepts them. |
| 1 | At least one diagnostic is an error. |
| 69 | The check could not run: a directory cannot be read, or the CDR answered a template fetch with neither the template nor a `404`. |
| 78 | The configuration names nothing to check, or a FHIRconnect tree with no template source. |

Besides the codes of the two mapping languages, the check raises
`template-build` for an operational template that does not build,
`fb-unknown-template` for a context whose template neither the template
directory nor the CDR provides, and `fb-no-template-id` for a context that
names no template. An operator API check or save also raises
`fb-search-declaration` for a search declaration that does not bind to the
set.

## Paths need a template

Paths in both languages are RM and archetype paths, such as
`$archetype/data[at0001]/items[at0077]`. A path alone is not executable,
because the leaf RM type and the template-specific identifiers live in the Web
Template built from the template's operational template. FerroBRIDGE resolves
in that order: the operational template, then the Web Template, then the path,
then the composition. Supply the template to the CDR, and FerroBRIDGE reads it
from there.

## Composition fields FHIR does not carry

A composition needs a composer, a `context/start_time`, a `setting`, a
`language`, a `territory`, and a `category`. FHIR carries no counterpart for
most of those, so FHIRconnect defaults them and requires the `start` mapping to
slot in a reusable `COMPOSITION.<archetype>.<Resource>` mapping. FerroBRIDGE
follows those defaults and records every defaulted value in the composition's
`FEEDER_AUDIT`, so a later reader can tell which values came from the source
and which came from a default.

## Code translation

FHIRconnect's `conceptmap` and OMOCL's `conceptMap` both translate codes, and
they resolve differently. A FHIRconnect `conceptmap` reference goes to the
configured FHIR terminology server through `$translate`. An OMOCL `conceptMap`
is an inline table in the mapping file, and everything else on the OMOP side
resolves through the loaded OHDSI vocabulary in SQL.

## Escaping to code

`mappingCode` in FHIRconnect and `CustomMapping` in OMOCL name a function that
the mapping cannot express. FerroBRIDGE resolves those names against Rust
functions registered at build time. There is no runtime plugin loading, so a
mapping file cannot introduce new executable behaviour into a deployment.

## Where the mapping libraries come from

The published FHIRconnect and OMOCL libraries are vendored verbatim under
`docs/specs/fhirconnect-mapping-lib/` and `docs/specs/omocl/` by committed
fetch scripts, with provenance recorded beside them. Both run as conformance
corpora: every file loads and validates, or carries a recorded skip with its
reason, and each corpus keeps a pass list with a badge in the README. The
corpus is how FerroBRIDGE finds a grammar feature it has not implemented.
