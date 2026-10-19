<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# The container image

FerroBRIDGE ships as one image carrying one binary, and the
[operator console](operator-console.md) as a second image beside it. This
page describes both images, the quickstart `compose.yaml`, the variables the
server reads, and the health probe you point your orchestrator at. Each
release publishes the bridge to `ghcr.io/ferrohealth/ferrobridge` and the
console to `ghcr.io/ferrohealth/ferrobridge-viewer` under its version tag,
with provenance and SBOM attestations, beside the release binaries on the
GitHub release page.

No specification governs packaging, the process model, or health probes: this
page is FerroBRIDGE's own design.

<!-- toc -->

## What the image is

| Fact | Value |
|---|---|
| Registry and repository | `ghcr.io/ferrohealth/ferrobridge` |
| Tag | the product version, for example `0.0.1`; each release publishes its own |
| Base | `gcr.io/distroless/static-debian13:nonroot`, pinned by index digest |
| Platforms | `linux/amd64` and `linux/arm64` |
| User | uid 65532, gid 65532, written numerically |
| Entrypoint | `/usr/local/bin/ferrobridge`, exec form, with `serve` as the default command |
| Port | 8080/tcp |

The base is distroless static: CA certificates, time zone data, an
`/etc/passwd` carrying the non-root user, and a `/tmp`. There is no shell, no
package manager, and no HTTP client, so the image cannot run a script and
nothing can be installed into it at runtime. The binary is a static musl build
that the release lane produces, attests, and stages before the image is built;
the image compiles nothing.

The user is written as `65532:65532` rather than as a name because the kubelet
cannot resolve a name against an image it does not read, so a named `USER`
makes `runAsNonRoot: true` refuse the pod.

Every build-independent OCI label is set in the recipe
(`org.opencontainers.image.title`, `.description`, `.url`, `.documentation`,
`.source`, `.vendor`, `.licenses`, `.base.name`). The release lane adds the
version, revision and creation labels at build time, so the same file produces
a correctly labelled image at every cut.

The tag scheme follows the product version, and a version is never rebuilt: a
bad cut ships forward as the next patch version.

## Running it with compose

`compose.yaml` at the repository root is the quickstart, and it ships as a
release asset. Download it, create the secret files beside it, and start:

```sh
mkdir -p secrets
printf '%s' "$CDM_PASSWORD" > secrets/cdm_password
printf 'postgresql://ferrobridge:%s@cdm:5432/omop?sslmode=disable' "$CDM_PASSWORD" > secrets/cdm_url
printf '%s' "$CDR_PASSWORD" > secrets/cdr_password
printf '%s' "$TX_PASSWORD" > secrets/terminology_password

docker compose up
curl http://127.0.0.1:8080/health/liveness
```

Compose refuses to start a service whose secret source file is missing, so
create all four even when a lane is off; a file nothing reads may be empty.
Never commit them.

