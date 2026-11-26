<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# Summary

[Introduction](introduction.md)

# Tutorials

- [Your first mapping](tutorials/first-mapping.md)

# How-to guides: integrate

- [Writing and loading mappings](integrate/mappings.md)
- [The FHIR facade](integrate/fhir-facade.md)
  - [Securing the facade](integrate/facade-security.md)
  - [Searching the facade](integrate/facade-search.md)
  - [Consent and access directives](integrate/facade-consent.md)
  - [SMART App State and launch Tasks](integrate/facade-smart.md)
  - [Documents and the patient summary](integrate/facade-documents.md)
  - [The Yellow Button API](integrate/yellow-button.md)
- [The FHIRconnect operations](integrate/operations.md)
- [The OMOP ETL](integrate/omop-etl.md)
  - [Loading FHIR into the CDM with no CDR](integrate/omop-fhir-load.md)
  - [Checking the CDM](integrate/omop-quality.md)
- [The inbound intake](integrate/inbound.md)
  - [Inbound sources](integrate/inbound-sources.md)
- [The change feed](integrate/change-feed.md)
  - [The events lane](integrate/events.md)
  - [The outbound FHIR lane](integrate/fhir-outbound.md)

# How-to guides: deploy and operate

- [What FerroBRIDGE runs beside](operate/deployment-shape.md)
- [The container image](operate/container.md)
- [Named secrets](operate/secrets.md)
- [The state store and high availability](operate/high-availability.md)
- [Upgrading to the state store](operate/upgrade-state-store.md)
- [Running behind FerroSMART](operate/behind-ferrosmart.md)
- [The HL7 v2 face](operate/hl7v2-face.md)
- [The access record](operate/access-record.md)
- [The operator console](operate/operator-console.md)
- [Checking a deployment](operate/deployment-probes.md)
- [The usage report](operate/usage-report.md)

# Reference

- [Configuration](operate/configuration.md)
- [Command-line reference](reference/cli.md)
- [The operator API](operate/operator-api.md)
- [Problem types](operate/problems.md)
- [Pinned versions](evaluate/versions.md)
- [Version lines and releases](operate/versions-and-releases.md)
- [Releases](operate/releases.md)
- [Licensing](evaluate/licensing.md)

# Explanation

- [How FerroBRIDGE maps](explanation/how-ferrobridge-maps.md)
- [Architecture](evaluate/architecture.md)
- [The two mapping specifications](evaluate/mapping-specifications.md)
- [Failure and identity behaviour](operate/failure-and-identity.md)

# Compliance

- [EHDS readiness](evaluate/ehds-readiness.md)
- [The interoperability component](evaluate/interoperability-component.md)
  - [The category crosswalks](evaluate/category-crosswalks.md)
  - [Receiving a document](evaluate/receiving-documents.md)
  - [Prescriptions and dispensations from openEHR](evaluate/prescription-mapping.md)
  - [Laboratory reports from openEHR](evaluate/laboratory-mapping.md)
  - [Imaging reports from openEHR](evaluate/imaging-mapping.md)
  - [Discharge reports from openEHR](evaluate/discharge-mapping.md)
- [CRA information for users](evaluate/cra-user-information.md)

# Contribute

- [How the work is organised](contribute/how-the-work-is-organised.md)
- [Checks and gates](contribute/checks-and-gates.md)
- [The performance gate](contribute/performance-gate.md)
- [Crate versions and the bump rule](contribute/crate-versions.md)
- [Cutting a release](contribute/release.md)

---

[Changelog](changelog.md)
