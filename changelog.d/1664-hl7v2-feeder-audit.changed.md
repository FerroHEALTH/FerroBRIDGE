- An HL7 v2 message's compositions name the sending application (MSH-3) as
  the originating system and the bridge as the feeder system in
  `FEEDER_AUDIT`, record the placer and filler numbers (ORC-2, ORC-3, OBR-2,
  OBR-3) as `originating_system_item_ids`, and move the message control id
  (MSH-10) to `feeder_system_item_ids`. `fhirconnect` gains
  `Origin::with_originating_items` and `with_feeder_items` for it.
