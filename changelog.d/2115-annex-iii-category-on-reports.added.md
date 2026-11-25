- A laboratory or medical test report document with Annex III metadata
  carries the draft EEHRxF `documentCategory` on its `Composition.category`
  and on its `DiagnosticReport.category`, which `Composition-eu-lab` and
  `DiagnosticReport-eu-lab` 2.0.0 slice open and `Bundle-eu-lab`
  `dr-comp-category` holds equal. An imaging report keeps it on the
  `DocumentReference` alone, since its form fixes the category (#2115).
