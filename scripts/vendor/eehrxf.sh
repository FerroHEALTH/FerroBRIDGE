#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
# scripts/vendor/eehrxf.sh
#
# Vendors the corpora the eehrxf crate's tests read: the published proxies
# for the European electronic health record exchange format of Regulation
# (EU) 2025/327 Article 15(1), until its implementing act is adopted, and the
# FHIR R4 pages its extension definition is checked against. The proxies are
# the Xt-EHR EHDS Logical Information Models and the HL7 Europe FHIR
# implementation guides (Base and Core, Patient Summary, Medication
# Prescription and Dispense, Laboratory Report, Imaging Report, Hospital
# Discharge Report), each at the one version the component reads, with the
# MyHealth@EU Master Value Sets Catalogue and HL7 France FR Core, whose value
# sets the terminology tests read. One corpus per package, each under
# docs/specs/.
#
# Each package is the FHIR package registry's tarball (Simplifier's for FR
# Core) and each page the
# hl7.org/fhir/R4 page, pinned by URL and sha256 (scripts/vendor/lib/pinned.sh)
# and committed whole, so its sha256 stays the pin; each corpus row of
# docs/VERSIONS.md carries its pin-set digest. The component reads a package
# as the archive the registry serves, so these are kept as tarballs beside
# the unpacked trees scripts/vendor/profile-packages.sh fetches for the
# profile validator. Every package declares CC0-1.0 in its manifest, and
# FHIR R4 licenses its pages under CC0-1.0.
#
# The corpora came from FerroFED with the crate (#1314), at the same pins
# and the same bytes.
#
# Usage:
#   scripts/vendor/eehrxf.sh
#   PINNED_DIGESTS_ONLY=1 scripts/vendor/eehrxf.sh   # print each pin-set digest
#
# Requires: curl, shasum, awk.
#
# No specification governs this file; it is FerroBRIDGE's own design.

set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$root"

# shellcheck source=scripts/vendor/lib/corpus.sh
# shellcheck disable=SC1091 # shellcheck is not run with -x; the library is checked on its own
. "$root/scripts/vendor/lib/corpus.sh"
# shellcheck source=scripts/vendor/lib/pinned.sh
# shellcheck disable=SC1091 # shellcheck is not run with -x; the library is checked on its own
. "$root/scripts/vendor/lib/pinned.sh"

corpus_require curl shasum awk
pinned_begin

registry=https://packages.fhir.org
cc0='CC0-1.0: the license of the package manifest and of the ImplementationGuide resource'
whole='The registry tarball as served, package manifest included; the archive is committed whole so its sha256 stays the pin.'
read_by=$'#1314 (the eehrxf tests, crates/eehrxf/tests/it/, which read\n  the exchange-format proxies the component is designed against)'

pin commit xtehr.eu.ehds.models-1.0.0.tgz "1.0.0 (FHIR 5.0.0, 2026-04-13)" \
  "$registry/xtehr.eu.ehds.models/1.0.0" \
  a4853e58a869468464847e2eafa50fb07349e6da1e99456a0d93aae27848e1f8 "$cc0" "$whole"
pinned_corpus eu-xtehr-models "Xt-EHR EHDS Logical Information Models" \
  "the Xt-EHR EHDS Logical Information Models xtehr.eu.ehds.models" \
  "The logical models of the Xt-EHR joint action for the priority categories
