- The imaging report carries the exposure information of draft EEHRxF Annex
  VII Table 1 (initiative 17512, 2026-11-23), which the Xt-EHR models lack:
  `Document::with_exposure` writes `eehrxf::document::exposure::ExposureInformation`
  into the procedure section of `CompositionEuImaging`, the summary as its
  HL7 Europe Imaging `RadiationDose` extension and each structured dose as an
  `ObservationRadiationDoseEuImaging` part of the study, with its body site,
  dose quantity, modality and number of series. `eehrxf` moves to 0.47.0.
