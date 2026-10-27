- **The HL7 Europe Base and Core 2.0.0 `medicalTestResult-eu-core` round
  trip** (#116). A FerroBRIDGE context maps the laboratory test result of the
  published `KDS_Laborbericht` template to a `medicalTestResult-eu-core`
  Observation, with its category, code, effective time, status and report
  identifier, and each analyte as a member `Observation-resultslab-eu-lab`
  it references. The result's status travels through the codes the R4
  diagnostic-report-status and observation-status code systems share.
