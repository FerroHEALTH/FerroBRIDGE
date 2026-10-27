- **`ferrobridge config check` and `ferrobridge healthcheck`** (#1260), the
  FerroHEALTH family's subcommands from `ferrohealth-server`. `config check`
  prints the running configuration one key per line with where each value
  came from and every secret as `***`, then resolves it as `serve` does and
  exits 78 with every refusal on stderr. `healthcheck` asks the running
  server for `/health/readiness` on loopback and exits 0 only on `200`, and
  the image declares a `HEALTHCHECK` through it. Every subcommand takes
  `--set key=value`, applied over the file and the environment.
