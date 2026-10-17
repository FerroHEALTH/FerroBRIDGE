<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# ferrobridge-viewer

The FerroBRIDGE Console, the operator console: the `ferrobridge-viewer`
binary, a Leptos app rendered on the server and hydrated in the browser, with
a backend-for-frontend in front of the bridge.

Part of [FerroBRIDGE](https://ferrobridge.eu), a pure-Rust bridge between
openEHR and two interoperability targets: HL7 FHIR through the FHIRconnect
specification, and the OMOP Common Data Model through the OMOCL
specification. This crate is never published to crates.io.

The console is one more client of the bridge's public HTTP surface. Its
server calls the bridge over HTTP and never compiles or runs a mapping
itself. Built so far: the health route, a minimal page under the browser
security headers, the configuration with its environment overrides and
`_file` secrets, the typed bridge client, and the check that holds the crate
to the two workspace crates it may link.

## The subcommands

| Command | What it does |
|---|---|
| `ferrobridge-viewer serve` | Serves the console until the process is asked to stop |
| `ferrobridge-viewer config check` | Reads and resolves the configuration and exits, binding nothing |
| `ferrobridge-viewer healthcheck` | Asks the console on this host for `GET /health`; exits `0` only on `200` |

`--config <PATH>` names the configuration file, or `FERROBRIDGE_VIEWER_CONFIG`
does. A refused configuration exits `78`.

## Configuration

```toml
[server]
listen = "0.0.0.0:3000"
site_root = "/site"

[bridge]
base_url = "https://bridge.example.org/"
timeout_ms = 30000
```

Every key can be set from the environment as
`FERROBRIDGE_VIEWER__<SECTION>__<KEY>`, for example
`FERROBRIDGE_VIEWER__BRIDGE__BASE_URL`, and the environment wins over the
file. A secret is set inline or read from the file its `<key>_file` sibling
names, never both.

## Building

Build it with cargo-leptos, from the repository root:

```sh
scripts/release/viewer-tools.sh "$HOME/.cache/ferrobridge-viewer-tools"
export PATH="$HOME/.cache/ferrobridge-viewer-tools/bin:$PATH"
scripts/release/viewer-site.sh --release
cargo build --release -p ferrobridge-viewer
```

The first line installs the Tailwind CSS CLI and `wasm-opt` at their pinned
releases, each checked against its SHA-256, and the site script refuses to
run cargo-leptos unless the ones on `PATH` match those digests, so it never
downloads either itself. That writes the site bundle under `target/site` and
the server binary under `target/release`.

## Licence

Business Source License 1.1 (`LICENSE`): free for every non-production use
and for non-commercial production use; a commercial licence for other
production use; Apache License 2.0 four years after each version.
