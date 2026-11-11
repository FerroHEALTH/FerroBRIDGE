- `fhirconnect` unfolds a slot that re-enters an archetype its chain
  already holds as deep as the template instantiates it: at the first level
  the template does not nest the archetype, the method is left out with no
  refusal, where it refused `fc-unknown-template-node` and `fc-slot-depth`.
  A node the template lacks at a level it does instantiate is still refused.
  FHIRconnect v1.0.0 is silent on recursive slots (#1882), so this is
  FerroBRIDGE's own rule. The EEHRxF lab analyte extension of the
  FHIRconnect mapping library now runs verbatim.
