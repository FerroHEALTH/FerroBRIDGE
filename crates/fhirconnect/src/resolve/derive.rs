// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The data-type pair a mapping with no `type` key converts through.
//!
//! The `type` key is deprecated "since the information is derivable from the
//! instances of FHIR and openEHR"
//! (`docs/specs/fhirconnect/modules/ROOT/pages/types-of-mappings/data-type/data-mappings.adoc`,
//! §Deprecated), so the compiler derives the pair once, from the two sides it
//! already resolved: the FHIR type code the element table gives the element
//! and the reference-model class the Web Template gives the node. The pairs
//! themselves are the rows of the data-type chapter
//! (`types-of-mappings/data-type/*.adoc`) that the engine's cells carry.
//!
//! The chapter fixes which pairs convert and no order among them, so which
//! pair wins when a side admits several is our own design: the order a class's
//! chapter page lists its pairs in.

use fhir_types::schema::Kind;
use fhir_types::schema::ValueKind;
use openehr_rm::v1_2::model;

use crate::model::ast::keyword::DataType;

/// The FHIR type codes each openEHR class converts with, in the order the
/// class's page of the data-type chapter lists them.
///
/// A row holds only the pairs a data-type cell of the engine carries both
/// ways, so a derived pair is always one the engine runs. A `DV_INTERVAL` row
/// is keyed by its parameter, because the bound class decides the cell:
/// `DV_INTERVAL<DV_DATE_TIME>` pairs with `Period` and
/// `DV_INTERVAL<DV_QUANTITY>` with `Range` (`DV_INTERVAL.adoc` §Range,
/// `data-mappings.adoc`), and the bare `DV_INTERVAL` row is the fallback for a
/// class name that carries no parameter.
/// `DV_BOOLEAN` against `boolean` is a primitive pair the chapter leaves out
/// as "straight forward" (`data-mappings.adoc`).
/// `DV_ORDINAL` against `CodeableConcept` carries the symbol, the value coming
/// from the options the template lists (`DV_ORDINAL.adoc` §`CodeableConcept`).
/// `PARTY_REF` against `Reference` is no row of the chapter: it carries
/// `ROLE.performer` in a party context, and no specification governs it, so
/// it is our own design.
pub const PAIRS: &[(&str, &[&str])] = &[
    ("DV_TEXT", &["string", "Coding", "CodeableConcept"]),
    ("DV_CODED_TEXT", &["CodeableConcept", "Coding"]),
    ("CODE_PHRASE", &["Coding"]),
    ("DV_DATE", &["date"]),
    ("DV_DATE_TIME", &["dateTime", "Period"]),
    ("DV_INTERVAL<DV_DATE_TIME>", &["Period"]),
    ("DV_INTERVAL<DV_QUANTITY>", &["Range"]),
    ("DV_INTERVAL", &["Period", "Range"]),
    ("PARTY_IDENTIFIED", &["Reference"]),
    ("PARTY_PROXY", &["Reference"]),
    ("DV_IDENTIFIER", &["Identifier"]),
    ("DV_PROPORTION", &["Quantity"]),
    ("DV_QUANTITY", &["Quantity", "decimal", "integer"]),
    ("DV_COUNT", &["integer", "Quantity", "Count"]),
    ("DV_ORDINAL", &["CodeableConcept"]),
    ("DV_TIME", &["time"]),
    ("DV_DURATION", &["Duration"]),
    ("DV_URI", &["url"]),
    ("DV_MULTIMEDIA", &["Attachment"]),
    ("DV_PARSABLE", &["Attachment"]),
    ("DV_BOOLEAN", &["boolean"]),
    ("PARTY_REF", &["Reference"]),
];

/// The FHIR type code every primitive that travels as a JSON string falls
/// back to.
pub const TEXT: &str = "string";

/// The reference-model class every node that holds other nodes descends from.
///
/// `PATHABLE` is the root of the classes a path can step through
/// (<https://specifications.openehr.org/releases/RM/Release-1.1.0/common.html#_pathable_class>),
/// and no data value is one.
const STRUCTURAL: &str = "PATHABLE";

