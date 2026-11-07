- An HL7 v2 message's compositions name the clinician the message names as
  their composer, from the first of EVN-5, OBX-16, ORC-12 and OBR-16, with
  its identifier. A message carrying a result that names no clinician is
  answered `AE`. A sender declared `origin = "device"` with its
  `device_identifier_issuer` in `[[hl7v2.senders]]` commits its results as
  acts of the device: the composer names the equipment (OBX-18), and the
  device is the originating system. `fhirconnect` gains
  `Defaults::with_composer_id`.
