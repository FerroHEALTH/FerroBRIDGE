- **`timingToDaily` carries every time of day.** The CKM daily timing
  cluster repeats its time of day (`at0004`, `0..*`) as R4
  `Timing.repeat.timeOfDay` does, and the function now writes and reads one
  time per instance in order, where it refused more than one. A function
  declares a port repeating through `MappingCodeRegistry::repeating`, the
  compiler accepts such a port whose node repeats directly below the
  mapping's `with.openehr` (`Port::repeats`), and `Ports` holds one value per
  instance (`Ports::with_each`, `Ports::all`, `Ports::ports`). A function
  that writes more than one value at a port whose node holds one is refused
  with the new `MappingCodeError::RepeatedPort`, a breaking change to that
  enum, so a consumer requires `fhirconnect = "0.7"`, `eehrxf = "0.19"` and
  `ferrobridge-hl7v2 = "0.21"`.
