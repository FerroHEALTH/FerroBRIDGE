// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `DV_MULTIMEDIA` and `DV_PARSABLE` against FHIR `Attachment`.
//!
//! The rows are
//! `docs/specs/fhirconnect/modules/ROOT/pages/types-of-mappings/data-type/DV_MULTIMEDIA.adoc`
//! and `DV_PARSABLE.adoc`. A multimedia value's `uri`, `data`, `media_type`,
//! `size` and `language` are the attachment's `url`, `data`, `contentType`,
//! `size` and `language`, and its `integrity_check` is `hash` when the
//! algorithm is SHA-1, the one R4 names for `Attachment.hash`
//! (<https://hl7.org/fhir/R4/datatypes.html#Attachment>). A parsable value's
//! `value` is the attachment's `data`, base64 encoded as `base64Binary`
//! requires, and its `formalism` is the `contentType`.
//!
//! Four rows need a decision of our own, each recorded on the data-type
//! defects upstream report:
//!
//! - The multimedia page warns that `compression_algorithm` has no FHIR field.
//!   Compressed data written as the plain media type would misstate the bytes,
//!   so a value that names a compression algorithm is refused going out.
//! - `size` is required in openEHR and optional in FHIR; an attachment with
//!   data and no size takes the decoded length of its data, a row that runs
//!   one way.
//! - The parsable page says `contentType` "should be hardcoded depending on
//!   the formalism used" and gives no table, so the formalism moves as the
//!   content type only when it is itself a media type (`type/subtype`), and is
//!   refused otherwise in both directions.
//! - The parsable page's R4 field is `contentType`, which its table spells
//!   `content_type`; the R4 name is the one written.

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use fhir_types::r4::attachment::Attachment;
use fhir_types::r4::primitives::Base64Binary;
use fhir_types::r4::primitives::Code;
use fhir_types::r4::primitives::UnsignedInt;
use fhir_types::r4::primitives::Url;
use openehr_base::v1_3::base_types::identification::terminology_id::TerminologyId;
use openehr_rm::v1_2::data_types::encapsulated::dv_multimedia::DvMultimedia;
use openehr_rm::v1_2::data_types::encapsulated::dv_parsable::DvParsable;
use openehr_rm::v1_2::data_types::text::code_phrase::CodePhrase;
use openehr_rm::v1_2::data_types::uri::dv_uri::DvUri;
use openehr_rm::v1_2::data_types::uri::dv_uri::DvUriData;

use crate::engine::lens::Fallback;
use crate::engine::lens::Lens;
use crate::engine::lens::LensError;
use crate::engine::lens::code_phrase::text_of;
use crate::model::ast::keyword::Direction;

/// The name the multimedia cell carries in a diagnostic.
pub const MULTIMEDIA_CELL: &str = "DV_MULTIMEDIA against Attachment";

/// The name the parsable cell carries in a diagnostic.
pub const PARSABLE_CELL: &str = "DV_PARSABLE against Attachment";

/// The openEHR terminology a media type is coded in.
pub const MEDIA_TYPES: &str = "IANA_media-types";

/// The openEHR terminology a language is coded in.
pub const LANGUAGES: &str = "ISO_639-1";

/// The openEHR terminology an integrity check algorithm is coded in.
pub const INTEGRITY_ALGORITHMS: &str = "openehr_integrity_check_algorithms";

/// The integrity check algorithm R4 names for `Attachment.hash`.
pub const SHA1: &str = "SHA-1";

/// `DV_MULTIMEDIA` and `DV_PARSABLE` against `Attachment`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct EncapsulatedLens;

