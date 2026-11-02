- The bridge records its own accesses behind a new `[audit]` section. Every
  FHIR facade access an authenticated caller makes is written as the access
  record of Regulation (EU) 2025/327 Annex II 3.2: the caller the token
  verified, the categories of the templates it reached (the
  `[audit.categories]` map), the time and the CDR as the origin. Each message
  the HL7 v2 face commits is written as a system import record. Both are sent
  as IHE BALP `AuditEvent`s over the ITI-20 ATX: FHIR Feed to the Audit Record
  Repository `[audit] base_url` names, from a spool on disk. Recording fails
  closed: an access whose record has no place is refused with `503` before the
  CDR is called, and an HL7 v2 message is answered `AR`.