of Regulation (EU) 2025/327 Article 14(1), canonical
\`http://www.xt-ehr.eu/fhir/models\`, committed under CC0-1.0." "" "$read_by"

pin commit hl7.fhir.eu.base-2.0.1.tgz "2.0.1 (FHIR 4.0.1, 2026-09-13)" \
  "$registry/hl7.fhir.eu.base/2.0.1" \
  3fb23b64d70656ea809e1ad66d84d5d008400e18424266debf176cb46fba0b4a "$cc0" "$whole"
pinned_corpus eu-hl7-base "HL7 Europe Base and Core" \
  "the HL7 Europe Base and Core FHIR IG hl7.fhir.eu.base" \
  "The European base and core profiles the other HL7 Europe guides build on,
canonical \`http://hl7.eu/fhir/base\`, committed under CC0-1.0." "" "$read_by"

pin commit hl7.fhir.eu.eps-1.0.0-ballot.tgz "1.0.0-ballot (FHIR 4.0.1, 2026-06-06)" \
  "$registry/hl7.fhir.eu.eps/1.0.0-ballot" \
  e0fff1fb20d3daf75609259faa6b860131ebf4fc8890bb68f0da882307afc33c "$cc0" \
  "$whole The only version on the registry."
pinned_corpus eu-hl7-eps "HL7 Europe Patient Summary" \
  "the HL7 Europe Patient Summary hl7.fhir.eu.eps" \
  "The European Patient Summary guide in its ballot version, canonical
\`http://hl7.eu/fhir/eps\`, committed under CC0-1.0." "" "$read_by"

pin commit hl7.fhir.eu.mpd-1.0.0.tgz "1.0.0 (FHIR 4.0.1, 2026-05-11)" \
  "$registry/hl7.fhir.eu.mpd/1.0.0" \
  f1bc1084efc93e16ae4b45c1bb71340385d314a72031949c4a7e0d97aea5fd6b "$cc0" "$whole"
pinned_corpus eu-hl7-mpd "HL7 Europe Medication Prescription and Dispense" \
  "the HL7 Europe Medication Prescription and Dispense guide hl7.fhir.eu.mpd" \
  "The European ePrescription and eDispensation guide, canonical
\`http://hl7.eu/fhir/mpd\`, committed under CC0-1.0." "" "$read_by"

pin commit hl7.fhir.eu.laboratory-2.0.0.tgz "2.0.0 (FHIR 4.0.1, 2026-05-05)" \
  "$registry/hl7.fhir.eu.laboratory/2.0.0" \
  097aa45c6efcdbc3f25ead8b7d448fee5011b5e7ecf264aa400c5e2c69c5c488 "$cc0" \
  "$whole The version MyHealth@EU Laboratory 9.1.1 derives from, which the component reads."
pinned_corpus eu-hl7-laboratory "HL7 Europe Laboratory Report" \
  "the HL7 Europe Laboratory Report guide hl7.fhir.eu.laboratory" \
  "The European laboratory report guide, canonical
\`http://hl7.eu/fhir/laboratory\`, at the version the eehrxf component reads,
committed under CC0-1.0." "" "$read_by"

pin commit hl7.fhir.eu.imaging-1.0.0-ballot.tgz "1.0.0-ballot (FHIR 4.0.1, 2026-03-16)" \
  "$registry/hl7.fhir.eu.imaging/1.0.0-ballot" \
  bb6012bd7a019ac82f5e939dd6e892e6db6b15fd2931dcf7cc445c1ad92c3d70 "$cc0" \
  "$whole Its Composition and DiagnosticReport profiles require the priority category code Medical-Imaging."
pinned_corpus eu-hl7-imaging "HL7 Europe Imaging Report" \
  "the HL7 Europe Imaging Report R4 guide hl7.fhir.eu.imaging" \
  "The European imaging report guide in its ballot version, canonical
\`http://hl7.eu/fhir/imaging\`, committed under CC0-1.0." "" "$read_by"

pin commit hl7.fhir.eu.hdr-1.0.0.tgz "1.0.0 (FHIR 4.0.1, 2026-10-16)" \
  "$registry/hl7.fhir.eu.hdr/1.0.0" \
  eedf9af9f66b68cd66f7d6c678a36dfba86b0920df4ef15e8d2da900c4134879 "$cc0" \
  "$whole The first release."
pinned_corpus eu-hl7-hdr "HL7 Europe Hospital Discharge Report" \
  "the HL7 Europe Hospital Discharge Report guide hl7.fhir.eu.hdr" \
  "The European hospital discharge report guide at its first release,
canonical \`http://hl7.eu/fhir/hdr\`, committed under CC0-1.0." "" "$read_by"

pin commit myhealth.eu.fhir.mvc-package-9.1.0.tgz "9.1.0 (FHIR 4.0.1, 2026-05-08)" \
  "$registry/myhealth.eu.fhir.mvc-package/9.1.0" \
  3f99350f36a2a1ea17b7fde37c39ff766bd5c3bc9a29c6183af1acfd07b78ce1 "$cc0" \
  "$whole Its value sets name the preferred system URI of each terminology the component writes."
pinned_corpus ehdsi-mvc "MyHealth@EU Master Value Sets Catalogue" \
  "the MyHealth@EU Master Value Sets Catalogue myhealth.eu.fhir.mvc-package" \
  "The value sets MyHealth@EU exchanges are coded with, canonical
\`http://fhir.ehdsi.eu/mvc-package\`, committed under CC0-1.0." "" "$read_by"

pin commit hl7.fhir.fr.core-2.2.0.tgz "2.2.0" \
  https://packages.simplifier.net/hl7.fhir.fr.core/2.2.0 \
  085e61bf579829a6d5520c6f388d763d27ba400f9f6ca548bc88da5545ba13f9 \
  'CC0-1.0: the license of the package manifest' \
  "$whole Its value sets include SNOMED CT at a version, the form the component's version check reads."
pinned_corpus fr-ans "HL7 France FR Core" \
  "the HL7 France FR Core guide hl7.fhir.fr.core" \
  "FR Core 2.2.0 as Simplifier serves it, whose value sets include SNOMED CT
at an edition and release, committed under CC0-1.0." "" "$read_by"

r4=https://hl7.org/fhir/R4
r4cc0='CC0-1.0: FHIR R4 2.20.2 (license.html) licenses the specification under Creative Commons No Rights Reserved (CC0)'
read_by_r4=$'#1314 (the remaining-dispensations extension definition of\n  crates/eehrxf/src/medication/, checked by\n  crates/eehrxf/tests/it/medication/extension.rs)'

pin commit extensibility.html "4.0.1, 2.5.0 Extensibility" "$r4/extensibility.html" \
  3c1df2fd19b10d5f7f132976dea0691e42addfaafd0bd10295ccf8ec51269eb3 "$r4cc0"
pin commit defining-extensions.html "4.0.1, 2.5.1 Defining Extensions" "$r4/defining-extensions.html" \
  dfdfa215d56375ced5f69d10ec752a33ad3088f2c4885823f01b8e18e7a21021 "$r4cc0"
pin commit structuredefinition.html "4.0.1, the StructureDefinition resource" "$r4/structuredefinition.html" \
  35cd9ecf45464d7f0adbc189ddd014e8641b860e4db91d1eebacc971be43c596 "$r4cc0"
pin commit license.html "4.0.1, 2.20 License and Legal Terms" "$r4/license.html" \
  b2aeb02247ca2aa855ed1fce29f62600aeeb8dcc8257ec1e0b40ceaf4fd60a15 "$r4cc0" \
  "The licence statement of the pages beside it, vendored as their LICENSE."
pinned_corpus fhir-r4 "FHIR R4 core specification pages" \
  "the FHIR R4 core specification pages" \
  "The Extensibility (2.5.0), Defining Extensions (2.5.1), StructureDefinition
and License and Legal Terms (2.20) pages of HL7 FHIR Release 4, version
4.0.1, as \`hl7.org/fhir/R4/\` serves them, committed under CC0-1.0, which the
license page states for the specification." "" "$read_by_r4"

say "done"