impl Lens<DvMultimedia, Attachment> for EncapsulatedLens {
    fn cell(&self) -> &'static str {
        MULTIMEDIA_CELL
    }

    fn get(&self, source: &DvMultimedia) -> Result<Attachment, LensError> {
        if let Some(ref algorithm) = source.compression_algorithm {
            return Err(LensError::Shape {
                cell: MULTIMEDIA_CELL,
                attribute: "DV_MULTIMEDIA.compression_algorithm",
                found: algorithm.code_string.clone(),
                reason: "an Attachment has no field for a compression algorithm",
                direction: Direction::OpenehrToFhir,
            });
        }
        let sha1 = source
            .integrity_check_algorithm
            .as_ref()
            .is_some_and(|algorithm| algorithm.code_string == SHA1);
        let size = u32::try_from(source.size).map_err(|_refused| LensError::Shape {
            cell: MULTIMEDIA_CELL,
            attribute: "DV_MULTIMEDIA.size",
            found: source.size.to_string(),
            reason: "an Attachment size is an unsigned integer",
            direction: Direction::OpenehrToFhir,
        })?;
        Ok(Attachment {
            content_type: Some(Code::from(source.media_type.code_string.as_str())),
            language: source
                .language
                .as_ref()
                .map(|language| Code::from(language.code_string.as_str())),
            data: source.data.as_deref().map(Base64Binary::from),
            url: source.uri.as_ref().map(|uri| Url::from(uri_text(uri))),
            size: Some(UnsignedInt::from(size)),
            hash: source
                .integrity_check
                .as_deref()
                .filter(|_check| sha1)
                .map(Base64Binary::from),
            ..Attachment::default()
        })
    }

    fn put(
        &self,
        view: &Attachment,
        existing: Option<&DvMultimedia>,
    ) -> Result<DvMultimedia, LensError> {
        let media = text_of(view.content_type.as_ref().map(|code| &code.value)).ok_or(
            LensError::Missing {
                cell: MULTIMEDIA_CELL,
                attribute: "DV_MULTIMEDIA.media_type",
                direction: Direction::FhirToOpenehr,
            },
        )?;
        let data = text_of(view.data.as_ref().map(|data| &data.value));
        let size = match view.size.as_ref().and_then(|size| size.value) {
            Some(size) => size,
            None => decoded_length(data)?,
        };
        let size = i32::try_from(size).map_err(|_refused| LensError::Shape {
            cell: MULTIMEDIA_CELL,
            attribute: "Attachment.size",
            found: size.to_string(),
            reason: "a DV_MULTIMEDIA size is a signed 32-bit integer",
            direction: Direction::FhirToOpenehr,
        })?;
        let hash = text_of(view.hash.as_ref().map(|hash| &hash.value));
        let held_check = existing.and_then(|held| held.integrity_check.as_deref());
        let held_algorithm = existing.and_then(|held| held.integrity_check_algorithm.clone());
        let held_sha1 = held_algorithm
            .as_ref()
            .is_some_and(|algorithm| algorithm.code_string == SHA1);
        // NOTE: no specification governs this: our own design, a held check
        // `hash` cannot carry (another algorithm, or none) stays in place, and
        // a held SHA-1 check the view no longer carries is removed.
        let (integrity_check, integrity_check_algorithm) = if let Some(hash) = hash {
            let algorithm = held_algorithm
                .filter(|_held| held_sha1)
                .unwrap_or_else(|| phrase(INTEGRITY_ALGORITHMS, SHA1));
            (Some(String::from(hash)), Some(algorithm))
        } else if held_check.is_some() && held_sha1 {
            (None, None)
        } else {
            (held_check.map(String::from), held_algorithm)
        };
        let url = text_of(view.url.as_ref().map(|url| &url.value));
        Ok(DvMultimedia {
            charset: existing.and_then(|held| held.charset.clone()),
            language: coded(
                view.language.as_ref(),
                LANGUAGES,
                existing.and_then(|held| held.language.as_ref()),
            ),
            alternate_text: existing.and_then(|held| held.alternate_text.clone()),
            uri: url.map(|url| uri_as(url, existing.and_then(|held| held.uri.as_ref()))),
            data: data.map(String::from),
            media_type: coded_text(media, MEDIA_TYPES, existing.map(|held| &held.media_type)),
            compression_algorithm: None,
            integrity_check,
            integrity_check_algorithm,
            thumbnail: existing.and_then(|held| held.thumbnail.clone()),
            size,
        })
    }

    fn fallbacks(&self, view: &Attachment) -> Vec<Fallback> {
        if view.size.as_ref().and_then(|size| size.value).is_none() {
            return vec![Fallback::new(MULTIMEDIA_CELL, "DV_MULTIMEDIA.size")];
        }
        Vec::new()
    }
}

