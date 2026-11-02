- `[change_feed]` (off by default) is the change-feed adapter the consumers
  follow, with two sources: `aql`, which polls ITS-REST 1.1.0 `/query/aql`
  over composition versions by commit instant past a watermark kept in a
  `redb` file and works against any CDR, and `ferroehr`, the reference CDR's
  outbox cursor feed. Each consumer reads under its own reader name, moves
  its watermark only over the events it finished, and reports `degraded`
  without blocking readiness while it cannot follow. The book's change feed
  page states the limits of the AQL source.
