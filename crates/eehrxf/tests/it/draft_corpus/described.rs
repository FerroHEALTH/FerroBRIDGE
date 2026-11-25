// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The `DocumentReference` element that carries each draft `EEHRxF` Annex III
//! Table 1 element of a document exchanged as an embedded PDF.
//!
//! Draft `EEHRxF` Art 3(1)(a) (initiative 17512, 2026-11-23) exchanges a
//! document as a PDF/A attachment embedded in a FHIR resource with its
//! metadata. Each row names a Table 1 element and the R4 `DocumentReference`
//! element `eehrxf::document::metadata` writes it to, or `None` where R4 has
//! no place for it. No specification publishes this map: our own design,
//! the table of the `eehrxf::document::metadata` module, held to the R4
//! resource by [`super::every_described_path_is_a_document_reference_element`].

/// The rows: the Table 1 element and the `DocumentReference` element.
pub(super) const DESCRIBED: &[(&str, Option<&str>)] = &[
    ("subject", Some("DocumentReference.subject")),
    ("identifier", Some("DocumentReference.masterIdentifier")),
    ("version", Some("DocumentReference.extension")),
    ("author[x]", Some("DocumentReference.author")),
    ("date", Some("DocumentReference.date")),
    ("attestation", None),
    ("attestation.attester[x]", None),
    ("attestation.dateTime", None),
    ("status", None),
    (
        "language",
        Some("DocumentReference.content.attachment.language"),
    ),
    ("documentCategory", Some("DocumentReference.category")),
    ("documentType", Some("DocumentReference.type")),
    ("documentTitle", Some("DocumentReference.description")),
    ("period", Some("DocumentReference.context.period")),
    (
        "legalAuthentication",
        Some("DocumentReference.authenticator"),
    ),
    (
        "legalAuthentication.legalAuthenticator[x]",
        Some("DocumentReference.authenticator"),
    ),
    ("legalAuthentication.dateTime", None),
    ("eventCategory", Some("DocumentReference.context.event")),
    (
        "serviceSpecialty",
        Some("DocumentReference.context.practiceSetting"),
    ),
    ("custodian", Some("DocumentReference.custodian")),
];