The published port binds `127.0.0.1` by default. A published port is DNAT'd
ahead of the host firewall's own chains, so a port published on `0.0.0.0` is
reachable from the network even when the firewall says otherwise
([Docker: packet filtering and
firewalls](https://docs.docker.com/engine/network/packet-filtering-firewalls/)).
To serve another machine, name that interface explicitly or put a reverse proxy
in front:

```sh
FERROBRIDGE_BIND_HOST=10.0.0.7 docker compose up
```

The service runs with a read-only root filesystem, every Linux capability
dropped, and `no-new-privileges:true`. There is no `tmpfs` beside it because
the quickstart's server writes almost nothing: no cache, no scratch file, no
upload directory, and every secret is read once at boot. It keeps the
[usage report](usage-report.md)'s instance id on the named volume
`ferrobridge-state`, mounted at `/home/nonroot`, the image's home directory,
which its runtime user owns. The other file the server writes is the FHIR
facade's identity store, and the facade is off in the quickstart;
[the identity store](#the-identity-store) says what to mount when you turn it
on.

## The identity store

With `[facade] enabled = true`, the server keeps its identity map in one
file, `[facade] identity_store` (`FERROBRIDGE__FACADE__IDENTITY_STORE`). The map
records each subject's `ehr_id`, each FHIR resource id and the composition it
came from, and the source identity of every resource the facade consumed. It
holds no clinical content. The server opens the file at boot, creating it when
it is absent, and refuses to start when it cannot. The default, `identity.redb`,
is relative to the working directory, which is `/` in the image: root-owned and
read-only, so set the key to a path on a volume.

- **Where it lives:** a path on a writable volume mounted into the container,
  such as `/var/lib/ferrobridge/identity.redb`. The image runs as user
  `65532`, so the volume must be writable by that user (in Kubernetes,
  `securityContext.fsGroup: 65532` on the pod).
- **What volume it needs:** a persistent one. The file survives a restart
  only if the volume does, so use a `PersistentVolumeClaim` (or a named volume
  under Compose), never an `emptyDir` or a `tmpfs`. A lost file is a lost map:
  the next create of a resource the CDR already holds writes a second
  composition, and a client's FHIR ids stop resolving. Back the file up with
  the CDR.
- **Replicas:** one process per file. The store takes an exclusive lock on the
  file when it opens it, so a second process pointed at the same file refuses
  its start, and a network filesystem that does not honour the lock is not a
  safe place for it. Two replicas with a file each would keep two maps that
  disagree. Run the facade as a single replica (a `StatefulSet` of one with a
  `ReadWriteOnce` claim), and use the `Recreate` strategy for a `Deployment`,
  so the old pod releases the file before the new one opens it.
- **Restarts:** the map is read from the file at boot, so a restarted process
  answers with the ids it handed out before. The claims that order
  overlapping deliveries live in the process, and a restart drops them with
  the deliveries they held; a retry is then recognised from the identity map
  ([Failure and identity behaviour](failure-and-identity.md)).

A Kubernetes pod spec with a read-only root filesystem keeps the store on its
claim:

```yaml
securityContext:
  runAsNonRoot: true
  fsGroup: 65532
containers:
  - name: ferrobridge
    securityContext:
      readOnlyRootFilesystem: true
    env:
      - name: FERROBRIDGE__FACADE__IDENTITY_STORE
        value: /var/lib/ferrobridge/identity.redb
    volumeMounts:
      - name: identity
        mountPath: /var/lib/ferrobridge
volumes:
  - name: identity
    persistentVolumeClaim:
      claimName: ferrobridge-identity
```

## The environment variables

Every key is settable as `FERROBRIDGE__<SECTION>__<KEY>`, and
[Configuring the server](configuration.md) is the complete reference with every
default and every refusal. This section covers the variables the quickstart
names.

The image sets one of them itself: `FERROBRIDGE__SERVER__LISTEN=0.0.0.0:8080`,
because the binary's own default is loopback and no container can publish a
loopback port.

The quickstart passes the upstream variables through from your shell or your
`.env` file rather than defaulting them, because an empty base URL is a boot
error while an absent one is a lane the server never starts. Set the ones you
use:

```sh
FERROBRIDGE__CDR__BASE_URL=https://cdr.example.org/openehr/v1
FERROBRIDGE__CDR__CREDENTIALS__USER=ferrobridge
FERROBRIDGE__CDR__CREDENTIALS__PASSWORD_FILE=/run/secrets/cdr_password
FERROBRIDGE__TERMINOLOGY__BASE_URL=https://tx.example.org/r4
FERROBRIDGE__TERMINOLOGY__WIRE_VERSION=r4
FERROBRIDGE__TERMINOLOGY__CREDENTIALS__USER=ferrobridge
FERROBRIDGE__TERMINOLOGY__CREDENTIALS__PASSWORD_FILE=/run/secrets/terminology_password
FERROBRIDGE__CDM__URL_FILE=/run/secrets/cdm_url
```

The CDR takes basic authentication, as above, or a token the bridge obtains
with the OAuth 2.0 client credentials grant: leave the two basic variables
unset and set the `FERROBRIDGE__CDR__CREDENTIALS__OAUTH__` variables
([`[cdr.credentials.oauth]`](configuration.md#cdrcredentialsoauth-and-terminologycredentialsoauth)), with the
client secret in a compose secret of your own.

The terminology server takes the same two modes under its own names:
`FERROBRIDGE__TERMINOLOGY__CREDENTIALS__USER` and
`FERROBRIDGE__TERMINOLOGY__CREDENTIALS__PASSWORD_FILE`, read from the
`terminology_password` secret as above, or
`FERROBRIDGE__TERMINOLOGY__CREDENTIALS__OAUTH__ISSUER` (or `__TOKEN_ENDPOINT`),
`__CLIENT_ID`, `__CLIENT_SECRET_FILE` and `__AUDIENCE`
([`[terminology.credentials.oauth]`](configuration.md#cdrcredentialsoauth-and-terminologycredentialsoauth)).
The CDR takes the same five OAuth variables under
`FERROBRIDGE__CDR__CREDENTIALS__OAUTH__`. Leave every credential variable of a
server that checks no credentials unset.

Every credential is reachable through a `_file` sibling read once at boot, and
that is what the compose secrets are for: a secret mounts at
`/run/secrets/<name>`, so the `_file` variable names that path and the value
never enters the environment, where `docker inspect` would show it. Setting a
value together with its `_file` sibling is a boot error, and a refused
configuration exits 78.

`FERROBRIDGE_CONFIG` names a TOML file instead, for a deployment that mounts
its configuration rather than setting variables.

A container's stdout is no terminal, so the default `auto` format writes one
JSON object per line with no colour and no banner, and you configure nothing
for a log collector to read it. Set `FERROBRIDGE_LOG_FORMAT=pretty` to read
the banner and human-readable lines in `docker compose logs`
([The console](configuration.md#the-console)).

## The health probe

The image declares no `HEALTHCHECK`. A distroless image has no shell and no
HTTP client, and the binary carries no probe subcommand, so the probe is
external. Two endpoints answer it:

- `GET /health/liveness` answers `200` while the process is up. Restart the
  container when it stops answering.
- `GET /health/readiness` runs one bounded check per configured upstream and
  answers `200` when every one of them answered. Each check sends the
  configured credentials: the CDR check lists the ADL 1.4 templates
  (`GET /definition/template/adl1.4`, filtered so the answer is empty), and a
  `401` or `403` from either upstream counts as down, because every real call
  would be refused the same way. With a `[cdm]` section, a `cdm` check opens
  one connection to the CDM database over the configured TLS and asks
  `SELECT 1`; a refused connection or a failed query counts as down. Take the
  instance out of the load balancer when it does not answer `200`.

Readiness answers `503` when any indicator is down, with a JSON body naming the
aggregate and every indicator by name:

```json
{
  "state": "down",
  "indicators": {
    "cdr": { "state": "up" },
    "terminology": { "state": "down", "detail": "the upstream answered 503 Service Unavailable" }
  }
}
```

The `detail` names the upstream status or the reason the call never reached it,
and never a body, a credential, or clinical content. A probe that reaches the
upstream counts it up, `401` and `404` included; a `5xx` and a failure to
connect count it down. Each check is bounded, so a wedged upstream cannot hang
the probe. In Kubernetes, point `livenessProbe` at `/health/liveness` and
`readinessProbe` at `/health/readiness`.

The server fetches the templates its mappings name from the CDR before it
serves. When the CDR does not answer that fetch yet (a refused connection, a
timeout, or a `5xx` other than `501`), the process stays up: it binds the
listen address at once, answers liveness with `200` and readiness with `503`,
and tries the start again after 1 second, doubling the wait up to 30 seconds.
Readiness then carries a `startup` indicator whose `detail` names the attempt
and why it failed, beside the `cdr` indicator. A documented answer such as a
`404` for a template the CDR does not hold still stops the start. On
Kubernetes this keeps a pod that starts before its CDR out of a crash loop.

## The operator console image

The console is a separate binary, `ferrobridge-viewer`, that calls the
bridge over its public HTTP surface. Its image follows the bridge image's
rules:

| Fact | Value |
|---|---|
| Registry and repository | `ghcr.io/ferrohealth/ferrobridge-viewer` |
| Tag | the product version, the same as the bridge image of that release |
| Base | the bridge image's base, at the same index digest |
| Platforms | `linux/amd64` and `linux/arm64` |
| User | uid 65532, gid 65532, written numerically |
| Entrypoint | `/usr/local/bin/ferrobridge-viewer`, exec form, with `serve` as the default command |
| Site bundle | `/site`, root-owned and read-only to the runtime user |
| Port | 3000/tcp |
| Health check | `HEALTHCHECK` through `ferrobridge-viewer healthcheck` |

The image compiles nothing: the release lane builds the static console
binary per architecture and the site bundle (the WebAssembly, its JavaScript
glue and the stylesheet), attests both, and stages them before the image is
built. It sets `FERROBRIDGE_VIEWER__SERVER__LISTEN=0.0.0.0:3000` and
`FERROBRIDGE_VIEWER__SERVER__SITE_ROOT=/site`, because the binary's own
defaults are loopback and a checkout's `target/site`.

Unlike the bridge image, the console image declares a `HEALTHCHECK`. The base
still has no shell and no HTTP client, so the probe is the binary's own
`healthcheck` subcommand: it reads the same configuration as `serve`, asks
`GET /health` on the listen port over loopback within 3 seconds, and exits 0
on a `200` and 1 otherwise. Docker and Compose read the instruction;
Kubernetes ignores it, so point a probe at `GET /health` on port 3000 there.

Every key of the console's configuration is settable as
`FERROBRIDGE_VIEWER__<SECTION>__<KEY>`, and every secret through its `_file`
sibling, the same way the bridge reads its own
([The operator console](operator-console.md) lists the keys).

## The viewer profile

The `viewer` profile of `compose.yaml` runs the console beside the bridge:

```sh
printf '%s' "$CLIENT_SECRET" > secrets/viewer_oidc_client_secret
docker compose --profile viewer up
```

It publishes port 3000 on `127.0.0.1` (`FERROBRIDGE_VIEWER_PORT` moves it,
and `FERROBRIDGE_BIND_HOST` names another interface, as for the bridge), and
reaches the bridge at `http://ferrobridge:8080/` over the compose network.
The service runs with a read-only root filesystem, every Linux capability
dropped, and `no-new-privileges:true`. The console keeps its sessions in
memory and writes no file, so it mounts no volume.

Sign-in stays off until you name your OpenID Provider. Register the console
there as a confidential client with the redirect URI
`http://127.0.0.1:3000/auth/callback` (or the address you publish), write
its client secret to `secrets/viewer_oidc_client_secret`, and set the
provider in your shell or `.env`:

```sh
FERROBRIDGE_VIEWER__OIDC__ISSUER=https://idp.example.org/realms/ferrobridge
FERROBRIDGE_VIEWER__OIDC__AUTHORIZATION_ENDPOINT=https://idp.example.org/realms/ferrobridge/protocol/openid-connect/auth
FERROBRIDGE_VIEWER__OIDC__TOKEN_ENDPOINT=https://idp.example.org/realms/ferrobridge/protocol/openid-connect/token
FERROBRIDGE_VIEWER__OIDC__JWKS_URI=https://idp.example.org/realms/ferrobridge/protocol/openid-connect/certs
FERROBRIDGE_VIEWER__OIDC__CLIENT_ID=ferrobridge-viewer
FERROBRIDGE_VIEWER__OIDC__CLIENT_SECRET_FILE=/run/secrets/viewer_oidc_client_secret
FERROBRIDGE_VIEWER__OIDC__REDIRECT_URI=http://127.0.0.1:3000/auth/callback
FERROBRIDGE_VIEWER__SESSION__SECURE_COOKIE=false
```

The client secret reaches the console as a compose secret mounted at
`/run/secrets/viewer_oidc_client_secret`, so the value never enters the
environment. `FERROBRIDGE_VIEWER__SESSION__SECURE_COOKIE=false` is for a
console served over plain `http` on loopback, as above, and the console
refuses it for any other redirect URI; behind an `https` reverse proxy, leave
it unset so the session cookie carries `Secure`. The quickstart passes each
of these variables through only when you set it, because an absent issuer is
a console without sign-in while an empty one is a boot error.

The console calls the bridge with the operator's own access token, so the
bridge must validate it: turn on the bridge's `[auth]` section, and
`[operator]` for the mapping files ([The operator API](operator-api.md)).

## The OMOP CDM profiles

Two compose profiles cover the OMOP target, and neither runs on a plain
`docker compose up`.

The `cdm` profile stands up the CDM database on PostgreSQL 18.6, pinned by
digest, with its password read from `secrets/cdm_password` rather than from the
environment, and a `pg_isready` health check that gates the jobs below:

```sh
docker compose --profile cdm up -d cdm
docker compose --profile cdm run --rm cdm-init
```

`cdm-init` runs the bridge image with `cdm init`, which applies the OMOP CDM
v5.4 tables, primary keys and indices to `[cdm] schema` in one transaction,
then creates the bridge schema (`[cdm] bridge_schema`) with its side table,
watermarks and one id sequence per CDM table. Running it again is safe: on a
schema that already holds every CDM table it applies nothing, says the schema
is initialised and exits 0, and on one that holds only some of them it exits
1 naming the missing tables. The OHDSI foreign keys are applied only with
`cdm init --with-constraints`, which PostgreSQL refuses at the pinned OHDSI
tag; the configuration page's `cdm init` section has the three states and the
refused statement.

The `vocab` profile loads an OHDSI Athena vocabulary export. The export is a
licensed download you obtain yourself; it is bind-mounted read only and no
vocabulary byte enters the image, the build context, or a release asset:

```sh
FERROBRIDGE_VOCAB_DIR=/srv/athena/2026-09 \
  docker compose --profile vocab run --rm vocab-load
```

`vocab load DIR --schema NAME` exits 2 today with a line naming #233, the
loader that waits on the observed export format, so that service is the shape
the job runs in rather than a working load. `etl run` is a working job; the image runs it as
`ferrobridge etl run` with the `[etl]` section the configuration page
describes.

## Sources

- distroless: <https://github.com/GoogleContainerTools/distroless>
- The Dockerfile reference: <https://docs.docker.com/reference/dockerfile/>
- The Compose file reference: <https://docs.docker.com/reference/compose-file/>
- Docker packet filtering and firewalls:
  <https://docs.docker.com/engine/network/packet-filtering-firewalls/>
