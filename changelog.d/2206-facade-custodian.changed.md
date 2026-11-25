- **Breaking:** an enabled facade requires `[facade.custodian]` (`name`,
  `identifier_system`, `identifier_value`): the healthcare provider that
  holds the records behind the CDR. Every document the bridge writes,
  `Composition/$document`, the documents the MHD Document Responder
  describes, and `Patient/$summary`, names it as its custodian (FHIR R4
  `Composition.custodian`, HL7 Europe EU Health Data API
  `EehrxfMhdDocumentReference`). A configuration without it refuses the
  start, naming the missing key (#2206).
