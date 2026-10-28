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

The crate authors no mapping language and no profile mapping: FHIRconnect
files are its input. It depends on no logging component and on no
application, so it and the European logging component stay independent of
each other, as Art 2(2)(n) and (o) define them. The Rust documentation is
on [docs.rs](https://docs.rs/eehrxf).

## The profile checks

The crate holds what it reads and what it writes to the pinned profiles in
three ways:

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
written in the MPD form. The laboratory, imaging and discharge reports have
their crosswalks and their document assembly; building each report from
openEHR content waits on the mapping contexts for their profiles. The open
work is on the [tracker](https://github.com/FerroHEALTH/FerroBRIDGE/issues).
The profiles this repository's own FHIR facade maps to, category by category,
are on [EHDS readiness](ehds-readiness.md).
