- A `DV_IDENTIFIER` whose `type` is a plain string, such as the `login` of a
  SCIM-provisioned professional, reads as `Identifier.type.coding[0].code`
  with no system instead of failing the read with `500`, as FHIRconnect's
  `String.adoc` §Coding maps an openEHR string without `::` (the RM types
  `DV_IDENTIFIER.type` as a free `String`). A coding with a `code` and no
  `system` now writes the bare code into `DV_IDENTIFIER.type` instead of
  being dropped; one with both still writes `system::code`.
