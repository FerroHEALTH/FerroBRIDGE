- `fhirconnect` writes a `manual` entry that sets only `value` at a
  `DV_CODED_TEXT` leaf with an open value set as the `DV_TEXT` that leaf may
  carry (Simplified Formats master04 §Open Value-Sets), where it refused a
  coded text with no `defining_code`. An `openehrCondition` with
  `operator: type` tests the class of the value an `ELEMENT` holds, and one
  whose attribute ends on a data value compares that value's text.
  `openehr-mapping-core` answers whether a coded leaf admits free text
  (`WebTemplateIndex::admits_free_text`). The IPS allergy extension of the
  FHIRconnect mapping library now runs verbatim.