impl Lens<DvParsable, Attachment> for EncapsulatedLens {
    fn cell(&self) -> &'static str {
        PARSABLE_CELL
    }

    fn get(&self, source: &DvParsable) -> Result<Attachment, LensError> {
        // NOTE: R4 StructureDefinition-base64Binary: the regex takes at least one
        // group of four characters, so an empty value has no `data` to travel as.
        if source.value.is_empty() {
            return Err(LensError::Missing {
                cell: PARSABLE_CELL,
                attribute: "Attachment.data",
                direction: Direction::OpenehrToFhir,
            });
        }
        if !is_media_type(&source.formalism) {
            return Err(LensError::Shape {
                cell: PARSABLE_CELL,
                attribute: "DV_PARSABLE.formalism",
                found: source.formalism.clone(),
                reason: "the formalism is no media type, and the page gives no table to one",
                direction: Direction::OpenehrToFhir,
            });
        }
        Ok(Attachment {
            content_type: Some(Code::from(source.formalism.as_str())),
            language: source
                .language
                .as_ref()
                .map(|language| Code::from(language.code_string.as_str())),
            data: Some(Base64Binary::from(
                STANDARD.encode(source.value.as_bytes()).as_str(),
            )),
            ..Attachment::default()
        })
    }

    fn put(
        &self,
        view: &Attachment,
        existing: Option<&DvParsable>,
    ) -> Result<DvParsable, LensError> {
        let formalism = text_of(view.content_type.as_ref().map(|code| &code.value)).ok_or(
            LensError::Missing {
                cell: PARSABLE_CELL,
                attribute: "DV_PARSABLE.formalism",
                direction: Direction::FhirToOpenehr,
            },
        )?;
        if !is_media_type(formalism) {
            return Err(LensError::Shape {
                cell: PARSABLE_CELL,
                attribute: "Attachment.contentType",
                found: String::from(formalism),
                reason: "a content type is a media type",
                direction: Direction::FhirToOpenehr,
            });
        }
        let data =
            text_of(view.data.as_ref().map(|data| &data.value)).ok_or(LensError::Missing {
                cell: PARSABLE_CELL,
                attribute: "DV_PARSABLE.value",
                direction: Direction::FhirToOpenehr,
            })?;
        let refuse = || LensError::Shape {
            cell: PARSABLE_CELL,
            attribute: "Attachment.data",
            found: String::from("(base64 data)"),
            reason: "the data is no base64 of UTF-8 text",
            direction: Direction::FhirToOpenehr,
        };
        let bytes = STANDARD.decode(data).map_err(|_refused| refuse())?;
        let value = String::from_utf8(bytes).map_err(|_refused| refuse())?;
        Ok(DvParsable {
            charset: existing.and_then(|held| held.charset.clone()),
            language: coded(
                view.language.as_ref(),
                LANGUAGES,
                existing.and_then(|held| held.language.as_ref()),
            ),
            value,
            formalism: String::from(formalism),
        })
    }

    fn fallbacks(&self, _view: &Attachment) -> Vec<Fallback> {
        Vec::new()
    }
}

/// Returns the number of bytes base64 `data` decodes to.
fn decoded_length(data: Option<&str>) -> Result<u32, LensError> {
    let data = data.ok_or(LensError::Missing {
        cell: MULTIMEDIA_CELL,
        attribute: "DV_MULTIMEDIA.size",
        direction: Direction::FhirToOpenehr,
    })?;
    let refuse = |found: String, reason: &'static str| LensError::Shape {
        cell: MULTIMEDIA_CELL,
        attribute: "Attachment.data",
        found,
        reason,
        direction: Direction::FhirToOpenehr,
    };
    let bytes = STANDARD
        .decode(data)
        .map_err(|_refused| refuse(String::from("(base64 data)"), "the data is no base64"))?;
    u32::try_from(bytes.len()).map_err(|_refused| {
        refuse(
            bytes.len().to_string(),
            "the data is larger than an unsigned integer counts",
        )
    })
}

/// Returns whether `text` has the `type/subtype` form of a media type
/// (<https://www.rfc-editor.org/rfc/rfc6838#section-4.2>).
fn is_media_type(text: &str) -> bool {
    let token = |part: &str| {
        !part.is_empty()
            && part
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"!#$&-^_.+".contains(&byte))
    };
    let essence = text.split(';').next().unwrap_or(text).trim();
    essence
        .split_once('/')
        .is_some_and(|(kind, subtype)| token(kind) && token(subtype))
}

/// Returns the reference a `DV_URI` of either class holds.
fn uri_text(uri: &DvUri) -> &str {
    match *uri {
        DvUri::DvEhrUri(ref ehr) => &ehr.value,
        DvUri::DvUri(ref plain) => &plain.value,
    }
}

