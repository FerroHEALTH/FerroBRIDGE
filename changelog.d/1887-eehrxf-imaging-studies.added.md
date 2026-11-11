- `eehrxf` 0.6.3: `Document::studies` (feature `imaging`) returns the
  `fullUrl` of each `ImagingStudy` the document holds, in the order the
  entries were added. A writer whose mapping run added an imaging study
  lists it in the `imagingstudy` section of `CompositionEuImaging`
  without searching the run's `Bundle` again.
