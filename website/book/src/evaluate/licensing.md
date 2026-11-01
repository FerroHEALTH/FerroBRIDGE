<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# Licensing

FerroBRIDGE is source-available under the Business Source License 1.1, which
is not an OSI-approved open-source licence. The authoritative text is
[`LICENSE`](https://github.com/FerroHEALTH/FerroBRIDGE/blob/main/LICENSE),
with a summary in
[`NOTICE`](https://github.com/FerroHEALTH/FerroBRIDGE/blob/main/NOTICE). This
page states the terms; where it and the licence differ, the licence text wins.

## Do you need a commercial licence?

The table is the boundary in the order people ask about it. The `Why` column
names the part of `LICENSE` that decides each row.

| What you are doing | What you need | Why |
|---|---|---|
| Reading, building, modifying or redistributing the source | Free | Terms: "the right to copy, modify, create derivative works, redistribute". |
| Development, testing, evaluation, prototyping | Free | Terms: the right to "make non-production use of the Licensed Work". |
| Production use for Non-Commercial Purposes | Free | Additional Use Grant: personal use, academic or scientific research, teaching, and use by a non-profit organisation or public body that is not in the course of a business, does not deliver a service for payment, and is not for commercial advantage. |
| A hospital, clinic or care provider running it for its patients | Commercial licence | Additional Use Grant: "the delivery of health care or any other service for payment, requires a commercial license". |
| A vendor or integrator, or any company running it in production | Commercial licence | Additional Use Grant: production use in the course of a business is outside Non-Commercial Purposes, and "Any other production use" needs a commercial license. |
| Offering it, or a work derived from it, to third parties as a hosted, managed or embedded service | Commercial licence | Additional Use Grant (a): a service "through which anyone other than you and your affiliates maps, converts, or exchanges health data", excluded whoever you are. |
| Selling, sublicensing or otherwise distributing it for a fee | Commercial licence | Additional Use Grant (b): excluded whoever you are, on its own or as a component of another product. |

The last two rows hold whatever else you are: they need a commercial licence
even for an organisation the rows above would otherwise leave free.

## Arranging a commercial licence

A commercial licence is arranged with Cadasto B.V., the Licensor, which handles
the business side of FerroBRIDGE: write to
[info@cadasto.com](mailto:info@cadasto.com) or use
<https://www.cadasto.com/contact/>. Technical questions go to the maintainer
named in
[`MAINTAINERS.md`](https://github.com/FerroHEALTH/FerroBRIDGE/blob/main/MAINTAINERS.md).

## The change date

Each version becomes available under the Apache License 2.0 four years after
that version is published (`LICENSE`, Change Date and Change License).

## The published crates

Two published crates are under the Apache License 2.0 instead of the Business
Source License, because they are generated models meant for any Rust project:
`fhir-types` and `hl7v2-types`, each with its own `LICENSE`. Every other
published crate is BUSL-1.1 on the terms above, which its `Cargo.toml`
`license` field and its README state.

## There is no open-core tier

The bridge, the server, and the tools are in this repository under the one
licence. Nothing is held back to be sold back to you.

## Contributions and third-party material

A contribution is licensed under the same licence.
You keep your copyright, and you grant the Licensor the relicensing right in
CONTRIBUTING.md § Licensing of contributions, accepted by opening the pull
request.
Vendored specifications and third-party material keep their upstream terms,
recorded in a `PROVENANCE.md` beside each vendored tree.

The licensing and trademark terms every FerroHEALTH product shares, the same
table for each of them included, are on the family book's
[licensing page](https://ferrohealth.eu/docs/licensing.html).
