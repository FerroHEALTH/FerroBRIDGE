- A composition the facade commits for a machine caller names the caller's
  client as the originating system (`feeder_audit.originating_system_audit.system_id`)
  and the bridge as the feeder system (`feeder_system_audit.system_id`).
  `fhirconnect` gains `Origin::with_originating_system` for it.
- `[cdr.credentials.onward]` refuses the start when `[cdr.credentials.oauth]`
  names neither `resource` nor `scope`, so the bridge's own token, which a
  machine's request reaches the CDR under, is asked for the CDR alone.
