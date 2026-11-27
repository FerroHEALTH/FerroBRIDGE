<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# Checking the CDM

**Audience:** integrators and data stewards who load a CDM and check it.

The Data Quality Dashboard checks the bridge runs itself, the R package for the
rest, and the vocabulary check of OMOCL files.

## Checking a populated database with the Data Quality Dashboard

The bridge carries a Rust port of the OHDSI Data Quality Dashboard's nine
conformance checks, six completeness checks and seven temporal plausibility
checks (v2.9.0, Blacketer et al., JAMIA 2021): observation periods that
overlap, dates before birth or the date they must follow, dates more than 60
days after death, dates a week outside their visit, and starts after their
ends. Each is planned where the Dashboard's own settings enable them, with their
thresholds:

```sh
ferrobridge dq check                 # one line per failing check, then the counts
ferrobridge dq check --format json   # the Dashboard's result fields
```

The job reads the `[cdm]` section, runs every check over the CDM schema
(the vocabulary tables left out, as the Dashboard leaves them out by
default), and exits `1` when a check fails or cannot be evaluated. A check
of a missing or empty table, or of a missing or unpopulated field, is marked
not applicable with its reason, as the Dashboard marks it, and counts as
neither. The JSON document holds `CheckResults` (one object per check, in the
Dashboard's result columns), `Overview` (its counts) and the run's
timestamps. With `[etl] quality_checks = true`, `etl run` runs the same
checks over the tables it wrote and adds their verdicts to the run report; a
failed check never rolls a committed load back.

The other plausibility checks (`plausibleValueLow`, `plausibleValueHigh`,
`plausibleGender`, `plausibleGenderUseDescendants` and
`plausibleUnitConceptIds`) are not ported. To run them, or the R tool's own
viewer, run the Dashboard itself as below.

### Running the R package

The Dashboard itself is an R package
(<https://ohdsi.github.io/DataQualityDashboard/>), with no container image of
its own, so you run it from any R session that can reach the database:

1. Populate the database: `ferrobridge cdm init`, load the vocabulary, then
   `ferrobridge etl run`. The CDM tables are in `[cdm] schema` (`cdm` by
   default); the bridge's own tables are in `[cdm] bridge_schema` and are not
   part of the CDM, so the dashboard is never pointed at them.
2. Create a schema for the results, for example `CREATE SCHEMA dqd_results;`.
3. In R, install the package and the PostgreSQL driver:

   ```r
   install.packages("DataQualityDashboard")
   DatabaseConnector::downloadJdbcDrivers("postgresql", pathToDriver = "~/jdbc")
   ```

4. Connect and run the checks against CDM v5.4:

   ```r
   connectionDetails <- DatabaseConnector::createConnectionDetails(
     dbms = "postgresql",
     server = "localhost/ferrobridge",
     port = 5432,
     user = "ferrobridge",
     password = Sys.getenv("CDM_PASSWORD"),
     pathToDriver = "~/jdbc"
   )
   DataQualityDashboard::executeDqChecks(
     connectionDetails = connectionDetails,
     cdmDatabaseSchema = "cdm",
     resultsDatabaseSchema = "dqd_results",
     cdmSourceName = "FerroBRIDGE",
     cdmVersion = "5.4",
     outputFolder = "dqd-output",
     writeToTable = TRUE
   )
   ```

   The `server` argument is `host/database`, as `DatabaseConnector` documents
   for PostgreSQL.
5. Open the result with `DataQualityDashboard::viewDqDashboard()` on the JSON
   file the run writes into `dqd-output`, and record every failing check with
   its adjudication.

## Validating OMOCL files

OMOCL publishes no JSON schema, so FerroBRIDGE authors one from the grammar
tables and the published mapping library, validates every file against it
before running anything, and offers that schema upstream. The grammar also
leaves the reading of `alternatives` open; FerroBRIDGE tries them in order and
the first present value wins, which is the only reading the published library
is consistent with. A `CustomMapping` names a converter compiled into the
binary, and an unknown name refuses the file. A mapping that fails validation
is refused at load time.

A literal concept id (`code: 3004249`, or a value of a `conceptMap`) is a
concept of the OHDSI vocabulary, so it can only be checked against a loaded
one. With `[cdm]` configured and a vocabulary loaded,

```sh
ferrobridge mapping check --vocabulary
```

adds a diagnostic for every literal the vocabulary does not hold and every
literal whose domain is not one the CDM field definitions name for its column
(`omocl-unknown-concept`, `omocl-domain-mismatch`). Concept `0` is exempt.
Without `[cdm]` or `[mappings] omocl` the flag refuses to start.
