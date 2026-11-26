<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# Your first mapping

**Audience:** integrators and mapping authors meeting FerroBRIDGE for the
first time.

In this tutorial you write a FHIRconnect mapping between a FHIR `Condition`
and an openEHR problem diagnosis, check it, serve it, and send a `Condition`
to openEHR and back through the two FHIRconnect operations. You need the
`ferrobridge` binary and `curl` or any HTTP client. You need no CDR, no
terminology server and no database: the operations transform a payload and
store nothing.

A test runs every file and request on this page against the server in CI,
so what you read here is what the server answers.

<!-- toc -->

## 1. Get the template

A FHIRconnect path names openEHR nodes by archetype path, and FerroBRIDGE
resolves each path against the operational template the context names. This
tutorial uses a small synthetic template, `ferrobridge.diagnose.v1`, with one
problem diagnosis archetype. Make a working directory and download the
template into it:

```sh
mkdir -p first-mapping/mappings first-mapping/templates
cd first-mapping
curl -fsSL -o templates/diagnose.opt \
  https://raw.githubusercontent.com/FerroHEALTH/FerroBRIDGE/main/tools/ferrobridge-testkit/fixtures/opt/diagnose.opt
```

## 2. Write the model mapping

A model mapping relates one archetype to an unprofiled FHIR resource
(FHIRconnect v1.0.0, Types of mapping files). This one maps the problem name
to `Condition.code`, as a coded value in both directions:

<!-- book-file: mappings/diagnose.yml -->
```yaml
grammar: FHIRConnect/v1.0.0
type: model
metadata:
  name: tutorial_diagnose
  version: 1.0.0
spec:
  system: FHIR
  version: R4
  openEhrConfig:
    archetype: openEHR-EHR-EVALUATION.problem_diagnosis.v1
  fhirConfig:
    structureDefinition: http://hl7.org/fhir/StructureDefinition/Condition
mappings:
  - name: "problemDiagnose"
    with:
      fhir: "$resource.code"
      openehr: "$archetype/data[at0001]/items[at0002]"
      type: "CODEABLECONCEPT"
  - name: "subject"
    unidirectional: "openehr->fhir"
    with:
      fhir: "$resource.subject"
    manual:
      - name: "subjectReference"
        fhir:
          - path: "reference"
            value: "Patient/synthetic-subject-0001"
```

In `problemDiagnose`, `with.fhir` is the FHIR path, `with.openehr` the
openEHR path, and `type` the data type both sides carry. An openEHR
composition names no patient: its EHR does. FHIR R4 requires
`Condition.subject`, so `subject` runs only from openEHR to FHIR
(`unidirectional`) and writes a fixed reference with `manual`. In a
deployment the FHIR facade fills the subject from the EHR's patient instead
([The FHIR facade](../integrate/fhir-facade.md)); this tutorial has no CDR,
so the mapping names one.

## 3. Write the context mapping

A context mapping names the profile a resource claims, the template it maps
onto, the model mappings it imports and the one it starts from:

<!-- book-file: mappings/diagnose.context.yml -->
```yaml
grammar: FHIRConnect/v1.0.0
type: context
metadata:
  name: tutorial_diagnose.context
  version: 1.0.0
spec:
  system: FHIR
  version: R4
context:
  profile:
    url: "http://example.org/fhir/StructureDefinition/tutorial-diagnose"
  template:
    id: "ferrobridge.diagnose.v1"
  archetypes:
    - "tutorial_diagnose"
  start: "tutorial_diagnose"
```

FerroBRIDGE picks this context for any resource whose `meta.profile` names
`http://example.org/fhir/StructureDefinition/tutorial-diagnose`.

## 4. Configure the bridge

Write `ferrobridge.toml` beside the two directories:

<!-- book-file: ferrobridge.toml -->
```toml
[mappings]
directory = "mappings"
templates = "templates"

[operations]
composition_language = "en"
composition_territory = "NL"

[auth]
enabled = false

[usage_report]
enabled = false
```

