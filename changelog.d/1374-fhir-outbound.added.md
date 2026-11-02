- The outbound FHIR lane, behind a new `[fhir_outbound]` section, follows the
  change feed and delivers each changed composition to a FHIR R4 endpoint as
  one `transaction` Bundle per event, mapped through the facade's programs:
  every resource is a `PUT` carrying its `id`, `meta.versionId` and
  `meta.source`, and a deletion, an erasure or a resource a new version no
  longer maps to is a `DELETE`. Delivery is at least once; a composition of an
  EHR bound to no patient, or a resource without its subject, is refused and
  parked; the start warns that the lane carries identifiable data.
