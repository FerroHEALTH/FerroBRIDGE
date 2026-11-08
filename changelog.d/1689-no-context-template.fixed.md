- A composition whose operational template prohibits its `context`
  (existence `0..0`, which RM `COMPOSITION.context` `0..1` allows) is built
  with no `EVENT_CONTEXT` and no context defaults, and the facility an HL7 v2
  message names goes to `feeder_audit.originating_system_audit.location`. The
  `openehr-*` crates move to 0.0.130, whose Web Template leaves `/context`
  out of such a template.
