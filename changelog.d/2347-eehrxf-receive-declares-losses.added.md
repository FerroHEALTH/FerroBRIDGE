- A received document's composition names each element of the received
  resources its mapping project accounts for and the run did not carry, by
  its FHIRPath and never by its value, in the `other_details` of
  `FEEDER_AUDIT.feeder_system_audit`, beside the original content that keeps
  it, so a later reader of the record is told what the composition lacks.
  `ReceivedComposition::not_carried` returns the same paths, and
  `ReceiveMappingError::DeclarationTaken` refuses a mapping that already
  wrote those details. `eehrxf` moves to 0.48.1.
