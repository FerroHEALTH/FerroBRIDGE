- **The `eehrxf` crate, the European interoperability software component of
  an EHR system under Regulation (EU) 2025/327 (Art 2(2)(n), Art 25(1)),
  joins the crate line at 0.1.128.** It moves here from the FerroHEALTH
  federation gateway, where it was built up to 0.0.40 and never published,
  so its earlier history is that project's changelog. It carries the
  format-neutral dataset model read from the Xt-EHR EHDS Logical Information
  Models, one feature per Art 14(1) category with its crosswalk to the HL7
  Europe profiles, the exchange format in FHIR R4 (`fhir-r4`), and the
  mapping between openEHR and that format through `fhirconnect`
  (`openehr`). Against 0.0.40 its one change of behaviour is the id of a
  resource mapped from a composition, which is now the FHIRconnect
  deterministic id `fhirconnect` 0.1.203 derives from the composition's uid
  and the entry path (FHIRconnect v1.0.0, engine/id-management.adoc,
  Deterministic IDs). The corpora its tests read are vendored by
  `scripts/vendor/eehrxf.sh`, and the book gains the component's pages: the
  component, the category crosswalks and receiving a document.
