- `[omop_incremental]` (off by default) is a change-feed consumer that keeps
  the CDM database current between batch runs: for each composition an event
  names it reads the latest version over ITS-REST and commits it through the
  per-composition step `etl run` uses, removes the rows of a composition the
  CDR reports deleted or an erasure names, reads the visits and rebuilds the
  derived tables once per pass, and reports degraded as `omop-incremental`.
  A batch run after an incremental run writes the same rows.
