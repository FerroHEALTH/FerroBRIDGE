- A sent imaging or medical test report whose composition holds the
  report-result `Comment` (at0007) declared nothing about it, because no
  mapping reads the composition's context and the comment reached no FHIR
  element. That comment is where a received report names the elements it
  lost. Now the run declares the comment left unread, in its warnings and in
  the Bundle's `OperationOutcome`, whenever a project declares a comment
  (`Project::note`) and no mapping carries it.
