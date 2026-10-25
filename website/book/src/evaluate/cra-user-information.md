<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# CRA information for users

The Cyber Resilience Act (CRA, Regulation (EU) 2024/2847) has each product
"accompanied by the information and instructions to the user set out in Annex
II, in paper or electronic form" (CRA Art. 13(18)). This page carries that
information for FerroBRIDGE, point by point in the order of Annex II. Each
section quotes what the point asks and gives FerroBRIDGE's answer, or links
the page that already gives it.

Annex II opens: "At minimum, the product with digital elements shall be
accompanied by:". The product is each tagged FerroBRIDGE release, as Cadasto
B.V. publishes it on the
[GitHub release page](https://github.com/FerroHEALTH/FerroBRIDGE/releases):
the `ferrobridge` binary for four Linux targets, the
`ghcr.io/ferrohealth/ferrobridge` container image, the operator console's
`ferrobridge-console` binaries, site bundle and
`ghcr.io/ferrohealth/ferrobridge-console` image, the quickstart
`compose.yaml`, and the library crates on crates.io.

<!-- toc -->

The quotations are from the Official Journal text of the
[CRA](https://eur-lex.europa.eu/eli/reg/2024/2847/oj) at EUR-Lex. No
conformity assessment has been carried out, no EU declaration of conformity
exists and FerroBRIDGE carries no CE marking. This page gives the information
Annex II lists; it does not say that FerroBRIDGE, or a deployment of it, meets
the Regulation.

This book is published from `main` and is not frozen per release, so the
notes of every release link this one page, and it describes the code on
`main`. Where a release differs, its changelog section says how.

The information is written in English. Whether that meets CRA Art. 13(18) in
every Member State is a question the FerroHEALTH book records for counsel
([questions for counsel](https://ferrohealth.eu/docs/cra.html#questions-for-counsel)).

## 1. The manufacturer

"the name, registered trade name or registered trademark of the manufacturer,
and the postal address, the email address or other digital contact as well
as, where available, the website at which the manufacturer can be contacted;"

Cadasto B.V. is the manufacturer of every FerroHEALTH product. Write to
[info@cadasto.com](mailto:info@cadasto.com), or use
<https://www.cadasto.com/contact/>. The postal address and the other contact
details are in the FerroHEALTH book:
[the manufacturer](https://ferrohealth.eu/docs/manufacturer.html#contact-details).

## 2. The single point of contact for vulnerabilities

"the single point of contact where information about vulnerabilities of the
product with digital elements can be reported and received, and where the
manufacturer's policy on coordinated vulnerability disclosure can be found;"

- **Report a vulnerability** through
  [GitHub private vulnerability reporting](https://github.com/FerroHEALTH/FerroBRIDGE/security/advisories/new),
  or by email to [info@cadasto.com](mailto:info@cadasto.com) with
  "FerroBRIDGE vulnerability" in the subject, as
  [`SECURITY.md`](https://github.com/FerroHEALTH/FerroBRIDGE/blob/main/SECURITY.md#reporting-a-vulnerability)
  says. Both routes reach the same people and the same procedure. Do not open
  a public issue, and never put patient data in a report.
- **Receive information about vulnerabilities** from the GitHub security
  advisories of the repository, published with the release that fixes each
  one and readable by machine through
  `GET https://api.github.com/repos/FerroHEALTH/FerroBRIDGE/security-advisories`
  ([security advisories](https://github.com/FerroHEALTH/FerroBRIDGE/blob/main/SECURITY.md#security-advisories)).
  Each release also carries `ferrobridge-vX.Y.Z-vulnerability-record.json`,
  every scanner finding in the images and the server tarballs joined with the
  OpenVEX statement that judges it.
- **The coordinated vulnerability disclosure policy** is the manufacturer's,
  shared by every FerroHEALTH product:
  [security](https://ferrohealth.eu/docs/security.html) in the FerroHEALTH
  book. The response times FerroBRIDGE commits to are in
  [`SECURITY.md`](https://github.com/FerroHEALTH/FerroBRIDGE/blob/main/SECURITY.md#response-times).

## 3. Name, type and unique identification

"name and type and any additional information enabling the unique
identification of the product with digital elements;"

- **Name:** FerroBRIDGE.
- **Type:** software. A server that maps clinical data between an openEHR
  CDR and HL7 FHIR R4 or an OMOP CDM v5.4 database, with its browser
  operator console.
- **Version:** the release tag `vX.Y.Z`. `ferrobridge --version` prints it,
  and the FHIR facade names it as `software.version` in the
  `CapabilityStatement` at `GET /fhir/metadata`
  ([version lines and releases](../operate/versions-and-releases.md)).
- **The artefacts of a version:** each release tarball carries a checksum and
  a signed build provenance, and the images carry the version as their tag and
  are identified by digest
  ([verifying a release](https://github.com/FerroHEALTH/FerroBRIDGE/blob/main/SECURITY.md#verifying-a-release)).
  A library crate carries its own version, independent of the product's
  ([the crate line](../operate/versions-and-releases.md#the-crate-line)).
- **Release date:** the date on the release's heading in the
  [changelog](https://github.com/FerroHEALTH/FerroBRIDGE/blob/main/CHANGELOG.md).

## 4. Intended purpose, security environment, essential functionalities and security properties

"the intended purpose of the product with digital elements, including the
security environment provided by the manufacturer, as well as the product's
essential functionalities and information about the security properties;"

- **Intended purpose and essential functionalities:** FerroBRIDGE runs as
  its own server beside an openEHR CDR it reaches over openEHR ITS-REST
  1.1.0. Its FHIR facade serves the CDR's compositions as FHIR R4 resources
  and writes FHIR resources into the CDR, driven by FHIRconnect mappings; its
  OMOP ETL reads compositions over AQL and writes them into an OMOP CDM v5.4
  database, driven by OMOCL mappings; its HL7 v2 face writes HL7 v2 messages
  into the CDR through the facade
  ([introduction](../introduction.md);
  [what FerroBRIDGE runs beside](../operate/deployment-shape.md)).
- **Security environment:** what FerroBRIDGE relies on and does not provide
  itself. The server speaks plain HTTP on its listen address and the HL7 v2
  face plain MLLP, so a reverse proxy or the network terminates TLS and
  limits who reaches them. An authorization server issues the access tokens
  `[auth]` checks. The CDR, the terminology server and the CDM database are
  yours, each reached with the credentials you configure. The disks that hold
  the identity store and the mapping files are yours to protect
  ([the container image](../operate/container.md#running-it-with-compose);
  [what FerroBRIDGE stores](../operate/deployment-shape.md#what-ferrobridge-stores)).
- **Security properties:** the ones that ship with every release.
  - `[auth]` is on by default: the facade, the FHIRconnect operations and the
    operator API refuse to start until it names an issuer and an audience,
    and every request then needs a valid SMART App Launch 2.x access token or
    a configured Basic credential. A route under `/fhir` the guard does not
    classify is refused with `403`
    ([the FHIR facade, security](../integrate/fhir-facade.md#security)).
  - The request log carries the matched route and never a body, and a query
    parameter value reaches it only when you name the parameter
    ([`[telemetry]`](../operate/configuration.md#telemetry)).
  - Every request has a timeout and a body ceiling
    ([`[server]`](../operate/configuration.md#server)).
  - Every secret can be read from a file, so it never shows in the
    environment ([the secrets](../operate/configuration.md#the-secrets-in-one-place)).
  - A CDM connection that could fall back to plaintext without saying so is
    refused at load ([`[cdm]`](../operate/configuration.md#cdm)).
  - Both images run as a non-root user on a distroless base with no shell,
    and the quickstart runs them with a read-only root filesystem and every
    capability dropped ([the container image](../operate/container.md)).
  - A mapping set is validated when it loads, and `ferrobridge mapping check`
    runs the same validation before you deploy it
    ([writing and loading mappings](../integrate/mappings.md#checking-a-set-before-you-deploy-it)).

## 5. Circumstances that may lead to significant cybersecurity risks

"any known or foreseeable circumstance, related to the use of the product with
digital elements in accordance with its intended purpose or under conditions
of reasonably foreseeable misuse, which may lead to significant cybersecurity
risks;"

The ones to know before you deploy:

- **`[auth] enabled = false`** serves the facade and the operations to every
  caller that reaches them, reads and writes of patient data included. The
  server logs a warning at start-up when it is off
  ([`[auth]`](../operate/configuration.md#auth)).
- **A published port on every interface.** The quickstart binds `127.0.0.1`;
  a port published on `0.0.0.0` is reachable from the network whatever the
  host firewall says
  ([running it with compose](../operate/container.md#running-it-with-compose)).
- **The HL7 v2 face checks no credential.** Its allow list matches MSH-4
  alone, a value the sender writes itself, so keep the MLLP port on a network
  only your senders reach ([limits](../operate/hl7v2-face.md#limits)).
- **A lost or shared identity store.** A lost file makes the next create of
  a resource the CDR already holds write a second composition, and two
  processes with a file each keep two maps that disagree
  ([the identity store](../operate/container.md#the-identity-store)).
- **The operator scope rewrites mappings.** With `[operator] mapping_dir` set,
  a token that carries `[auth] product_scope` can save and reload the
  mappings that decide what the facade writes into the CDR
  ([the operator API](../operate/operator-api.md)).
- **`sslmode=disable` on the CDM URL** sends the clinical rows the ETL writes
  in plaintext; use it only on a trusted local network
  ([`[cdm]`](../operate/configuration.md#cdm)).

## 6. The EU declaration of conformity

"where applicable, the internet address at which the EU declaration of
conformity can be accessed;"

No EU declaration of conformity has been drawn up, so there is no address to
give. The obligation applies from 11 December 2027 (CRA Art. 71(2)). The
FerroHEALTH book records the conformity route of FerroBRIDGE together with
FerroEHR
([how conformity is assessed](https://ferrohealth.eu/docs/cra.html#how-conformity-is-assessed-product-by-product)).
When the declaration exists, it is published with each release it covers,
and this section gives its address.

## 7. Technical security support and the end of the support period

"the type of technical security support offered by the manufacturer and the
end-date of the support period during which users can expect vulnerabilities
to be handled and to receive security updates;"

- **Only the newest release is supported.** When a new release is published,
  every earlier release stops receiving fixes: there are no maintenance
  branches, no long-term-support line and no backports. Upgrading is free.
- **The end date:** Cadasto B.V. keeps publishing security fixes for at least
  five years after each release (CRA Art. 13(8)) and ships them only in the
  newest release (Art. 13(10)). The notes of each release state the month
  and year that commitment ends for it, five years after the date on its
  changelog heading (Art. 13(19)). A release itself is supported only while
  it is the newest.
- **The type of support:** the manufacturer's, the same for every FerroHEALTH
  product ([how a fix reaches you](https://ferrohealth.eu/docs/security.html#how-a-fix-reaches-you)),
  with FerroBRIDGE's supported artefacts in
  [`SECURITY.md` § Supported versions](https://github.com/FerroHEALTH/FerroBRIDGE/blob/main/SECURITY.md#supported-versions).

## 8. Detailed instructions

"detailed instructions or an internet address referring to such detailed
instructions and information on:"

### 8(a) Secure use from commissioning on

"the necessary measures during initial commissioning and throughout the
lifetime of the product with digital elements to ensure its secure use;"

- **Commissioning:** verify what you downloaded with `gh attestation verify`
  ([verifying a release](https://github.com/FerroHEALTH/FerroBRIDGE/blob/main/SECURITY.md#verifying-a-release)),
  then install the image or the binary
  ([the container image](../operate/container.md)). Keep the published ports
  on loopback or a named interface behind a reverse proxy that terminates
  TLS. Name your authorization server in `[auth]` before you turn on the
  facade, the operations or the operator API, and pass every secret through
  its `_file` sibling ([configuring the server](../operate/configuration.md)).
  Put the identity store on a persistent volume and back it up with the CDR
  ([the identity store](../operate/container.md#the-identity-store)). Connect
  to the CDM database with `sslmode=verify-full`
  ([`[cdm]`](../operate/configuration.md#cdm)). Check the mapping set with
  `ferrobridge mapping check` and the deployment with the probes
  ([checking a deployment](../operate/deployment-probes.md)).
- **Throughout its lifetime:** follow the release feed and the security
  advisories, upgrade to each new release, and run `mapping check` on every
  mapping change before you load it.

### 8(b) How changes to the product can affect the security of data

"how changes to the product with digital elements can affect the security of
data;"

- **Configuration.** `[auth] enabled = false` removes the credential check
  from the facade and the operations. Adding a user to `[[auth.basic.users]]`
  or a subject to `scopeless_subjects` grants access to patient data.
  `[operator] mapping_dir` lets an operator-scope token change the mappings.
  `[hl7v2] enabled` opens an MLLP listener that writes into the CDR. A name in
  `[telemetry] logged_query_parameters` lets that parameter's value reach the
  log, and `[facade.references] fetch_bases` lets the facade fetch from the
  bases it lists ([configuring the server](../operate/configuration.md)).
- **Mappings.** A mapping decides which data crosses between the CDR and the
  target, so a changed mapping changes what the facade serves and writes and
  what the ETL copies into the CDM database.
- **Outbound data.** The usage report sends what
  [the usage report](../operate/usage-report.md#what-a-report-contains) lists,
  and `FERROBRIDGE__USAGE_REPORT__ENABLED=false` stops it
  ([switch it off](../operate/usage-report.md#switch-it-off)).
- **Upgrades.** An upgrade replaces the binary or the image; the identity
  store, the mapping files and the configuration stay where you keep them.
  The release's changelog section says what changed.
- **Changes to the source.** A binary or image built from modified source is
  not a Cadasto B.V. release: its build provenance does not verify.

### 8(c) Installing security-relevant updates

"how security-relevant updates can be installed;"

Subscribe to the release feed
(`https://github.com/FerroHEALTH/FerroBRIDGE/releases.atom`) and poll the
security advisories; the server does not look for newer releases itself.
Verify the new release
([verifying a release](https://github.com/FerroHEALTH/FerroBRIDGE/blob/main/SECURITY.md#verifying-a-release)),
then replace the binary or deploy the new image by digest and restart. On
`SIGTERM` the server drains its requests for up to
`[server] shutdown_timeout_ms` before it exits. Run one process per identity
store, so on Kubernetes use the `Recreate` strategy
([the identity store](../operate/container.md#the-identity-store)).

### 8(d) Secure decommissioning, and removing user data

"the secure decommissioning of the product with digital elements, including
information on how user data can be securely removed;"

FerroBRIDGE has no erase command. Clinical data lives in the CDR and in the
CDM database, which you run; what FerroBRIDGE keeps of its own is in files,
directories and schemas you name. To take a deployment out of service and
remove its data, stop the server, the console and every ETL job, then
delete:

- the file `[facade] identity_store` names, which maps subject identifiers to
  `ehr_id`s and FHIR ids to compositions;
- the directory `[usage_report] state_dir` names, which holds the usage
  report's instance id;
- the mapping directory, `[operator] mapping_dir` if you set it, and the
  templates directory;
- in the CDM database, the schema `[cdm] bridge_schema` names, with the side
  table, the watermarks and the id sequences, and the CDM schema itself if
  the data the ETL wrote there should go too;
- the configuration file, the secret files, the Basic users' password hash
  files, and the console's configuration and client secret.

The console keeps its sessions in memory, so stopping it ends them
([sessions](../operate/operator-console.md#sessions)). Remove the copies
FerroBRIDGE cannot reach yourself: backups, volume snapshots, proxy and log
collector records, and the data held by the CDR and the clients the facade
served. Sanitising the physical media is yours. To have the usage reports
already sent erased, follow
[your right to object](../operate/usage-report.md#your-right-to-object).

### 8(e) Turning off automatic installation of security updates

"how the default setting enabling the automatic installation of security
updates, as required by Part I, point (2)(c), of Annex I, can be turned off;"

FerroBRIDGE installs no update by itself, so there is no such setting to turn
off. The server reaches the manufacturer only through the usage report, reads
nothing of the collector's answer but its status code, and nothing the
collector answers changes its configuration or behaviour
([network](../operate/usage-report.md#network)). You install each
update as 8(c) describes.

### 8(f) Information for an integrator

"where the product with digital elements is intended for integration into
other products with digital elements, the information necessary for the
integrator to comply with the essential cybersecurity requirements set out in
Annex I and the documentation requirements set out in Annex VII."

The server, the console and their images are not intended for integration
into another product; other software calls a running server over the FHIR
API, the FHIRconnect operations or MLLP. The library crates on crates.io are
intended for integration, and each is a product of its own: each crate's
`README.md` and docs.rs page describe its API, and
[version lines and releases](../operate/versions-and-releases.md#the-crate-line)
says which crates are published and which are name reservations.

## 9. The software bill of materials

"If the manufacturer decides to make available the software bill of materials
to the user, information on where the software bill of materials can be
accessed."

Cadasto B.V. makes the software bills of materials available. Each release
publishes, beside each `ferrobridge` tarball, a CycloneDX SBOM of the source
dependency graph (`.cdx.json`) and an SPDX SBOM read from the shipped binary
(`.spdx.json`), both attested; an SPDX SBOM beside each console tarball; and
an SPDX SBOM attestation on each platform manifest of both images. The
binaries also embed their dependency lists through `cargo auditable`, which
`syft`, `trivy`, `grype` and `osv-scanner` read directly. How to fetch and
verify each one is in
[verifying a release](https://github.com/FerroHEALTH/FerroBRIDGE/blob/main/SECURITY.md#verifying-a-release).