/// Returns the `DV_URI` of `url`, keeping the held one when it names the same
/// reference.
fn uri_as(url: &str, held: Option<&DvUri>) -> DvUri {
    match held {
        Some(uri) if uri_text(uri) == url => uri.clone(),
        _ => DvUri::DvUri(DvUriData {
            value: String::from(url),
        }),
    }
}

/// Returns a `CODE_PHRASE` of `code` in `terminology`.
fn phrase(terminology: &str, code: &str) -> CodePhrase {
    CodePhrase {
        terminology_id: TerminologyId {
            value: String::from(terminology),
        },
        code_string: String::from(code),
        preferred_term: None,
    }
}

/// Returns the `CODE_PHRASE` of `code`, keeping the held one when it carries
/// the same code.
fn coded_text(code: &str, terminology: &str, held: Option<&CodePhrase>) -> CodePhrase {
    match held {
        Some(phrase) if phrase.code_string == code => phrase.clone(),
        _ => phrase(terminology, code),
    }
}

/// Returns the `CODE_PHRASE` an optional FHIR code names.
fn coded(code: Option<&Code>, terminology: &str, held: Option<&CodePhrase>) -> Option<CodePhrase> {
    text_of(code.map(|code| &code.value)).map(|code| coded_text(code, terminology, held))
}

#[cfg(test)]
mod tests {
    use super::EncapsulatedLens;
    use super::INTEGRITY_ALGORITHMS;
    use super::LANGUAGES;
    use super::MEDIA_TYPES;
    use super::SHA1;
    use super::phrase;
    use crate::engine::lens::Lens;
    use crate::engine::lens::LensError;
    use fhir_types::r4::attachment::Attachment;
    use fhir_types::r4::primitives::Base64Binary;
    use fhir_types::r4::primitives::Code;
    use openehr_rm::v1_2::data_types::encapsulated::dv_multimedia::DvMultimedia;
    use openehr_rm::v1_2::data_types::encapsulated::dv_parsable::DvParsable;
    use openehr_rm::v1_2::data_types::uri::dv_uri::DvUri;
    use openehr_rm::v1_2::data_types::uri::dv_uri::DvUriData;
    use proptest::option;
    use proptest::prop_compose;
    use proptest::proptest;
    use proptest::sample::select;

    /// Returns a synthetic multimedia value of `size` bytes in `media`.
    fn multimedia(media: &str, size: i32) -> DvMultimedia {
        DvMultimedia {
            charset: None,
            language: None,
            alternate_text: None,
            uri: None,
            data: None,
            media_type: phrase(MEDIA_TYPES, media),
            compression_algorithm: None,
            integrity_check: None,
            integrity_check_algorithm: None,
            thumbnail: None,
            size,
        }
    }

    /// Returns a synthetic parsable value of `value` in `formalism`.
    fn parsable(value: &str, formalism: &str) -> DvParsable {
        DvParsable {
            charset: None,
            language: None,
            value: String::from(value),
            formalism: String::from(formalism),
        }
    }

    prop_compose! {
        /// A multimedia value with any of the attributes the cell carries.
        fn media()(
            media in select(vec!["image/png", "application/pdf"]),
            size in 0i32..10_000,
            data in option::of(select(vec!["c3ludGhldGlj", "AAEC"])),
            url in option::of(select(vec!["https://example.org/synthetic.png", "ehr:/synthetic"])),
            language in option::of(select(vec!["en", "nl"])),
            check in option::of(select(vec!["c3ludGhldGljLWhhc2g=", "AAAA"])),
            algorithm in option::of(select(vec![SHA1, "SHA-256"])),
            alternate in option::of(select(vec!["a synthetic picture"])),
        ) -> DvMultimedia {
            DvMultimedia {
                data: data.map(String::from),
                uri: url.map(|value| DvUri::DvUri(DvUriData { value: String::from(value) })),
                language: language.map(|code| phrase(LANGUAGES, code)),
                integrity_check: check.map(String::from),
                integrity_check_algorithm: algorithm.map(|code| phrase(INTEGRITY_ALGORITHMS, code)),
                alternate_text: alternate.map(String::from),
                ..multimedia(media, size)
            }
        }
    }

