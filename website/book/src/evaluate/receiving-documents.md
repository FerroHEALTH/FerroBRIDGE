<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# Receiving a document

Regulation (EU) 2025/327 asks an EHR system that stores, intermediates or
gives access to health data to "be able to receive personal electronic health
data in the European electronic health record exchange format, by means of
the European interoperability software component" (Annex II, points 2.2 and
2.3). The [interoperability component](interoperability-component.md),
`eehrxf`, reads a received document, checks it against its profiles and maps
it into openEHR, under its `fhir-r4` feature and, for the mapping, `openehr`.
Where the composition is written, who may write it and what is recorded of
the receipt belong to the system that links the crate.

<!-- toc -->

## What the component accepts

A received document is a FHIR R4 `Bundle` in JSON. `ReceivedDocument::read`
reads it in this order and refuses it at the first rule it breaks:

1. An object that repeats a name is refused before anything reads the
   text, because two JSON readers may keep different values for it (RFC
   8259, §4).
2. The text is decoded against the R4 definitions of `Bundle` and of every
   resource it carries. An unknown property, a value of the wrong type, a
   `null` or an empty object or array, and an invalid primitive are refused,
   with the element path where they were found, and so is a resource of a
   type R4 does not define, anywhere in the document. The decoded document
   must encode back to the JSON it was read from. Every later rule, the
   profile checks and the mapping read that one decoded document.
3. The `Bundle` is a `document`, and meets the R4 rules for one: an
   `identifier` with a system and a value (`bdl-9`), a `timestamp`
   (`bdl-10`), `fullUrl`s given once (`bdl-7`), none version specific
   (`bdl-8`), each REST-style one ending with its resource's type and id,
   and a `Composition` as its first entry (`bdl-11`).
4. The `Composition.subject` names one `Patient` entry of the same `Bundle`,
   by its `fullUrl`, or by a relative `Patient/<id>` resolved against the
   server base of the composition's own `fullUrl`. Every reference in the
   document is found by its R4 type, at any depth: in backbone elements,
   contained resources and extensions, and resolved by one resolver,
   `receive::reference::resolve`, which answers exactly one entry or a
   typed refusal. It reads a reference and a `fullUrl` in one canonical
   spelling only: a relative `Type/id`, an `http` or `https` URL with a
   lower-case host, `urn:uuid:` with a lower-case UUID, `urn:oid:`, a
   `_history` version matched on the entry's `meta.versionId`, and a `#id`
   into the resource's own `contained`. Whitespace, a query string,
   percent-encoding, a trailing or doubled slash, a dot segment and a
   resource type in another case are refused, so no reader can take one of
   them for an entry the resolver would not. A `type` or an `identifier`
   beside the reference must agree with its target. A `subject`, `patient`,
   `beneficiary` or `for` must name that same entry by reference. Any other
   reference may name it, an entry of another type, a contained resource
   by `#id`, or a resource outside the document whose path names a type
   other than `Patient`; one that could name another patient (a path or a
   `type` that says `Patient`, or a URN no entry carries) is refused, and so
   is a `type` that disagrees with the reference's target. A second
   `Patient` entry and a contained `Patient` are refused, and so is an
   element the R4 element table does not describe. A document is
   registered under the identification data of the one person it is about
   (Art 13(3)), so a document that could be read as about two is refused
   whole.

The document's text is kept byte for byte. No refusal quotes a value of the
document, so a patient identifier never reaches an error message or a log
line through one. `ReceivedDocument::category` names the priority category
whose document profile fixes a `Composition.type` coding of the document,
and `Category::profiles` the profiles a document of that category is held
to.

## The structural check

`ReceivedDocument::check` walks the `Bundle` profile and the `Composition`
profile of the category, read from the vendored package, over the document:
the cardinality of every element in play, the values a `fixed[x]` or
`pattern[x]` requires, and slicing, each slice matched by its
discriminators. A finding names its element path and the profile element
it breaks. Every constraint the walk meets and does not evaluate, such as an
invariant or a pattern form the dataset model does not read, is listed as
not evaluated, never passed in silence.

## Full validation

