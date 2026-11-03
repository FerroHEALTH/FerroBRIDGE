- `eehrxf` maps electronic prescriptions and dispensations between openEHR
  and the HL7 Europe MPD 1.0.0 resources through FerroBRIDGE's FHIRconnect
  contexts. `Mapping::compile_template` compiles a mapping against an ADL 2
  operational template as well as an OPT 1.4 one.
  `ReceivedResources::read` reads a `collection` Bundle as strictly as a
  document, and `Mapping::resources_to_openehr` maps it into one composition
  that keeps the collection as its original content.
  `ReceivedComposition::instructed_by` names the order a received dispense
  carries out. The MPD request mapping now carries the refills authorised,
  their least interval, the refills remaining, and the amount and duration
  of one fill, from the CKM medication authorisation and supply amount
  clusters. Every mapped `MedicationRequest` carries the status `unknown`,
  because no published crosswalk pairs the openEHR order status with the R4
  request status.
