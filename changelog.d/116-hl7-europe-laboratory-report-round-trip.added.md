- **The HL7 Europe Laboratory Report 2.0.0 `Composition-eu-lab` round trip**
  (#116). A FerroBRIDGE document context starts at the report composition of
  the published `KDS_Laborbericht` template and maps it to a
  `Composition-eu-lab`: status, identifier, date, author, the LOINC 11502-2
  type and a title, and one section that carries the test's code and
  references the `DiagnosticReport-eu-lab`, which references its results and
  specimens. The document's report reads its status and identifier out of
  openEHR, because the Composition writes those nodes going in.
