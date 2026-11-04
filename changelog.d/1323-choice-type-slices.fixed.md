- `eehrxf`'s structural check (`receive::conform`) matches an occurrence of
  a choice element such as `effective[x]` to its type slice by the type its
  property names. It refused a conformant `effectivePeriod` under the closed
  type slicing of `MedicationStatement-eu-eps` before.
