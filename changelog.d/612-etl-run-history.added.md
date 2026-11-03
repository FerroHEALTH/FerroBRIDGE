- `etl run` and `etl fhir` keep each finished run's report in a run table of
  the bridge schema, which `ferrobridge cdm init` creates, and the operator
  API answers the newest runs at `GET /operator/etl/runs` and one run at
  `GET /operator/etl/runs/{run_id}`. The operator console lists the runs on
  the OMOP ETL page's Runs tab and shows each run's report: when it ran, the
  rows it wrote, its deletion check and every record it refused. When the
  record is refused, the job still prints the report and then exits with the
  refusal.
