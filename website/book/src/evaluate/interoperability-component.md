<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# The interoperability component

Regulation (EU) 2025/327, the European Health Data Space regulation
(<https://eur-lex.europa.eu/eli/reg/2025/327/oj>), defines the European
interoperability software component for EHR systems as "a software component
of the EHR system which provides and receives personal electronic health
data under a priority category ... in the European electronic health record
exchange format" (Art 2(2)(n)). An EHR system that claims compliance carries
one (Art 25(1)), and Annex II, points 2.1 to 2.3, says what it must provide
and receive.

FerroBRIDGE ships that component as the Rust crate
[`eehrxf`](https://crates.io/crates/eehrxf), published from this repository
on the crate line ([Crate versions](../contribute/crate-versions.md)). An EHR
system or a gateway in front of several of them links the crate and serves
the documents it builds on its own FHIR face; the crate holds no route, no
store and no access record of its own.

Nothing on this page is a conformance claim. The Commission has not adopted
the implementing act that lays down the exchange format (Art 15(1)), so the
crate is built against its published proxies, listed under
[The pinned proxies](#the-pinned-proxies), and is read again against the act
when it is adopted.

<!-- toc -->

## What the crate holds

The crate has one Cargo feature per priority category of Art 14(1) and one
per serialisation, and no default feature:

| Feature | What it adds |
|---|---|
| `patient-summary`, `prescription`, `dispensation`, `imaging`, `laboratory`, `discharge` | the category of Art 14(1)(a) to (f): its Xt-EHR logical model, its obligations profile and its [crosswalk](category-crosswalks.md) to the HL7 Europe profiles |
| `fhir-r4` | the exchange format in FHIR R4: the patient summary document, the prescription and dispensation records, a [received document](receiving-documents.md) read and checked, the terminology rules and the record of each transformation; it compiles no openEHR crate |
| `openehr` | turns on `fhir-r4`, and maps openEHR compositions to the exchange format and back through FHIRconnect 1.0.0 contexts run in process by the `fhirconnect` crate |

Without a feature the crate is the format-neutral dataset model: `dataset`
reads the Xt-EHR *EHDS Logical Information Models* package as data, every
logical model on its canonical URL and every element on its element path,
with what the obligations profiles ask of a producer and a consumer. No
element is a Rust type of its own, so a new release of the models is a new
package, never new code.

`draft` sets the data sets of the draft implementing act of 2026-11-23
(Annexes III to X) beside those models, one row per annex element with its
Xt-EHR element, the cardinality on both sides and how the two differ. The
draft leads where they differ, and the crate's tests fail when the annex
text or the package and the rows disagree.

The crate authors no mapping language and no profile mapping: FHIRconnect
files are its input. It depends on no logging component and on no
application, so it and the European logging component stay independent of
each other, as Art 2(2)(n) and (o) define them. The Rust documentation is
on [docs.rs](https://docs.rs/eehrxf).

## The profile checks

The crate holds what it reads and what it writes to the pinned profiles in
four ways:

- **The crosswalks:** a test per category holds every row to the packages
  it names, and fails when an element a producer must populate has no row,
  when a row names an element or slice the profile lacks, or when a row's
  obligation differs from the obligations profile
  ([The category crosswalks](category-crosswalks.md)).
- **The structural check of a received document:** `ReceivedDocument::check`
  walks the document's `Bundle` and `Composition` profiles: cardinalities,
  `fixed[x]` and `pattern[x]` values and slices, with every constraint it
  cannot evaluate listed
  ([The structural check](receiving-documents.md#the-structural-check)).
- **The full validation of a received document:**
  `ReceivedDocument::validate` hands the document to
  [`ferrobridge-validator`](https://docs.rs/ferrobridge-validator), this
  repository's FHIR R4 profile validator, over the category's `Bundle`
  profile and its whole dependency closure
  ([Full validation](receiving-documents.md#full-validation)).
- **HL7's FHIR Validator over the documents it writes:** the suites that
  write a patient summary or a laboratory report document and hold it to the
  profiles also write it to the directory `EEHRXF_EMIT_DIR` names, and so
  does the suite that maps a prescription and a dispensation to their MPD
  resources. The CI `fhir-validator` job runs HL7's FHIR Validator over
  every one of them, offline, against `hl7.fhir.eu.eps` 1.0.0-ballot,
  `hl7.fhir.eu.base` 2.0.1, `hl7.fhir.eu.laboratory` 2.0.0,
  `hl7.fhir.eu.mpd` 1.0.0, the remaining-dispensations extension and the
  MyHealth@EU Master Value Sets Catalogue, with the jar and every package
  pinned by sha256 in `docs/VERSIONS.md`. It fails on an error, a fatal issue
  or a claimed profile the validator could not find. Its results, each
  document with the validator's outcome and a summary, are the
  `eehrxf-fhir-validation` artifact of the run. They are the evidence for the
  documents the component writes.
- **The MyHealth@EU guides on their CI line:** the same job measures the
  component's documents against four MyHealth@EU guides as built on
  `build-fhir.ehdsi.eu`, each pinned by sha256 because the publisher
  overwrites a build in place: each laboratory report against Laboratory
  9.2.0, each discharge report against HDR 9.2.0, every entry of a type Core
  1.0.1 profiles against that profile, and every coding against the value
  sets of the Master Value Sets Catalogue 10.1.0. Each guide is a conformance
  corpus with a pass list under `conformance/myhealth-eu-*/`, so a case that
  stops passing fails the job, and its README badge shows the count. A badge
  is a self-test against the named CI build, never a certification.

## The Annex III metadata

Art 4 and Annex III of the draft implementing act put the metadata of
Table 1 on every document and the metadata of Table 2 on every data group.
`document::metadata` writes them: `DocumentMetadata` holds what Table 1 asks
beyond the parts a document already takes, `Document::with_metadata` writes
it on the `Composition`, and `DocumentMetadata::describe` writes the
`DocumentReference` describing the document, with the `masterIdentifier`,
`type`, `subject`, `date` and `description` MyHealth@EU
DocumentReference-minimal requires. `DocumentMetadata::from_composition`
reads the version, the language and the period from the openEHR
composition. With metadata set, every data group takes the document's date
and language where it carries none, and one with no subject is refused. The
crate documentation lists where each Table 1 element is written. FHIR R4
holds one `Composition.language` and one `practiceSetting`, so a second value
is refused rather than dropped.

Xt-EHR D5.2 Table 3 adds traceability metadata that Annex III does not ask
for. The draft leads, so each is recorded as followed or not:

| D5.2 Table 3 element | Followed | Where, or why not |
|---|---|---|
| `authorRole` | no | Annex III asks for no role; `author[x]` names the author |
| `sourceSystem` | yes | `meta.source` of the `Composition` and the `DocumentReference`, when the caller sets it |
| `creationDate` | yes | `Composition.date` and `DocumentReference.date`, Table 1 `date` |
| `lastModified` | no | Annex III Table 1 has no such element; a server sets `meta.lastUpdated` |
| `provenanceEvent` | yes | the `Provenance` each mapping run writes |

## The representations

Art 3(1) of the draft implementing act exchanges a document as (a) a PDF
attachment embedded in a FHIR resource with its metadata, (b) FHIR
narrative by data group, (c) structured FHIR with its narrative, or (d) for
imaging, DICOM with a FHIR report and manifest. Annex II admits PDF/A-1a or
PDF/A-1b alone, and admits (d) for imaging only.

- **(a) PDF:** `PdfA::new` accepts a PDF only when its XMP metadata carries
  the PDF/A identification schema with part 1 and conformance A or B, and
  refuses any other with a typed error. `DocumentMetadata::describe_pdf`
  embeds the bytes unchanged in a `DocumentReference`, `contentType`
  `application/pdf`, with their size and SHA-1, beside the full Annex III
  metadata. The `DocumentReference` is the FHIR resource Art 3(1)(a) names,
  and the one MyHealth@EU searches by content type.
- **(b) Narrative:** `Document::narrative_only` writes the sections with
  their narrative and refuses a section given a structured entry. A section
  rule of the category's profile that asks for an entry or an empty reason
  still holds, so a category whose profile does not permit a narrative-only
  section refuses it.
- **(c) Structured** is what every category's document writes.

Which representations a system declares per category is configuration: a
`Declarations` map from the category code of HL7 Europe's
`EEHRxFDocumentPriorityCategoryCS` (`Patient-Summaries`,
`Laboratory-Reports` and the others) to its representations (`pdf`,
`narrative`, `structured`, `dicom`), read with serde from the system's
configuration and held to Annex II by `Declarations::check`.
`Declarations::structured` declares structured FHIR for every category, and
DICOM beside it for imaging.

## The definitions it publishes

The component writes two things no specification defines, and FerroBRIDGE
publishes both under its own domain:

- **The archetype-local code systems:** an archetype's local codes are
  written under `https://ferrobridge.eu/fhir/CodeSystem/archetype-local/`
  followed by the archetype id, one code system per archetype. The page at
  that base describes them.
- **The remaining-dispensations extension:** the number of dispensations a
  prescription item has left travels in
  `https://ferrobridge.eu/fhir/StructureDefinition/remaining-dispensations`
  on `MedicationRequest.dispenseRequest`, as a `valueUnsignedInt`. The site
  serves its `StructureDefinition` at that URL, and at the same URL with
  `.json`.

## The pinned proxies

Each proxy is vendored by `scripts/vendor/eehrxf.sh` as the archive its
publisher serves, pinned by URL and sha256 in `docs/VERSIONS.md`, and read by
the crate's tests:

| Proxy | Version | What the crate reads it for |
|---|---|---|
| Xt-EHR EHDS Logical Information Models (`xtehr.eu.ehds.models`) | 1.0.0 | the dataset model of every category |
| HL7 Europe Patient Summary (`hl7.fhir.eu.eps`) | 1.0.0-ballot | the patient summary document and its crosswalk |
| HL7 Europe Base and Core (`hl7.fhir.eu.base`) | 2.0.1 | the core profiles a patient summary section claims |
| HL7 Europe Medication Prescription and Dispense (`hl7.fhir.eu.mpd`) | 1.0.0 | the prescription and dispensation records |
| HL7 Europe Laboratory Report (`hl7.fhir.eu.laboratory`) | 2.0.0 | the laboratory report crosswalk and document |
| HL7 Europe Imaging Report (`hl7.fhir.eu.imaging`) | 1.0.0-ballot | the imaging report and imaging study crosswalks |
| HL7 Europe Hospital Discharge Report (`hl7.fhir.eu.hdr`) | 1.0.0 | the discharge report crosswalk and document |
| MyHealth@EU Master Value Sets Catalogue (`myhealth.eu.fhir.mvc-package`) | 9.1.0 | the preferred system of each terminology |

## Where it stands

The patient summary is the most complete category: the crosswalk, the
document `Bundle`, its mapping from openEHR, the terminology rules and the
receipt are built. The prescription and dispensation records are read and
written in the MPD form, and [mapped from and into openEHR](prescription-mapping.md)
through FerroBRIDGE's MPD contexts. The laboratory report is
[mapped from and into openEHR](laboratory-mapping.md) through the shipped
laboratory project and received as a `Bundle-eu-lab`. The imaging report is
[mapped from and into openEHR](imaging-mapping.md) through the shipped
imaging project, over FerroBRIDGE's template of published CKM archetypes,
and received as a `BundleReportEuImaging`. The discharge report is
[mapped from and into openEHR](discharge-mapping.md) through the shipped
HDR project over FerroBRIDGE's discharge report template and received as a
`bundle-eu-hdr`. The open
work is on the [tracker](https://github.com/FerroHEALTH/FerroBRIDGE/issues).
The profiles this repository's own FHIR facade maps to, category by category,
are on [EHDS readiness](ehds-readiness.md).
