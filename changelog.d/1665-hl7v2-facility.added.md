- An HL7 v2 message's compositions name their health care facility, read
  from PV1-3.4, PV1-39, ORC-21 and MSH-4 in that order, or name it as the
  originating system's location when they have no `EVENT_CONTEXT`. With
  `[[hl7v2.senders]]` set, a facility no sender names is answered `AE` naming
  it, and nothing is committed. The performing organisation (OBX-23) is
  recorded as the originating system's provider. `fhirconnect` gains
  `Defaults::with_facility`, `Origin::with_location` and `with_provider`.
