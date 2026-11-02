- `[events]` (off by default) publishes every commit event the change feed
  reads to an AMQP 0.9.1 broker (RabbitMQ) with publisher confirms, one
  persistent message per changed version under the routing keys the
  reference CDR used (`<kind>.<change_type>.<template_id>`), and moves the
  feed's watermark only after the broker confirmed an event whole. The
  exchange and the bound queues come from the configuration, the broker URL
  has a `url_file` route, an unreachable broker turns the `events` indicator
  degraded without blocking readiness, and the usage report counts
  `events_published`. The book's events lane page carries the migration from
  FerroEHR's own publisher.
