<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# Upgrading

**Audience:** operators moving a deployment to a new release.

FerroBRIDGE is on its 0.0 line, where a release may remove a configuration
key, a route or a stored format with no alias ([Version lines and
releases](versions-and-releases.md)). Every release names what it changed in
the [changelog](../changelog.md), under Changed and Removed. No
specification governs upgrading: this page is FerroBRIDGE's own procedure.

## Before you upgrade

1. Read the changelog sections between your release and the new one, and
   note every removed or renamed key.
2. Run the new binary's check against your configuration, with the same
   environment the server gets:

   ```sh
   ferrobridge --config /etc/ferrobridge/config.toml config check
   ```

   It exits 0 when `serve` would start and 78 with every refusal on stderr
   otherwise. An unknown key names the closest known one
   ([Configuring the server](configuration.md#checking-a-configuration-before-a-restart)).
3. Check the mapping set against the new release:

   ```sh
   ferrobridge --config /etc/ferrobridge/config.toml mapping check
   ```

4. Back up the state store database, which the new release may migrate.

## Roll out

`serve` applies the state store migrations the binary carries when it
starts, under a PostgreSQL advisory lock, so replicas that start together
migrate once ([The state store and high
availability](high-availability.md)). Replace the replicas one at a time:
each new replica migrates or finds the store migrated, and joins the load
balancer once its readiness answers `200`. When the new release adds a
migration, a replica of the old release that restarts after the store was
migrated refuses to start, because the store holds a migration it does not
know; roll back by restoring the backup, never by restarting old replicas.

With the compose quickstart, download the `compose.yaml` the new release
attaches, or set `FERROBRIDGE_VERSION` to the new version, and run
`docker compose up --wait` again ([The container image](container.md)).

## After you upgrade

- Check readiness on every replica, `GET /health/readiness`, and the
  detail on `GET /operator/readiness` when the operator API is on.
- Run `ferrobridge cdm init` once against each CDM database. It applies
  nothing to a CDM schema that holds every table, and brings the bridge's own
  schema to the shape the release needs
  ([What `cdm init` does](configuration.md#what-cdm-init-does)).
- Upgrade the operator console to the same release; it reads the bridge's
  operator documents and is released with it.

## Release-specific notes

- [Upgrading to the state store](upgrade-state-store.md): the release that
  moved the bridge's own state into PostgreSQL removed every key that named
  a local file.
