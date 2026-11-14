#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
# scripts/vendor/consent.sh
#
# Vendors the texts the access directive exchange (`consent`, #1933) is
# checked against into four corpora under docs/specs/:
#
# - fhir-consent-r4/ and fhir-consent-r5/: the HL7 FHIR `Consent` resource of
#   R4 (4.0.1) and R5 (5.0.0): the resource page, its formal definitions, its
#   StructureDefinition, the value sets its coded elements bind, the examples
#   index, every published Consent example in JSON and the licence page;
# - ihe-consents/: IHE ITI TF-1 chapter 19 and ITI TF-3 section 5.1 (BPPC),
#   the APPC supplement Rev. 1.4 (ITI TF-1 chapter 43 and ITI TF-3 section
#   5.6), and the copyright licence chapter of the IHE General Introduction;
# - xacml/: the OASIS XACML 2.0 core specification, the language APPC
#   Privacy Consent Documents are written in (APPC TF-3 5.6.1).
#
# Each artefact is pinned by its URL and the sha256 of its bytes
# (scripts/vendor/lib/pinned.sh); the files are the ones FerroHEALTH/FerroEHR
# vendored for the same mapping, identified by the same sha256. The four
# rows of docs/VERSIONS.md carry the pin-set digests.
#
# Usage:
#   scripts/vendor/consent.sh
#
# Requires: curl, shasum, awk.

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

read_by="FerroHEALTH/FerroBRIDGE#1933 (app/ferrobridge-server/src/consent and its tests)"

fhir=https://hl7.org/fhir
cc0='Creative Commons "No Rights Reserved" (CC0 1.0), as the license.html page of the release states; HL7 keeps the "FHIR" trademark, which this copy does not use as a mark'

# pin_fhir RELEASE VERSION FILE SHA256
pin_fhir() {
  pin commit "$3" "FHIR $1 ($2)" "$fhir/$1/$3" "$4" "$cc0"
}

pin_fhir R4 4.0.1 consent.html 438c1e3de0759274f2196fa3bb3efd0f56f812543b3a4c7547e86b42f26c6d2c
pin_fhir R4 4.0.1 consent-definitions.html 9ddabeb5c4821b9e923dd767aae1106e9daa3a81a56112f9de9be68d9b42507e
pin_fhir R4 4.0.1 consent.profile.json c4d00dcd93cfc55242c315f72dfe5a4f25f4a590f162bf20ccad16145972b02d
pin_fhir R4 4.0.1 consent-examples.html afcb4440e95d71c310e52bd65d3306e1bc8f332d31cce40cd204dc4fdb5b4c47
pin_fhir R4 4.0.1 license.html b2aeb02247ca2aa855ed1fce29f62600aeeb8dcc8257ec1e0b40ceaf4fd60a15
pin_fhir R4 4.0.1 valueset-consent-scope.html e0ed4a9ceb5c3a3e8b08c5d01fafe7927af5f5b530e90bfe1dff1cb986c3eb9e
pin_fhir R4 4.0.1 valueset-consent-category.html abef5c333602abd495aea3c2f6b9bf0af41138a1f3edf2ff2cc849522e86c809
pin_fhir R4 4.0.1 valueset-consent-state-codes.html 28d193618ffaa5cc1651e5ce8a4ccece7e3eb71fff2e001e373a516aa8e2f75c
pin_fhir R4 4.0.1 valueset-consent-provision-type.html 5690d59a40c30335f2669da6790a730c283859a69b3ddf365f4ab33c799ca9cb
pin_fhir R4 4.0.1 valueset-consent-data-meaning.html 24c717e06ab1e6f46b3335b91a6029383ba44eb19d2742b99a5506afdf76bbf0
pin_fhir R4 4.0.1 valueset-consent-action.html 517798619f2f9b72b391b938ae499cdecfd14585cf25f8e6a6fa180215769ed5
pin_fhir R4 4.0.1 valueset-consent-policy.html 8106d77f42fbaea38af9ee15a7b7b38a4e77e92318507e162fae9bc1da9d3841
pin_fhir R4 4.0.1 consent-example.json 464abff4f0bda68b6247e17acb5bad76ab9d1701e20b68357b67bdace62c6924
pin_fhir R4 4.0.1 consent-example-Emergency.json f13c1ff607d9b120f0b18cb8f9c880e63be55c0df18965d4d8db554218a06868
pin_fhir R4 4.0.1 consent-example-Out.json eaf2721c7e37e664aca46ea327c504e62213b87845668c978eec5a5535122d2e
pin_fhir R4 4.0.1 consent-example-grantor.json 7e5fd4946f33c84555f8f7878623bfd7bc65efd5e3da29f3aad324c213fa4573
pin_fhir R4 4.0.1 consent-example-notAuthor.json b3be9411b1fe9ec38a82bbdcf075076abb9c4053c746088328b472dc39b2f00d
pin_fhir R4 4.0.1 consent-example-notOrg.json d5ce88400d638badd7c9eb2bb63d637644499750cf02343c0cdce71a6f68c24d
pin_fhir R4 4.0.1 consent-example-notThem.json c1fe61d49df3053ac0e1e074d0d2b86b79dff5a9eb797b46eeab1f8c78ea77fb
pin_fhir R4 4.0.1 consent-example-notThis.json e1ac7fa8868d4928ed596ebe7a39ad83697f878ed04c15e12d5e9df43397dff7
pin_fhir R4 4.0.1 consent-example-notTime.json f29b02ce05901311a4112aae201384d62307489e26ce38dff9602a55c1fdebe8
pin_fhir R4 4.0.1 consent-example-pkb.json 3369a161d5e0e819335c13cf538d60dde6cc3d2193c31592c405dfa63754dfd4
pin_fhir R4 4.0.1 consent-example-signature.json 90da67cb25f3b7dadbaee177e9d1af6fda6f7ab1f08eaeac649ae85a989abf42
pin_fhir R4 4.0.1 consent-example-smartonfhir.json 0834eccf9de9b717d3cba6b807090fe9420946e5a60aa7b1f7c64c2deb2414ea
pinned_corpus fhir-consent-r4 "FHIR R4 Consent" \
  "the FHIR R4 Consent resource" \
  "The \`Consent\` resource of HL7 FHIR R4 (4.0.1), each file whole as the
