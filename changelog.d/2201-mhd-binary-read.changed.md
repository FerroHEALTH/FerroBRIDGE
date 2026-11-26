- The MHD Document Responder serves Retrieve Document [ITI-68] only at the
  `$document` URL each `DocumentReference` names, and the facade's
  `CapabilityStatement` keeps listing no `Binary` read while it instantiates
  `IHE.MHD.DocumentResponder`, which R4 allows for a server that implements
  a subset and states its full capabilities.
