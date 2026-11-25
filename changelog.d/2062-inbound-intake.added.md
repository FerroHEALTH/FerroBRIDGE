- The inbound intake behind `[inbound]`: `POST /inbound/{source}` verifies
  each webhook request over its raw body (`hmac`, Standard Webhooks `v1`,
  `v1a` and `v2a`, `static-header` or `mtls`), reads the event through the
  source's JSON Pointer event map, and stores it with one work item per
  entity in the state store (`0009_baseline_inbound.sql`) before it answers.
  A repeat changes nothing, a wrong signature answers `401` and stores
  nothing, and above the source's `max_pending` or with the store down the
  intake answers `503`. Workers on every replica claim the work with
  `SKIP LOCKED` under a lease and rerun an item an event reached during the
  work. Each source and tenant has one slot pool across replicas, and failed
  work retries with backoff, then parks. `GET /operator/sources/{id}/parked`,
  `POST …/settle` and `POST …/replay` list, settle and replay it. A webhook
  source's document now names `verify` by `kind`, its `keys`, its `events`
  map and its `limits`.