/// Returns the class a Web Template type name names, without the generic
/// parameter a `DV_INTERVAL<T>` node carries (`DV_INTERVAL<DV_DATE_TIME>`).
///
/// `DV_INTERVAL<T>` is generic in its bound class
/// (<https://specifications.openehr.org/releases/RM/Release-1.1.0/data_types.html#_dv_interval_class>),
/// and the cells and the pairs are keyed by the class.
#[must_use]
pub fn base_class(rm_type: &str) -> &str {
    rm_type.split_once('<').map_or(rm_type, |(class, _)| class)
}

/// Returns the generic parameter a Web Template type name carries
/// (`DV_QUANTITY` of `DV_INTERVAL<DV_QUANTITY>`), `None` for a plain class.
#[must_use]
pub fn type_argument(rm_type: &str) -> Option<&str> {
    rm_type
        .split_once('<')
        .and_then(|(_, argument)| argument.strip_suffix('>'))
}

/// Returns the class an attribute of a node of `rm_type` holds: the node's
/// generic parameter where the attribute is typed by the class's formal
/// parameter, the attribute's declared type otherwise.
///
/// The attribute model resolves a formal parameter to its bound
/// (`DV_INTERVAL.lower` is `DV_ORDERED`), while `DV_INTERVAL<T>` types both
/// bounds `T` (<https://specifications.openehr.org/releases/RM/Release-1.1.0/data_types.html#_dv_interval_class>),
/// so a bound below a `DV_INTERVAL<DV_QUANTITY>` node is a `DV_QUANTITY`. An
/// attribute declared as the parameter's bound with no type arguments of its
/// own is read as typed by the parameter, and the argument replaces it only
/// when it conforms to that bound.
#[must_use]
pub fn attribute_class(rm_type: &str, attribute: &model::RmAttribute) -> &'static str {
    let declared = attribute.declared_type;
    let Some(argument) = type_argument(rm_type) else {
        return declared;
    };
    let parameterised = attribute.type_params.is_empty()
        && model::class(base_class(rm_type)).is_some_and(|class| {
            class
                .generic_params
                .iter()
                .any(|parameter| parameter.conforms_to == Some(declared))
        });
    if !parameterised {
        return declared;
    }
    model::class(argument)
        .filter(|class| model::is_a(class.name, declared))
        .map_or(declared, |class| class.name)
}

/// Returns the FHIR type codes `class` converts with, empty for a class no
/// cell carries.
///
/// A generic class name is looked up whole, so an interval whose parameter
/// no row names pairs with nothing; a bare class name falls back to its row.
#[must_use]
pub fn pairs(class: &str) -> &'static [&'static str] {
    let row = |wanted: &str| {
        PAIRS
            .iter()
            .find(|&&(name, _)| name == wanted)
            .map(|&(_, codes)| codes)
    };
    row(class)
        .or_else(|| {
            type_argument(class)
                .is_none()
                .then(|| row(base_class(class)))
                .flatten()
        })
        .unwrap_or(&[])
}

/// Returns whether `class` is a structural node, which holds other nodes and
/// carries no data value of its own.
#[must_use]
pub fn is_structural(class: &str) -> bool {
    model::is_a(class, STRUCTURAL)
}

/// Returns the FHIR type code an element of type `code` converts through
/// against a node of `class`.
///
/// The element's own type wins when the class pairs with it. A primitive that
/// travels as text and is no pair of the class converts as a `string`, the
/// FHIR type every such primitive specializes in its value space
/// (<https://hl7.org/fhir/R4/datatypes.html#primitive>), which is what the
/// engine did before the pair was derived.
#[must_use]
pub fn element(class: &str, code: &'static str, text: bool) -> &'static str {
    if text && !pairs(class).contains(&code) {
        return TEXT;
    }
    code
}

/// Returns the FHIR type code of the alternative a document carries for a
/// choice element, against a node of `class`.
///
/// A complex alternative names its type. A primitive's suffix is its type
/// code with the first letter raised (<https://hl7.org/fhir/R4/formats.html#choice>),
/// so it is matched against the class's pairs without regard to case, and an
/// alternative that travels as text and is no pair converts as a `string`.
/// A primitive that travels as a JSON boolean or a JSON number converts only
/// through a pair of the class, so it is `None` when no cell carries it.
#[must_use]
pub fn carried(class: &str, suffix: &str, kind: Kind) -> Option<&'static str> {
    match kind {
        Kind::Complex(name) => Some(name),
        Kind::Primitive(ValueKind::Text) | Kind::Attribute | Kind::Xhtml => Some(
            pairs(class)
                .iter()
                .find(|code| code.eq_ignore_ascii_case(suffix))
                .copied()
                .unwrap_or(TEXT),
        ),
        Kind::Primitive(ValueKind::Boolean | ValueKind::Integer | ValueKind::Decimal) => {
            pairs(class)
                .iter()
                .find(|code| code.eq_ignore_ascii_case(suffix))
                .copied()
        }
        Kind::Resource | Kind::Choice(_) => None,
    }
}