`ReceivedDocument::validate` validates the document against the HL7 Europe
`Bundle` profile of its category, for a patient summary `bundle-eu-eps` of
`hl7.fhir.eu.eps` 1.0.0-ballot, with
[`ferrobridge-validator`](https://docs.rs/ferrobridge-validator), over a
package set that holds the profile and its whole dependency closure. It
holds the document, at every occurrence of every element in play, to what
FHIR R4 has a validator check
([validation](https://hl7.org/fhir/R4/validation.html),
[profiling](https://hl7.org/fhir/R4/profiling.html)):

- the structure: unknown elements, the JSON shapes, the cardinality per
  parent, the types a choice may take and the lexical form of every
  primitive;
- the values a `fixed[x]` or `pattern[x]` requires;
- slicing, with all five discriminator kinds, `profile` and paths through
  `resolve()` included, ordered slicing, and each slice's own constraints;
- every invariant of the core definitions and of each profile, evaluated as
  FHIRPath: the R4 `ait-1` of an `AllergyIntolerance`, the `cmp-1` and
  `cmp-2` of a section, the `eps-bundle-subject-ref` of the EPS `Bundle`;
- the terminology bindings, a `required` one as an error and an
  `extensible` one as a warning, read from the value sets the packages carry;
- the profile each entry's slice names, each profile a resource claims in
  `meta.profile`, and the target profiles of every reference;
- the content rules R4 gives a document
  ([documents](https://hl7.org/fhir/R4/documents.html)).

An issue of severity `error` or `fatal` refuses the document, with every
such issue located by its FHIRPath. Every other issue is a finding the
caller reports as a warning, and every constraint the validator met and did
not evaluate (an invariant whose expression calls a function it cannot
answer, a value set no package carries, a reference outside the document) is
listed, never passed in silence. A binding the packages cannot decide is
asked of the terminology server's `ValueSet/$validate-code` when one is
given, and stays listed as not evaluated when none is; a `required` binding
the server refused or did not answer refuses the receipt, so an upstream
failure never reads as a pass. No message quotes a value of the document.

A coding that carries no display is given one by the terminology server's
`$lookup` before the mapping runs, because `DV_CODED_TEXT.value` is
mandatory in the RM (Data Types IM, `DV_CODED_TEXT`). With no terminology
server configured, such a document is refused. The copy the mapping reads
carries the looked-up displays; the document as received is unchanged.

## Mapped into openEHR, with the original kept

`Mapping::to_openehr` maps the document into one openEHR composition through
FHIRconnect mapping files, run in process by the `fhirconnect` engine. The
whole document is the input of FHIRconnect's `$toopenehr`, so the context
that maps it is the one whose profile the document's resources claim, and a
mapping may follow the document's references into its other entries. A
document no context maps, or one with two resources of the type a context
maps, is refused with the engine's reason.

The composition keeps the document itself. The openEHR RM lets
`FEEDER_AUDIT.original_content` carry the original content inline, and puts
one at the composition to establish the equivalence between the whole
composition and the whole document (RM 1.1.0, Common Information Model,
"Original Content"). The component sets it to a `DV_PARSABLE` holding the
document's text with the formalism `application/fhir+json`, beside the
engine's own audit of the system the content passed through. What the
mapping does not carry into structured content is therefore still in the
record, and what the run declared lost comes back beside the composition as
an `OperationOutcome`. `ReceivedComposition::model` is the composition as the
RM model holds it.

## Information a person inserted

Art 5 asks that information a person, or their representative of Art 4(2),
inserts in the person's EHR "be clearly distinguishable as having been
inserted by the natural person or by his or her representative". Whether a
caller is the person, or a representative acting for them, is decided by the
system that authenticates the caller. The component marks the composition
when told:

- `ReceivedComposition::inserted_by_subject` names the record's subject as
  the `provider` of every entry, in the composition and in each of its
  sections, as a `PARTY_SELF` with no `external_ref`, so the mark names no
  identifier of the person. The openEHR RM defines `ENTRY.provider` as "the
  agent who provided the information", which "is usually the patient or the
  clinician" (RM 1.1.0, EHR Information Model, "Entry and its Subclasses"),
  and refers to the subject of the record with a `PARTY_SELF` (Common
  Information Model, "PARTY_SELF and Referring to the Patient from the
  EHR").
- `ReceivedComposition::inserted_by_representative` names the
  representative as the `provider` of every entry, a `PARTY_IDENTIFIED`,
  which the Common Information Model uses for every party other than the
  subject of the record ("Referring to Demographic Entities").

A provider the mapping wrote is replaced, and the document as received stays
in the original content. A composition with no entry, or with a
`GENERIC_ENTRY`, which has no provider, cannot carry the mark and is refused.
