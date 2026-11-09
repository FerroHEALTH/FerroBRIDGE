- Breaking for a `ferrobridge-hl7v2` consumer (0.3.0): `parse::profile::Profile`
  gains `repetitions`, the fields whose repetition a profile sets, and
  `IHE_ITI_21` lets QPD-3 repeat in `QBP_Q21` and `RSP_K21` of v2.5, as
  IHE ITI TF-2 §3.21.4.1.2.3 makes it ("one or more repetitions", one per
  query parameter). A writer `with_profile(&IHE_ITI_21)` therefore composes an
  RSP^K22 that echoes a query with two parameters instead of refusing it with
  an `ExtraRepetition` at QPD-3. QPD-2 keeps its one repetition.
