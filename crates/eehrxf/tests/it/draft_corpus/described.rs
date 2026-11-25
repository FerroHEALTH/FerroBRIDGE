// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The `DocumentReference` element that carries each draft `EEHRxF` Annex III
//! Table 1 element of a document exchanged as an embedded PDF.
//!
//! Draft `EEHRxF` Art 3(1)(a) (initiative 17512, 2026-11-23) exchanges a
//! document as a PDF/A attachment embedded in a FHIR resource with its
//! metadata. Each row names a Table 1 element and the R4 `DocumentReference`
//! element `eehrxf::document::metadata` writes it to, or why the profiles it
//! claims leave it no place, which the corpus counts unmapped. No
//! specification publishes this map: our own design,
//! the table of the `eehrxf::document::metadata` module, held to the R4
//! resource by [`super::every_described_path_is_a_document_reference_element`].

/// Why R4 `DocumentReference` carries no attestation.
const NO_ATTESTER: &str = "R4 DocumentReference has no attester";

/// The rows: the Table 1 element and the `DocumentReference` element, or
/// the reason it has none.
pub(super) const DESCRIBED: &[(&str, Result<&str, &str>)] = &[
    ("subject", Ok("DocumentReference.subject")),
    ("identifier", Ok("DocumentReference.masterIdentifier")),
    ("version", Ok("DocumentReference.extension")),
    ("author[x]", Ok("DocumentReference.author")),
    ("date", Ok("DocumentReference.date")),
    ("attestation", Err(NO_ATTESTER)),
    ("attestation.attester[x]", Err(NO_ATTESTER)),
    ("attestation.dateTime", Err(NO_ATTESTER)),
    (
        "status",
        Err(
            "EehrxfMhdDocumentReference holds docStatus to 0..0 (IHE MHD Minimal), and status is the reference's own availability (upstream report #2231)",
        ),
    ),
    (
        "language",
        Ok("DocumentReference.content.attachment.language"),
    ),
    ("documentCategory", Ok("DocumentReference.category")),
    ("documentType", Ok("DocumentReference.type")),
    ("documentTitle", Ok("DocumentReference.description")),
    ("period", Ok("DocumentReference.context.period")),
    ("legalAuthentication", Ok("DocumentReference.authenticator")),
    (
        "legalAuthentication.legalAuthenticator[x]",
        Ok("DocumentReference.authenticator"),
    ),
    (
        "legalAuthentication.dateTime",
        Err("R4 DocumentReference.authenticator is a Reference, with no time"),
    ),
    ("eventCategory", Ok("DocumentReference.context.event")),
    (
        "serviceSpecialty",
        Ok("DocumentReference.context.practiceSetting"),
    ),
    ("custodian", Ok("DocumentReference.custodian")),
];