specification serves it, without the stylesheets, scripts and images the
pages link: the resource page (scope, boundaries, the element table, the
constraints), its formal definitions, its StructureDefinition, the value set
pages of the bindings the mapping reads (\`scope\`, \`category\`, \`status\`,
\`provision.type\`, \`provision.data.meaning\`, \`provision.action\` and the
policy codes of \`policyRule\`), the examples index, every published Consent
example in JSON, and the licence page that grants the copy. The XML and
Turtle forms of the examples are not taken." "" "$read_by"

pin_fhir R5 5.0.0 consent.html 9cefa81d1a6885d0d466330104d0441b88e5e51fc5bd011a03be2c9ddf8ea25b
pin_fhir R5 5.0.0 consent-definitions.html 7c42c4128cf540f3691ff4711a06309911e53ba2bd6fbffab38ccafd53f3b747
pin_fhir R5 5.0.0 consent.profile.json d647d01f64c6c3015b2efd85ad0f3f5d63f484cb9ee746e273f4fd88ae55c2ec
pin_fhir R5 5.0.0 consent-examples.html 362cbe8dc01644ae36731283c221087cf09ad7e059df53a03b1514f76e37dfa9
pin_fhir R5 5.0.0 license.html 452f8651729bb0e6b93c75a22f772cb7ece300a3585e1da17be944964b44aa90
pin_fhir R5 5.0.0 valueset-consent-category.html 64013a47b5341cc90658d6e87871ea0f8a91e49666161da0f9e0b239186d0df0
pin_fhir R5 5.0.0 valueset-consent-state-codes.html d46d4b1e2b0af90acc671504e82d364ccf6797e0c348f100254557a75b77fac1
pin_fhir R5 5.0.0 valueset-consent-provision-type.html bd78dee7d35db1aa5e92d7d5f7df67978e3b431f56761081b4a004386d38ff18
pin_fhir R5 5.0.0 valueset-consent-data-meaning.html b39a1731d2fde3f5bbc8cb9aa7bb18d7fb5d867cbcb791137707aaf5b462101e
pin_fhir R5 5.0.0 valueset-consent-action.html d1f017ed931997a9a0841cdef307fbe698458ec581ec416095677690ab89afa1
pin_fhir R5 5.0.0 valueset-consent-policy.html aac412ed8b2398375bb20661b05724a28dab67f0008072ba6ceb76c1893648ae
pin_fhir R5 5.0.0 consent-example.json d96a58b9322ddab78a09ac36a0cfc72a8b82240c5da9f137c53c3ca786b7326c
pin_fhir R5 5.0.0 consent-example-CDA.json 7d1e951f65b73cec42f11c93f1d01b35f585d8a0e1b0115dde6ac4f11698fc2a
pin_fhir R5 5.0.0 consent-example-Emergency.json 71c2a15953b96b250df93e01edee963de568d7ce917785f6221a125791f994e8
pin_fhir R5 5.0.0 consent-example-Out.json ae16b1c3abeeafde2d53c91f06f6cbb57a498c5dbaa072a50ab9f620be8c0332
pin_fhir R5 5.0.0 consent-example-grantor.json da308f4d572097f91adeae4ca050f1b1fc05c52c2bdf26cbab37224016bc92bf
pin_fhir R5 5.0.0 consent-example-notAuthor.json f3dcd222b3844598fcf01f30e94a7e14d2c1db78af8268baf1a62bba6ee5e112
pin_fhir R5 5.0.0 consent-example-notOrg.json 31c897a58f9f4ab34a57ada0eb2f69a4fec6b872a37f01c0f6e9e951c24d0ddb
pin_fhir R5 5.0.0 consent-example-notThem.json 112b527688c76cbad2374c256fd2078937937dc812eea954e353df3a74a74687
pin_fhir R5 5.0.0 consent-example-notThis.json 9d58d17649601d7dcfb84666d195f5cfd65c04b30c7911679189e80abf4492b3
pin_fhir R5 5.0.0 consent-example-notTime.json 8544d313e37f1fb146e1010bcd1e77257b4f346e527b53aeb944b7b31a6c4f6f
pin_fhir R5 5.0.0 consent-example-pkb.json 1f0add6432a0507fec7bfadeab74b7ee18ef776482e160270e5be0e805bb746f
pin_fhir R5 5.0.0 consent-example-smartonfhir.json b2dc4e962faa388dbcd5debdfd7d15e210079eee79be7ab99da5dee75f059ef0
pinned_corpus fhir-consent-r5 "FHIR R5 Consent" \
  "the FHIR R5 Consent resource" \
  "The \`Consent\` resource of HL7 FHIR R5 (5.0.0), each file whole as the
specification serves it, without the stylesheets, scripts and images the
pages link: the resource page, its formal definitions, its
StructureDefinition, the value set pages of the bindings the mapping reads
(\`category\`, \`status\`, \`decision\`, \`provision.data.meaning\`,
\`provision.action\` and the policy codes of \`regulatoryBasis\`), the
examples index, every published Consent example in JSON, and the licence
page that grants the copy." "" "$read_by"

ihe='IHE Technical Frameworks General Introduction, chapter 9 (vendored here as ch-9.html): "an irrevocable, worldwide, perpetual, royalty-free, nontransferable, nonexclusive, non-sublicensable license under its copyrights in any IHE profiles and Technical Framework documents ... to reproduce and distribute" them'
pin commit ch-19.html "ITI TF-1 chapter 19, Volume Revision 20.0, August 4, 2023, Final Text" \
  https://profiles.ihe.net/ITI/TF/Volume1/ch-19.html \
  a35ecb229ef67302051e92b13f3cd59ee12e67bd9d463b7166f720b6b055455f "$ihe" \
  "BPPC, the profile: policy identifiers, actors, options, process flow"
pin commit ch-5.1.html "ITI TF-3 section 5.1, Volume Revision 20.0, August 4, 2023, Final Text" \
  https://profiles.ihe.net/ITI/TF/Volume3/ch-5.1.html \
  82d220535db84971ed0162a8674e384938d7ae944a82cb217200391933c5b7b8 "$ihe" \
  "BPPC, the Patient Privacy Consent Acknowledgment Document content module"
pin commit ch-9.html "IHE Technical Frameworks General Introduction, chapter 9" \
  https://profiles.ihe.net/GeneralIntro/ch-9.html \
  2056d5b878f1bee7f3881cada792ec352476613b495afc677b9a467b9d2197cf "$ihe" \
  "Copyright Licenses, the grant this copy rests on"
pin commit IHE_ITI_Suppl_APPC.pdf "APPC supplement Rev. 1.4, Trial Implementation, July 2, 2021" \
  https://www.ihe.net/uploadedFiles/Documents/ITI/IHE_ITI_Suppl_APPC.pdf \
  823bd5e015ddfb384a19f80ed93e9d1946740393a4be057fd8c9ab6b01936282 "$ihe" \
  "APPC, ITI TF-1 chapter 43 and ITI TF-3 section 5.6, the Privacy Consent Document content module"
pinned_corpus ihe-consents "IHE BPPC and APPC" \
  "IHE Basic and Advanced Patient Privacy Consents" \
  "IHE Basic Patient Privacy Consents (BPPC) as the HTML IT Infrastructure
Technical Framework publishes it, and the Advanced Patient Privacy Consents
(APPC) Trial Implementation supplement, each file whole without the
stylesheets, scripts and images the pages link. The HTML Technical Framework
is republished in place at each revision, so a re-run fails once IHE
publishes a new one; move the pins then. The General Introduction chapter on
copyright licences is taken as the grant the copy rests on." "" "$read_by"

oasis='OASIS, Appendix E of the document: "This document and translations of it may be copied and furnished to others ... provided that the above copyright notice and this paragraph are included on all such copies"; the PDF is copied unmodified with its notice'
pin commit access_control-xacml-2.0-core-spec-os.pdf "XACML 2.0, OASIS Standard, 1 Feb 2005" \
  https://docs.oasis-open.org/xacml/2.0/access_control-xacml-2.0-core-spec-os.pdf \
  7424acfe6dc2406e3efdf88ee83200885ab11951b0a1f721ae3505125213b4d0 "$oasis" \
  "the mapping cites 5.1 (PolicySet), 5.22 (Policy), 5.29 (Rule), 10.2.3, B.10 and Appendix C (the combining algorithms)"
pinned_corpus xacml "OASIS XACML 2.0" \
  "the OASIS XACML 2.0 core specification" \
  "The core specification of the eXtensible Access Control Markup Language
(XACML) Version 2.0, OASIS Standard, document identifier
\`oasis-access_control-xacml-2.0-core-spec-os\`, unmodified with its notice.
The XML schemas, the profiles and the errata published beside it are not
taken; the schema fragments the mapping cites are quoted in the core
specification itself." "" "$read_by"

say "done"
