- The CDM database runs on PostgreSQL 19: `cdm init`, `vocab load`, `etl
  run` and every other CDM job refuse an older server when they connect,
  naming the `server_version_num` found and the 190000 needed. The
  quickstart's `cdm` profile runs `postgres:19.0` with its volume at
  `/var/lib/postgresql`, where the official image keeps its data.
  PostgreSQL 18 is no longer supported and there is no upgrade path: a
  `cdm-data` volume holding an older cluster is removed and created again.