`[mappings] directory` names the mapping tree and `[mappings] templates` the
operational templates, so the bridge needs no CDR to compile. A composition
needs a language and a territory, which a FHIR resource does not carry, so
`[operations]` names the ones every composition it builds takes. `[auth]
enabled = false` serves the operations without a token: do this only on your
own machine. In a deployment, `[auth]` names the authorization server whose
tokens the bridge accepts ([Configuring the
server](../operate/configuration.md#auth)).

## 5. Check the mapping

```sh
ferrobridge --config ferrobridge.toml mapping check
```

The check compiles every context against its template and prints one line
per diagnostic, then a summary. It exits 0 when no diagnostic is an error.
Break a path, for example `items[at0002]` to `items[at9999]`, and run it
again: the check names the file, the line and the path the template does not
hold.

## 6. Serve it

```sh
ferrobridge --config ferrobridge.toml serve
```

The server listens on `127.0.0.1:8080`. Ask whether it is ready; it answers
`200` with `"state": "up"` once it serves:

<!-- book-request: ready expect=200 holds=/state=up holds=/phase=serving -->
```http
GET /health/readiness
```

## 7. Send a Condition to openEHR

`$toopenehr` takes a FHIR `Bundle` and answers a `Parameters` carrying the
composition as a JSON string. The `Condition` claims the context's profile:

<!-- book-request: to-openehr expect=200 holds=/resourceType=Parameters holds=/parameter/0/name=composition -->
```http
POST /fhir/$toopenehr
Content-Type: application/fhir+json

{
  "resourceType": "Bundle",
  "type": "collection",
  "entry": [
    {
      "resource": {
        "resourceType": "Condition",
        "meta": {
          "profile": ["http://example.org/fhir/StructureDefinition/tutorial-diagnose"]
        },
        "code": {
          "coding": [
            {
              "system": "http://example.org/fhir/CodeSystem/synthetic-problems",
              "code": "SYN-001",
              "display": "Synthetic problem one"
            }
          ],
          "text": "Synthetic problem one"
        },
        "subject": { "reference": "Patient/synthetic-subject-0001" }
      }
    }
  ]
}
```

The first parameter, `composition`, holds a canonical openEHR composition of
the template `ferrobridge.diagnose.v1`, with the code in the problem
diagnosis's name. The composition fields FHIR does not carry take their
defaults: the language and the territory from `[operations]`, and the
composer from `[operations] composer`, `FHIRconnect` unless you set it. The
composition's `feeder_audit` names each defaulted value, so a reader can tell
a mapped value from a default.

## 8. Send a composition back to FHIR

The direct form of `$tofhir` takes a composition itself, as
`application/openehr+json`, and answers the FHIR `Bundle` itself. A
composition the CDR stores carries a `uid`, which `$tofhir` derives the
resource id from (FHIRconnect v1.0.0, Engine, ID-Management). The one step 7
answered has none, because nothing committed it, so this request sends that
composition as a CDR would store it, with a `uid` and without the
`feeder_audit`:

<!-- book-request: to-fhir expect=200 holds=/resourceType=Bundle holds=/type=collection holds=/entry/0/resource/resourceType=Condition holds=/entry/0/resource/code/coding/0/code=SYN-001 holds=/entry/1/resource/resourceType=Provenance -->
```http
POST /fhir/tofhir
Content-Type: application/openehr+json

{
  "_type": "COMPOSITION",
  "name": {
    "_type": "DV_TEXT",
    "value": "Diagnosis"
  },
  "uid": {
    "_type": "OBJECT_VERSION_ID",
    "value": "8a2c4f1e-3b7d-4c9a-9e5f-1d2b3c4a5e6f::ferrobridge.example::1"
  },
  "archetype_node_id": "openEHR-EHR-COMPOSITION.ferrobridge_report.v1",
  "archetype_details": {
    "_type": "ARCHETYPED",
    "archetype_id": {
      "_type": "ARCHETYPE_ID",
      "value": "openEHR-EHR-COMPOSITION.ferrobridge_report.v1"
    },
    "template_id": {
      "_type": "TEMPLATE_ID",
      "value": "ferrobridge.diagnose.v1"
    },
    "rm_version": "1.1.0"
  },
  "language": {
    "_type": "CODE_PHRASE",
    "terminology_id": {
      "_type": "TERMINOLOGY_ID",
      "value": "ISO_639-1"
    },
    "code_string": "en"
  },
  "territory": {
    "_type": "CODE_PHRASE",
    "terminology_id": {
      "_type": "TERMINOLOGY_ID",
      "value": "ISO_3166-1"
    },
    "code_string": "NL"
  },
  "category": {
    "_type": "DV_CODED_TEXT",
    "value": "event",
    "defining_code": {
      "_type": "CODE_PHRASE",
      "terminology_id": {
        "_type": "TERMINOLOGY_ID",
        "value": "openehr"
      },
      "code_string": "433"
    }
  },
  "context": {
    "_type": "EVENT_CONTEXT",
    "start_time": {
      "_type": "DV_DATE_TIME",
      "value": "2026-11-26T20:54:18.48952Z"
    },
    "setting": {
      "_type": "DV_CODED_TEXT",
      "value": "other care",
      "defining_code": {
        "_type": "CODE_PHRASE",
        "terminology_id": {
          "_type": "TERMINOLOGY_ID",
          "value": "openehr"
        },
        "code_string": "238"
      }
    }
  },
  "composer": {
    "_type": "PARTY_IDENTIFIED",
    "name": "FHIRconnect"
  },
  "content": [
    {
      "_type": "EVALUATION",
      "name": {
        "_type": "DV_TEXT",
        "value": "Problem diagnosis"
      },
      "archetype_node_id": "openEHR-EHR-EVALUATION.problem_diagnosis.v1",
      "archetype_details": {
        "_type": "ARCHETYPED",
        "archetype_id": {
          "_type": "ARCHETYPE_ID",
          "value": "openEHR-EHR-EVALUATION.problem_diagnosis.v1"
        },
        "rm_version": "1.1.0"
      },
      "language": {
        "_type": "CODE_PHRASE",
        "terminology_id": {
          "_type": "TERMINOLOGY_ID",
          "value": "ISO_639-1"
        },
        "code_string": "en"
      },
      "encoding": {
        "_type": "CODE_PHRASE",
        "terminology_id": {
          "_type": "TERMINOLOGY_ID",
          "value": "IANA_character-sets"
        },
        "code_string": "UTF-8"
      },
      "subject": {
        "_type": "PARTY_SELF"
      },
      "data": {
        "_type": "ITEM_TREE",
        "name": {
          "_type": "DV_TEXT",
          "value": "Tree"
        },
        "archetype_node_id": "at0001",
        "items": [
          {
            "_type": "ELEMENT",
            "name": {
              "_type": "DV_TEXT",
              "value": "Problem name"
            },
            "archetype_node_id": "at0002",
            "value": {
              "_type": "DV_TEXT",
              "value": "Synthetic problem one",
              "mappings": [
                {
                  "_type": "TERM_MAPPING",
                  "match": "=",
                  "target": {
                    "_type": "CODE_PHRASE",
                    "terminology_id": {
                      "_type": "TERMINOLOGY_ID",
                      "value": "http://example.org/fhir/CodeSystem/synthetic-problems"
                    },
                    "code_string": "SYN-001",
                    "preferred_term": "Synthetic problem one"
                  }
                }
              ]
            }
          }
        ]
      }
    }
  ]
}
```

The Bundle carries the `Condition` with the code you sent and the subject
the mapping writes, and one `Provenance` that names the composition the run
read by its `uid`. Send the request again: the `Condition` keeps its `id`,
because the id is a hash of the composition's versioned object id and the
entry path.

## 9. Ask for the FLAT format

`format=flat` answers the composition in the FLAT serialization of the
template instead of canonical JSON:

<!-- book-request: to-openehr-flat expect=200 holds=/resourceType=Parameters -->
```http
POST /fhir/$toopenehr?format=flat
Content-Type: application/fhir+json

{
  "resourceType": "Bundle",
  "type": "collection",
  "entry": [
    {
      "resource": {
        "resourceType": "Condition",
        "meta": {
          "profile": ["http://example.org/fhir/StructureDefinition/tutorial-diagnose"]
        },
        "code": { "text": "Synthetic problem one" },
        "subject": { "reference": "Patient/synthetic-subject-0001" }
      }
    }
  ]
}
```

## 10. Read a refusal

A resource whose profile no context claims is refused with an
`OperationOutcome`, never mapped by a guess:

<!-- book-request: unclaimed expect=400 holds=/resourceType=OperationOutcome holds=/issue/0/code=not-found -->
```http
POST /fhir/$toopenehr
Content-Type: application/fhir+json

{
  "resourceType": "Bundle",
  "type": "collection",
  "entry": [
    {
      "resource": {
        "resourceType": "Condition",
        "meta": {
          "profile": ["http://example.org/fhir/StructureDefinition/no-such-profile"]
        },
        "code": { "text": "Synthetic problem one" },
        "subject": { "reference": "Patient/synthetic-subject-0001" }
      }
    }
  ]
}
```

The outcome names the profiles the Bundle claimed. Every refusal of the
operations is an `OperationOutcome` with an R4 issue code ([The FHIRconnect
operations](../integrate/operations.md#what-a-failure-looks-like)).

## Where to go next

- [Writing and loading mappings](../integrate/mappings.md): extensions,
  sub-contexts, code translation and the full check.
- [The FHIR facade](../integrate/fhir-facade.md): serve the same mappings as a
  FHIR server over an openEHR CDR.
- [How FerroBRIDGE maps](../explanation/how-ferrobridge-maps.md): how the
  files become programs and where each part runs.
