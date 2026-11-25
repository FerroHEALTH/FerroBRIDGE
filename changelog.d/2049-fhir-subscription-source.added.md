- `fhir-subscription` inbound sources receive FHIR R4 Subscription
  notifications of the Subscriptions R5 Backport IG STU 1.1.0 on
  `POST /inbound/{source}`, verified by the `Subscription.channel.header`
  value, the client certificate's names under `mtls`, or both. A handshake
  records the subscription, each `id-only` or `full-resource` event becomes
  one work item for its patient, and a gap a heartbeat or an `empty` event
  reveals is recovered through `$events` by the replica that leads the
  source's recovery. The document gains `mtls`, `subscriptions` and `base`.
