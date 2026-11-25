<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# Provenance: the EEHRxF Annex X pass list

`pass-list.txt` records which of the 33 common data groups of Annex X of the
draft EEHRxF implementing act (Regulation (EU) 2025/327 Art 15(1), Have your
say initiative 17512, 2026-11-23) the FHIR facade serves as individual
resources, as draft Art 7(2) asks, one group name per line, and the group
count on its `total` line. It holds no copy of any input.

A group mapped to a resource type passes when the `eehrxf` Annex X project
(`crates/eehrxf/mappings/annex-x/`) serves it: a resource of the group,
mapped into a composition by its context, answers every IHE QEDm 3.0.0
PCC-44 search the group's option asks for (2:3.44.4.1.2.1) with the profile
its context names, and reads back by the id the search answers. A group the
QEDm options do not cover passes on read and the FHIR R4 base search
parameters. A data-type group (an address, a name, a dosage) passes when a
case asserts the element in each served resource that carries it. Every
other group fails with the reason it is not served yet.

`ferrobridge-server`'s `conformance_the_annex_x_groups_hold_their_pass_list`
measures it, and `scripts/checks/conformance.sh` gates it.
