- In a slotted model whose `fhirConfig.structureDefinition` names no
  resource (a `BackboneElement` or a data type), `$resource` now names the
  FHIR element the slot hands over, the root of the model's own type
  (`basics/Variables.adoc`), as `$archetype` names the slotted archetype;
  `$fhirRoot` is unchanged. A `reference` whose `with.fhir` names a choice
  that admits `Reference` now writes that alternative
  (`medication[x]` gives `medicationReference`), and only a choice that
  admits no `Reference` is refused as `fc-reference-into-choice`.
