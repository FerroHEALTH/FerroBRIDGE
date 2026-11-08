- `fhirconnect` compiles an openEHR path that steps through a `List`
  attribute of the reference model, such as `DV_TEXT.mappings` or
  `DV_ORDERED.other_reference_ranges`, as an occurrence axis inside the
  node's value: a mapping anchored on the list takes one element per input
  occurrence, its children name the attributes of that element
  (`meaning`, `range/lower`), and the run reads and writes each element in
  both directions. A path that ends on a list with no child to fill an
  element is still refused at load as `fc-uncarried-tail`. The range of a
  `REFERENCE_RANGE<DV_QUANTITY>` reads as `DV_INTERVAL<DV_QUANTITY>`, the
  type the openEHR RM gives it. `OpenehrTarget::new` and
  `OpenehrTarget::with_element_tail` are no longer `const`, and
  `OpenehrTarget::lists` and `ListAxis` are new.
- The `eehrxf` laboratory mapping carries a result's typed reference ranges
  both ways: a received `Observation.referenceRange` with a `type` becomes
  one `other_reference_ranges` entry of the result's quantity, its `meaning`
  from the `type`, and is no longer declared lost, and each entry is served
  back as a `referenceRange` with that `type`.