/// Returns the FHIR type code the `type` key names, `None` for `NONE`.
///
/// The key's identifiers are the Type IDs of the deprecated table in
/// `types-of-mappings/data-type/data-mappings.adoc`; `PROPORTION` travels as
/// the `Quantity` the engine's `DV_PROPORTION` cell carries.
#[must_use]
pub const fn declared(data_type: DataType) -> Option<&'static str> {
    match data_type {
        DataType::Quantity | DataType::Proportion => Some("Quantity"),
        DataType::DateTime => Some("dateTime"),
        DataType::CodeableConcept => Some("CodeableConcept"),
        DataType::Coding => Some("Coding"),
        DataType::String => Some("string"),
        DataType::Dosage => Some("Dosage"),
        DataType::Id => Some("id"),
        DataType::Identifier => Some("Identifier"),
        DataType::None => None,
    }
}

/// Returns the suffix of the alternative of a choice element that has the
/// type `code`, `None` when the choice admits no such alternative.
#[must_use]
pub fn suffix_of(code: &str, variants: &[(&'static str, Kind)]) -> Option<&'static str> {
    variants
        .iter()
        .find(|&&(suffix, _)| suffix.eq_ignore_ascii_case(code))
        .map(|&(suffix, _)| suffix)
}

/// Returns the suffix of the alternative a choice element is written as,
/// against a node of `class`.
///
/// The first pair of the class the choice admits is the one written; `None`
/// when the choice admits none of them.
#[must_use]
pub fn alternative(class: &str, variants: &[(&'static str, Kind)]) -> Option<&'static str> {
    pairs(class)
        .iter()
        .find_map(|code| suffix_of(code, variants))
}

#[cfg(test)]
mod tests {
    use fhir_types::schema::Kind;
    use fhir_types::schema::ValueKind;

    use super::alternative;
    use super::attribute_class;
    use super::carried;
    use super::element;
    use super::is_structural;
    use super::type_argument;
    use openehr_rm::v1_2::model;

    #[test]
    fn a_date_time_element_against_a_date_time_node_converts_as_a_date_time() {
        assert_eq!(element("DV_DATE_TIME", "dateTime", true), "dateTime");
    }

    #[test]
    fn a_date_time_element_against_a_text_node_converts_as_text() {
        assert_eq!(element("DV_TEXT", "dateTime", true), "string");
    }

    #[test]
    fn a_complex_element_keeps_its_type_whatever_the_class() {
        assert_eq!(element("DV_TEXT", "Annotation", false), "Annotation");
    }

    #[test]
    fn the_structural_classes_are_the_pathable_ones() {
        for class in [
            "EVALUATION",
            "CLUSTER",
            "COMPOSITION",
            "SECTION",
            "EVENT_CONTEXT",
        ] {
            assert!(is_structural(class), "{class} holds other nodes");
        }
        for class in [
            "DV_CODED_TEXT",
            "CODE_PHRASE",
            "PARTY_PROXY",
            "DV_DATE_TIME",
        ] {
            assert!(!is_structural(class), "{class} is a value");
        }
    }

    /// The alternatives `Extension.value[x]` admits that the pairs name.
    const VALUES: &[(&str, Kind)] = &[
        ("CodeableConcept", Kind::Complex("CodeableConcept")),
        ("Coding", Kind::Complex("Coding")),
        ("DateTime", Kind::Primitive(ValueKind::Text)),
        ("Period", Kind::Complex("Period")),
        ("String", Kind::Primitive(ValueKind::Text)),
    ];

    #[test]
    fn a_choice_is_written_as_the_first_pair_it_admits() {
        assert_eq!(
            alternative("DV_CODED_TEXT", VALUES),
            Some("CodeableConcept")
        );
        assert_eq!(alternative("CODE_PHRASE", VALUES), Some("Coding"));
        assert_eq!(alternative("DV_TEXT", VALUES), Some("String"));
        assert_eq!(alternative("DV_DATE_TIME", VALUES), Some("DateTime"));
    }

    #[test]
    fn a_choice_admitting_no_pair_of_the_class_has_no_alternative() {
        assert_eq!(alternative("DV_IDENTIFIER", VALUES), None);
        assert_eq!(alternative("DV_BOOLEAN", VALUES), None);
    }

    #[test]
    fn the_alternative_a_document_carries_names_its_type_code() {
        assert_eq!(
            carried("DV_DATE_TIME", "DateTime", Kind::Primitive(ValueKind::Text)),
            Some("dateTime")
        );
        assert_eq!(
            carried("DV_CODED_TEXT", "Coding", Kind::Complex("Coding")),
            Some("Coding")
        );
        assert_eq!(
            carried("DV_CODED_TEXT", "Code", Kind::Primitive(ValueKind::Text)),
            Some("string")
        );
        assert_eq!(
            carried("DV_TEXT", "Boolean", Kind::Primitive(ValueKind::Boolean)),
            None
        );
    }

    #[test]
    fn a_number_alternative_converts_only_through_a_pair_of_the_class() {
        // DV_QUANTITY.adoc §Primitives and DV_COUNT.adoc §Primitive.
        assert_eq!(
            carried("DV_COUNT", "Integer", Kind::Primitive(ValueKind::Integer)),
            Some("integer")
        );
        assert_eq!(
            carried(
                "DV_QUANTITY",
                "Decimal",
                Kind::Primitive(ValueKind::Decimal)
            ),
            Some("decimal")
        );
        assert_eq!(
            carried("DV_TEXT", "Integer", Kind::Primitive(ValueKind::Integer)),
            None
        );
    }

    #[test]
    fn a_quantity_choice_is_written_in_the_order_of_its_page() {
        let values: &[(&str, Kind)] = &[
            ("Quantity", Kind::Complex("Quantity")),
            ("Integer", Kind::Primitive(ValueKind::Integer)),
        ];
        assert_eq!(alternative("DV_QUANTITY", values), Some("Quantity"));
        assert_eq!(alternative("DV_COUNT", values), Some("Integer"));
    }

    #[test]
    fn a_generic_node_names_its_parameter() {
        assert_eq!(
            type_argument("DV_INTERVAL<DV_QUANTITY>"),
            Some("DV_QUANTITY")
        );
        assert_eq!(type_argument("DV_QUANTITY"), None);
    }

    #[test]
    fn a_bound_of_a_generic_interval_is_its_parameter() {
        let lower = model::attribute("DV_INTERVAL", "lower").expect("the model has the bound");
        assert_eq!(
            attribute_class("DV_INTERVAL<DV_QUANTITY>", lower),
            "DV_QUANTITY"
        );
        assert_eq!(attribute_class("DV_INTERVAL", lower), "DV_ORDERED");
        let flag =
            model::attribute("DV_INTERVAL", "lower_included").expect("the model has the flag");
        assert_eq!(attribute_class("DV_INTERVAL<DV_QUANTITY>", flag), "Boolean");
    }

    #[test]
    fn an_argument_outside_the_bound_keeps_the_declared_type() {
        let lower = model::attribute("DV_INTERVAL", "lower").expect("the model has the bound");
        assert_eq!(attribute_class("DV_INTERVAL<DV_TEXT>", lower), "DV_ORDERED");
    }

    #[test]
    fn an_interval_choice_is_picked_by_the_parameter() {
        // DV_INTERVAL.adoc §Range pairs a quantity interval with Range;
        // data-mappings.adoc pairs a Period with a date-time interval.
        let onset: &[(&str, Kind)] = &[
            ("Period", Kind::Complex("Period")),
            ("Range", Kind::Complex("Range")),
        ];
        assert_eq!(
            alternative("DV_INTERVAL<DV_QUANTITY>", onset),
            Some("Range")
        );
        assert_eq!(
            alternative("DV_INTERVAL<DV_DATE_TIME>", onset),
            Some("Period")
        );
        assert_eq!(alternative("DV_INTERVAL", onset), Some("Period"));
    }

    #[test]
    fn an_interval_of_a_class_no_cell_carries_has_no_pair() {
        assert!(
            super::pairs("DV_INTERVAL<DV_COUNT>").is_empty(),
            "no cell carries a count interval"
        );
        assert_eq!(super::pairs("DV_INTERVAL<DV_QUANTITY>"), &["Range"]);
    }
}
