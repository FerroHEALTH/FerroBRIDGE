- **The `[etl]` queries must end their `ORDER BY` in `v/uid/value`.** The
  versions of one contribution share its commit time (openEHR RM Common
  1.1.0, Contributions), so a composition query ordered by
  `v/commit_audit/time_committed/value` alone could tie, and offset paging
  over a tie can skip or repeat a row. `etl run` now refuses to start when
  the composition or visit query's last order key is not the `uid/value` of
  its `VERSION [LATEST_VERSION]` variable; add `, v/uid/value` to the end of
  the `ORDER BY` to upgrade. The book's composition query is the batch
  contract over `VERSIONED_OBJECT`, and the end-to-end tests run against the
  reference CDR release that takes `VERSIONED_OBJECT` in `FROM`.