    proptest! {
        #[test]
        fn multimedia_get_put(source in media()) {
            let view: Attachment = EncapsulatedLens.get(&source).expect("the value travels");
            let back = EncapsulatedLens.put(&view, Some(&source)).expect("the view is complete");
            assert_eq!(back, source, "GetPut failed for {source:?}");
        }

        #[test]
        fn multimedia_put_get(source in media()) {
            let view: Attachment = EncapsulatedLens.get(&source).expect("the value travels");
            let back: DvMultimedia = EncapsulatedLens.put(&view, None).expect("the view is complete");
            let again: Attachment = EncapsulatedLens.get(&back).expect("the value travels");
            assert_eq!(again, view, "PutGet failed for {view:?}");
        }

        #[test]
        fn parsable_get_put(
            value in "[ -~]{1,40}",
            formalism in select(vec!["text/plain", "text/html", "application/xml"]),
            language in option::of(select(vec!["en", "nl"])),
        ) {
            let source = DvParsable {
                language: language.map(|code| phrase(LANGUAGES, code)),
                ..parsable(&value, formalism)
            };
            let view: Attachment = EncapsulatedLens.get(&source).expect("the value travels");
            let back = EncapsulatedLens.put(&view, Some(&source)).expect("the view is complete");
            assert_eq!(back, source.clone(), "GetPut failed for {source:?}");
            let fresh: DvParsable = EncapsulatedLens.put(&view, None).expect("the view is complete");
            let again: Attachment = EncapsulatedLens.get(&fresh).expect("the value travels");
            assert_eq!(again, view, "PutGet failed for {view:?}");
        }
    }

    #[test]
    fn a_compressed_value_is_refused_going_out() {
        // DV_MULTIMEDIA.adoc: "The compression algorithm has no field in FHIR".
        let source = DvMultimedia {
            compression_algorithm: Some(phrase("openehr_compression_algorithms", "gzip")),
            ..multimedia("image/png", 4)
        };
        let error = Lens::<DvMultimedia, Attachment>::get(&EncapsulatedLens, &source)
            .expect_err("compressed bytes are no image/png");
        assert!(
            matches!(error, LensError::Shape { attribute, .. } if attribute == "DV_MULTIMEDIA.compression_algorithm"),
            "the refusal names the algorithm: {error}"
        );
    }

    #[test]
    fn a_hash_is_written_only_for_a_sha1_check() {
        let source = DvMultimedia {
            integrity_check: Some(String::from("AAAA")),
            integrity_check_algorithm: Some(phrase(INTEGRITY_ALGORITHMS, "SHA-256")),
            ..multimedia("image/png", 4)
        };
        let view: Attachment = EncapsulatedLens.get(&source).expect("the value travels");
        assert_eq!(view.hash, None, "R4 Attachment.hash is a SHA-1");
    }

    #[test]
    fn an_attachment_with_no_size_takes_the_length_of_its_data() {
        let view = Attachment {
            content_type: Some(Code::from("image/png")),
            data: Some(Base64Binary::from("c3ludGhldGlj")),
            ..Attachment::default()
        };
        let back: DvMultimedia = EncapsulatedLens.put(&view, None).expect("the data counts");
        assert_eq!(back.size, 9, "\"synthetic\" is nine bytes");
        assert_eq!(
            Lens::<DvMultimedia, Attachment>::fallbacks(&EncapsulatedLens, &view).len(),
            1
        );
        let empty = Attachment { data: None, ..view };
        assert!(Lens::<DvMultimedia, Attachment>::put(&EncapsulatedLens, &empty, None).is_err());
    }

    #[test]
    fn a_formalism_that_is_no_media_type_is_refused_both_ways() {
        // DV_PARSABLE.adoc gives no table from a formalism to a content type.
        assert!(
            Lens::<DvParsable, Attachment>::get(&EncapsulatedLens, &parsable("x", "GLIF 1.0"))
                .is_err()
        );
        let view = Attachment {
            content_type: Some(Code::from("GLIF")),
            data: Some(Base64Binary::from("eA==")),
            ..Attachment::default()
        };
        assert!(Lens::<DvParsable, Attachment>::put(&EncapsulatedLens, &view, None).is_err());
    }

    #[test]
    fn an_empty_parsable_value_is_refused_going_out() {
        assert!(
            Lens::<DvParsable, Attachment>::get(&EncapsulatedLens, &parsable("", "text/plain"))
                .is_err()
        );
    }

    #[test]
    fn parsable_data_that_is_no_utf8_text_is_refused() {
        let view = Attachment {
            content_type: Some(Code::from("text/plain")),
            data: Some(Base64Binary::from("/w==")),
            ..Attachment::default()
        };
        assert!(Lens::<DvParsable, Attachment>::put(&EncapsulatedLens, &view, None).is_err());
    }
}
