- The quickstart `compose.yaml` each release attaches now carries its
  `.sha256sum`, CI refuses a host path it would read from the repository,
  and the README and the book download it with `curl -fsSLO` from the
  latest release and name Docker Compose 2.23.1 or newer (#1874).
