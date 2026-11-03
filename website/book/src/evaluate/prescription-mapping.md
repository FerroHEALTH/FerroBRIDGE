<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# Prescriptions and dispensations from openEHR

The `openehr` feature maps an openEHR medication order to an electronic
prescription and a medication action to an electronic dispensation, in the
HL7 Europe Medication Prescription and Dispense (MPD) 1.0.0 form, and maps
a received dispensation back into openEHR. Regulation (EU) 2025/327 names
both as priority categories in Art 14(1)(b) and (c).

## The mapping

The mapping is FerroBRIDGE's MPD project of FHIRconnect 1.0.0 files. The
crate authors none of it and runs it as its input. The project has two
contexts over FerroBRIDGE's medication template. That template is written
in ADL 2 over published CKM archetypes, because the CKM publishes no
prescription or dispense template.

| Context | openEHR | MPD profile |
|---|---|---|
| `ferrobridge_eu_mpd_request.context` | `INSTRUCTION.medication_order.v3` | `MedicationRequest-eu-mpd`, with a `Medication-eu-mpd` and a `Dosage-eu-mpd` |
| `ferrobridge_eu_mpd_dispense.context` | `ACTION.medication.v1` at the careflow step "Prescription dispensed" | `MedicationDispense-eu-mpd`, with a `Medication-eu-mpd` |

The order's authorisation and dispense amount slots hold two CKM clusters,
and the request takes these elements from them:

| CKM element | MPD element |
|---|---|
| `CLUSTER.medication_authorisation.v0` "Maximum number of refills" | `dispenseRequest.numberOfRepeatsAllowed` |
| `CLUSTER.medication_authorisation.v0` "Minimum interval between refills" | `dispenseRequest.dispenseInterval` |
| `CLUSTER.medication_authorisation.v0` "Number of refills remaining" | the remaining-dispensations extension on `dispenseRequest` |
| `CLUSTER.medication_supply_amount.v0` "Amount", in "Dispense amount" | `dispenseRequest.quantity`, the amount for one fill |
| `CLUSTER.medication_supply_amount.v0` "Duration of supply" | `dispenseRequest.expectedSupplyDuration` |

The CKM "Dispense amount" is the amount of the ordered item to be
dispensed. That matches the R4 `dispenseRequest.quantity`, the amount for
one fill. It does not match the MPD `prescribedQuantity` extension, which
is the overall amount whatever the number of repeats. So the prescription
record of the `medication` module, which keeps only the Xt-EHR elements,
carries no per-fill amount.

Every `MedicationRequest` the mapping serves carries the status `unknown`.
The order keeps its status in the CKM order summary cluster, which the
archetype says is "generally used in non-prescription contexts". No
published crosswalk pairs its order status codes with the R4
`medicationrequest-status` codes the profile binds. A crosswalk written
here could serve a stopped order as an active one, so the mapping writes
the one code that states what is known.

## Mapping out of openEHR

`Mapping::compile_template` compiles the contexts against the operational
template, an OPT 1.4 or an ADL 2 one. `Mapping::to_fhir_with` then maps each
committed composition to a FHIR R4 `collection` Bundle: the request or the
dispense, the product it references, and one `Provenance` covering them.
The mapped request reads as the `Prescription` record and the mapped
dispense as the `Dispense` record, and both conform to the vendored MPD
profiles.

## Receiving a dispensation

MPD 1.0.0 defines no document profile, and its examples carry prescriptions
and dispenses in `collection` Bundles. `ReceivedResources::read` reads one
as strictly as a [received document](receiving-documents.md) is read, and
refuses a Bundle of any other type. `Mapping::resources_to_openehr` maps it
into one composition and keeps the collection as received in the
composition's `FEEDER_AUDIT.original_content`.

`ReceivedComposition::instructed_by` names the order the dispense carries
out in the `ACTION`'s `instruction_details`. That value is a `LOCATABLE_REF`
to the order's versioned composition, which the caller resolves. The system
that serves the category commits the composition at the member that owns
the prescription.
