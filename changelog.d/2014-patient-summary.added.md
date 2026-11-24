- `GET /fhir/Patient/{id}/$summary` answers the patient summary of one
  patient as an HL7 IPS 2.0.1 document, or in the HL7 Europe Patient Summary
  form that `profile` names, behind `[facade.summary]`. The patient comes from
  the `Patient` party context and the record from the patient summary
  compositions of the patient's EHR, read as the caller; each code in no
  preferred system is translated on the terminology server, and the document
  is validated in process before it is served, a `422` when it breaks its
  profile. `eehrxf` 0.23.2 adds the IPS 2.0.1 form beside the EPS one.
