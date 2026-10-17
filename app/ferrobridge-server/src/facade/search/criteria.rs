// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! One search request, read against the parameter set its type declares.
//!
//! The parameters arrive as `name[:modifier]=value` pairs from the query
//! string or the form body of `POST _search`
//! (<https://hl7.org/fhir/R4/search.html>, §3.1.1.2). A repeated parameter
//! is an `AND`, a comma inside one value an `OR` (§3.1.1.4.17). A parameter
//! the type does not declare is an error unless the client sent
//! `Prefer: handling=lenient` (§3.1.1.3), because the issue that fixes this
//! surface says a parameter outside the `CapabilityStatement` is "refused
//! with an `OperationOutcome`, never ignored"; a modifier the facade does not
//! support is always an error, which R4 states as a SHALL (§3.1.1.4.4). The
//! parameters the search used are kept in their received order, so the
//! `self` link names exactly those (§3.1.1.6).

use http::HeaderMap;

use crate::facade::media;
use crate::facade::outcome::Issue;
use crate::facade::outcome::IssueType;
use crate::facade::search::Declared;
use crate::facade::search::declaration::ParameterType;
use crate::facade::search::value;
use crate::facade::search::value::DateValue;
use crate::facade::search::value::StringMatch;
use crate::facade::search::value::Token;
use crate::facade::search::value::ValueError;

/// The parameter that names the page a `next` link continues from.
///
/// R4 lets a server "provide its own parameters in the links that it uses to
/// manage the state of the search" (§3.1.1.5.3); the name is our own design.
pub const CURSOR: &str = "_cursor";

/// What a client asked the facade to do with a parameter it does not
/// support.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Handling {
    /// `Prefer: handling=strict`, and the default: refuse the search.
    #[default]
    Strict,
    /// `Prefer: handling=lenient`: ignore the parameter and leave it out of
    /// the `self` link.
    Lenient,
}

impl Handling {
    /// Returns the handling `headers` ask for.
    ///
    /// "Servers SHOULD honor the client's request"
    /// (<https://hl7.org/fhir/R4/search.html>, §3.1.1.3), and a request that
    /// asks for neither is strict.
    #[must_use]
    pub fn of(headers: &HeaderMap) -> Self {
        for value in headers.get_all("prefer") {
            let Ok(text) = value.to_str() else {
                continue;
            };
            for part in text.split(',') {
                match part.trim().to_ascii_lowercase().as_str() {
                    "handling=lenient" => return Self::Lenient,
                    "handling=strict" => return Self::Strict,
                    _ => {}
                }
            }
        }
        Self::Strict
    }
}

/// What the client asked about `Bundle.total`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Total {
    /// `none`: the client will not read it.
    None,
    /// `estimate`: a rough number will do.
    Estimate,
    /// `accurate`: the exact number of matches.
    Accurate,
}

/// One value of a subject parameter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Subject {
    /// A literal reference, `Patient/[id]` or an absolute URL, as the
    /// identity map keys it.
    Literal(String),
    /// A reference to a type no EHR subject is, which matches nothing.
    Never,
}

/// The comparison one declared parameter asks for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Test {
    /// A token parameter, any of these values.
    Token(Vec<Token>),
    /// A string parameter, any of these values matched one way.
    Text {
        /// How each value is matched.
        matching: StringMatch,
        /// The values.
        values: Vec<String>,
    },
    /// A date parameter, any of these values.
    Date(Vec<DateValue>),
}

/// One occurrence of a declared parameter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Filter {
    /// The parameter name.
    pub name: String,
    /// What it compares.
    pub test: Test,
}

/// One search request, read.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Criteria {
    /// Each `_id` occurrence, any of its logical ids.
    pub ids: Vec<Vec<String>>,
    /// Each subject parameter occurrence, any of its subjects.
    pub subjects: Vec<Vec<Subject>>,
    /// Each declared parameter occurrence.
    pub filters: Vec<Filter>,
    /// The `_sort` keys: a parameter name and whether it orders downward.
    pub sort: Vec<(String, bool)>,
    /// The `_count` the client sent.
    pub count: Option<u32>,
    /// Whether the client asked for the count alone (`_summary=count`).
    pub summary_count: bool,
    /// The `_total` the client sent.
    pub total: Option<Total>,
    /// The cursor of the page asked for.
    pub cursor: Option<String>,
    /// The parameters the search used, as received, without `_count` and the
    /// cursor.
    pub used: Vec<(String, String)>,
    /// The parameters ignored under lenient handling.
    pub ignored: Vec<String>,
}

/// Why one parameter refuses the search.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum CriteriaError {
    /// A parameter the type does not declare, under strict handling.
    #[error(
        "{name} is not a search parameter of {resource_type} on this server; GET [base]/metadata names those it supports"
    )]
    Unknown {
        /// The parameter as sent.
        name: String,
        /// The type searched.
        resource_type: String,
    },
    /// A modifier the parameter does not support.
    #[error("the modifier :{modifier} is not supported on {name}")]
    Modifier {
        /// The parameter name.
        name: String,
        /// The modifier.
        modifier: String,
    },
    /// A value outside its type's grammar, or a control parameter sent twice.
    #[error("{name}: {reason}")]
    Invalid {
        /// The parameter name.
        name: String,
        /// Why.
        reason: String,
    },
    /// A well-formed value asking for what this server does not do.
    #[error("{name}: {reason}")]
    Unsupported {
        /// The parameter name.
        name: String,
        /// Why.
        reason: String,
    },
    /// A search that names none of the subject parameters the type requires.
    #[error("a {resource_type} search names its subject with one of: {names}")]
    Required {
        /// The type searched.
        resource_type: String,
        /// The subject parameter names, comma separated.
        names: String,
    },
}

impl CriteriaError {
    /// Returns the `OperationOutcome` issue this refusal renders as.
    #[must_use]
    pub fn issue(&self) -> Issue {
        let (code, location) = match *self {
            Self::Unknown { ref name, .. } | Self::Modifier { ref name, .. } => {
                (IssueType::NotSupported, Some(name.clone()))
            }
            Self::Invalid { ref name, .. } => (IssueType::Invalid, Some(name.clone())),
            Self::Unsupported { ref name, .. } => (IssueType::NotSupported, Some(name.clone())),
            Self::Required { .. } => (IssueType::Required, None),
        };
        let issue = Issue::error(code).diagnosing(self.to_string());
        match location {
            Some(name) => issue.at(name),
            None => issue,
        }
    }
}

/// Reads `pairs` against what `declared` answers.
///
/// `base` is the FHIR service base, so a subject written as an absolute URL
/// on this server reads as the relative reference the identity map keys.
///
/// # Errors
///
/// Returns every [`CriteriaError`] the pairs raise, in received order.
pub fn read(
    pairs: &[(String, String)],
    declared: &Declared,
    handling: Handling,
    base: &str,
) -> Result<Criteria, Vec<CriteriaError>> {
    let mut reader = Reader {
        declared,
        handling,
        base,
        criteria: Criteria::default(),
        errors: Vec::new(),
        seen: Vec::new(),
    };
    for (name, text) in pairs {
        reader.pair(name, text);
    }
    reader.require_subject();
    if reader.errors.is_empty() {
        Ok(reader.criteria)
    } else {
        Err(reader.errors)
    }
}

/// The state of one read.
struct Reader<'a> {
    /// What the type answers.
    declared: &'a Declared,
    /// What to do with an unsupported parameter.
    handling: Handling,
    /// The FHIR service base.
    base: &'a str,
    /// What has been read so far.
    criteria: Criteria,
    /// What has been refused so far.
    errors: Vec<CriteriaError>,
    /// The control parameters seen, each allowed once.
    seen: Vec<&'static str>,
}

impl Reader<'_> {
    /// Reads one pair.
    fn pair(&mut self, name: &str, text: &str) {
        let (base, modifier) = match name.split_once(':') {
            Some((base, modifier)) => (base, Some(modifier)),
            None => (name, None),
        };
        if text.is_empty() {
            // NOTE: R4 search.html §3.1.1.3: "Empty parameters are not an error -
            // they are just ignored by the server", so the self link omits them.
            return;
        }
        match base {
            "_count" => self.control(name, modifier, "_count", |reader| reader.count(text)),
            CURSOR => self.control(name, modifier, CURSOR, |reader| {
                reader.criteria.cursor = Some(text.to_owned());
            }),
            "_sort" => self.control(name, modifier, "_sort", |reader| reader.sort(text)),
            "_summary" => self.control(name, modifier, "_summary", |reader| {
                reader.summary(name, text);
            }),
            "_total" => self.control(name, modifier, "_total", |reader| reader.total(name, text)),
            "_id" => self.ids(name, modifier, text),
            _ => self.declared_parameter(name, base, modifier, text),
        }
    }

    /// Reads a control parameter, which takes no modifier and appears once.
    fn control(
        &mut self,
        name: &str,
        modifier: Option<&str>,
        key: &'static str,
        read: impl FnOnce(&mut Self),
    ) {
        if let Some(modifier) = modifier {
            self.errors.push(CriteriaError::Modifier {
                name: key.to_owned(),
                modifier: modifier.to_owned(),
            });
            return;
        }
        if self.seen.contains(&key) {
            self.errors.push(CriteriaError::Invalid {
                name: name.to_owned(),
                reason: String::from("the parameter is sent more than once"),
            });
            return;
        }
        self.seen.push(key);
        read(self);
    }

    /// Reads `_count` through the one reader the request guard uses.
    fn count(&mut self, text: &str) {
        match media::page_size(text) {
            Ok(count) => self.criteria.count = Some(count),
            Err(error) => self.errors.push(CriteriaError::Invalid {
                name: String::from("_count"),
                reason: format!("`{}` is no non-negative whole number", error.found()),
            }),
        }
    }

    /// Reads `_sort`: declared parameters, each optionally prefixed `-`
    /// (<https://hl7.org/fhir/R4/search.html>, §3.1.1.5.1).
    fn sort(&mut self, text: &str) {
        let mut kept = Vec::new();
        for item in text.split(',') {
            let (descending, parameter) = match item.strip_prefix('-') {
                Some(rest) => (true, rest),
                None => (false, item),
            };
            if self
                .declared
                .parameters()
                .get(parameter)
                .is_some_and(crate::facade::search::declaration::Parameter::sortable)
            {
                self.criteria.sort.push((parameter.to_owned(), descending));
                kept.push(item.to_owned());
            } else {
                self.unsupported(
                    "_sort",
                    format!(
                        "`{parameter}` is no parameter {} sorts on; _sort names a declared parameter read under the composition, which has one value per composition",
                        self.declared.resource_type()
                    ),
                );
            }
        }
        if !kept.is_empty() {
            self.criteria
                .used
                .push((String::from("_sort"), kept.join(",")));
        }
    }

    /// Reads `_summary`, of which this server does `count` and `false`.
    fn summary(&mut self, name: &str, text: &str) {
        match text {
            "count" => self.criteria.summary_count = true,
            "false" => {}
            other => {
                self.unsupported(
                    name,
                    format!(
                        "_summary={other} is not supported; this server answers count and false"
                    ),
                );
                return;
            }
        }
        self.criteria.used.push((name.to_owned(), text.to_owned()));
    }

    /// Reads `_total` (<https://hl7.org/fhir/R4/search.html>, §3.1.1.5.2).
    fn total(&mut self, name: &str, text: &str) {
        let total = match text {
            "none" => Total::None,
            "estimate" => Total::Estimate,
            "accurate" => Total::Accurate,
            other => {
                self.errors.push(CriteriaError::Invalid {
                    name: name.to_owned(),
                    reason: format!("`{other}` is none of none, estimate and accurate"),
                });
                return;
            }
        };
        self.criteria.total = Some(total);
        self.criteria.used.push((name.to_owned(), text.to_owned()));
    }

    /// Reads `_id`, whose values are logical ids.
    fn ids(&mut self, name: &str, modifier: Option<&str>, text: &str) {
        if let Some(modifier) = modifier {
            self.errors.push(CriteriaError::Modifier {
                name: String::from("_id"),
                modifier: modifier.to_owned(),
            });
            return;
        }
        let mut ids = Vec::new();
        for part in value::split(text, ',') {
            match value::unescape(&part) {
                Ok(id) => ids.push(id),
                Err(error) => {
                    self.invalid_value(name, &error);
                    return;
                }
            }
        }
        self.criteria.ids.push(ids);
        self.criteria.used.push((name.to_owned(), text.to_owned()));
    }

    /// Reads a parameter the declaration names, or refuses an unknown one.
    fn declared_parameter(&mut self, name: &str, base: &str, modifier: Option<&str>, text: &str) {
        let Some(kind) = self.declared.surface().get(base).copied() else {
            // TODO(#492): chained parameters such as `patient.identifier` read here.
            self.unknown(name);
            return;
        };
        let read = match kind {
            ParameterType::Reference => self.subject(modifier, text),
            ParameterType::Token => no_modifier(modifier).and_then(|()| {
                each(text, value::token).map(|values| {
                    self.criteria.filters.push(Filter {
                        name: base.to_owned(),
                        test: Test::Token(values),
                    });
                })
            }),
            ParameterType::String => self.text(base, modifier, text),
            ParameterType::Date => no_modifier(modifier).and_then(|()| {
                each(text, value::date).map(|values| {
                    self.criteria.filters.push(Filter {
                        name: base.to_owned(),
                        test: Test::Date(values),
                    });
                })
            }),
        };
        match read {
            Ok(()) => self.criteria.used.push((name.to_owned(), text.to_owned())),
            Err(Refused::Modifier(modifier)) => self.errors.push(CriteriaError::Modifier {
                name: base.to_owned(),
                modifier,
            }),
            Err(Refused::Value(error)) => self.invalid_value(base, &error),
        }
    }

    /// Reads a string parameter with its optional `:exact` or `:contains`
    /// (<https://hl7.org/fhir/R4/search.html>, §3.1.1.4.8).
    fn text(&mut self, base: &str, modifier: Option<&str>, text: &str) -> Result<(), Refused> {
        let matching = match modifier {
            None => StringMatch::Start,
            Some("exact") => StringMatch::Exact,
            Some("contains") => StringMatch::Contains,
            Some(other) => return Err(Refused::Modifier(other.to_owned())),
        };
        let values = each(text, |part| {
            let plain = value::unescape(part)?;
            value::like_pattern(&plain, matching)?;
            Ok(plain)
        })?;
        self.criteria.filters.push(Filter {
            name: base.to_owned(),
            test: Test::Text { matching, values },
        });
        Ok(())
    }

    /// Reads a subject parameter: `[id]`, `Patient/[id]`, an absolute URL, or
    /// a `:[type]` modifier (<https://hl7.org/fhir/R4/search.html>,
    /// §3.1.1.4.9).
    fn subject(&mut self, modifier: Option<&str>, text: &str) -> Result<(), Refused> {
        let typed = match modifier {
            None => None,
            // TODO(#492): the `:identifier` modifier resolves a subject by its identifier.
            Some(kind) if is_type_name(kind) => Some(kind),
            Some(other) => return Err(Refused::Modifier(other.to_owned())),
        };
        let own = format!("{}/", self.base.trim_end_matches('/'));
        let subjects = each(text, |part| {
            let plain = value::unescape(part)?;
            Ok(subject_of(&plain, typed, &own))
        })?;
        self.criteria.subjects.push(subjects);
        Ok(())
    }

    /// Refuses a search that names none of the subject parameters a
    /// declaration requires, unless `_id` names the resources outright.
    fn require_subject(&mut self) {
        let Some(scope) = self.declared.patient() else {
            return;
        };
        if scope.required() && self.criteria.subjects.is_empty() && self.criteria.ids.is_empty() {
            self.errors.push(CriteriaError::Required {
                resource_type: self.declared.resource_type().to_owned(),
                names: scope.names().join(", "),
            });
        }
    }

    /// Refuses or ignores an unknown parameter, as the handling says.
    fn unknown(&mut self, name: &str) {
        match self.handling {
            Handling::Strict => self.errors.push(CriteriaError::Unknown {
                name: name.to_owned(),
                resource_type: self.declared.resource_type().to_owned(),
            }),
            Handling::Lenient => self.criteria.ignored.push(name.to_owned()),
        }
    }

    /// Refuses or ignores an unsupported use of a known parameter, as the
    /// handling says.
    fn unsupported(&mut self, name: &str, reason: String) {
        match self.handling {
            Handling::Strict => self.errors.push(CriteriaError::Unsupported {
                name: name.to_owned(),
                reason,
            }),
            Handling::Lenient => self.criteria.ignored.push(name.to_owned()),
        }
    }

    /// Records a malformed or unsupported value.
    fn invalid_value(&mut self, name: &str, error: &ValueError) {
        self.errors.push(match *error {
            ValueError::Malformed(ref reason) => CriteriaError::Invalid {
                name: name.to_owned(),
                reason: reason.clone(),
            },
            ValueError::Unsupported(ref reason) => CriteriaError::Unsupported {
                name: name.to_owned(),
                reason: reason.clone(),
            },
        });
    }
}

/// Why one parameter occurrence was refused while it was read.
enum Refused {
    /// A modifier the parameter does not take.
    Modifier(String),
    /// A value outside the grammar or outside what this server does.
    Value(ValueError),
}

impl From<ValueError> for Refused {
    fn from(error: ValueError) -> Self {
        Self::Value(error)
    }
}

/// Refuses any modifier on a parameter that takes none.
fn no_modifier(modifier: Option<&str>) -> Result<(), Refused> {
    modifier.map_or(Ok(()), |modifier| {
        Err(Refused::Modifier(modifier.to_owned()))
    })
}

/// Reads every comma-separated value of `text` with `read`.
fn each<T>(text: &str, read: impl Fn(&str) -> Result<T, ValueError>) -> Result<Vec<T>, Refused> {
    value::split(text, ',')
        .iter()
        .map(|part| read(part).map_err(Refused::Value))
        .collect()
}

/// Whether `text` reads as a FHIR resource type name: an upper-case letter,
/// then letters and digits.
fn is_type_name(text: &str) -> bool {
    let mut characters = text.chars();
    characters
        .next()
        .is_some_and(|first| first.is_ascii_uppercase())
        && characters.all(|character| character.is_ascii_alphanumeric())
}

/// Returns the subject one reference value names.
///
/// Every subject the facade binds is a `Patient` (the EHR's subject), so a
/// bare id or a `:Patient` modifier reads as `Patient/[id]` on either the
/// `patient` or the `subject` parameter, and a reference to another type
/// matches nothing (no specification governs the bare id on a parameter with
/// several target types: our own design).
fn subject_of(value: &str, typed: Option<&str>, own: &str) -> Subject {
    let relative = value.strip_prefix(own).unwrap_or(value);
    let bare = !relative.contains('/');
    match typed {
        Some("Patient") | None if bare => Subject::Literal(format!("Patient/{relative}")),
        None if relative.starts_with("Patient/") || relative.contains("://") => {
            Subject::Literal(relative.to_owned())
        }
        _ => Subject::Never,
    }
}

#[cfg(test)]
mod tests {
    use super::{Handling, Subject, subject_of};
    use http::{HeaderMap, HeaderValue};

    #[test]
    fn handling_is_strict_unless_the_client_asks_for_lenient() {
        assert_eq!(Handling::Strict, Handling::of(&HeaderMap::new()));
        let mut headers = HeaderMap::new();
        headers.insert(
            "prefer",
            HeaderValue::from_static("return=minimal, handling=lenient"),
        );
        assert_eq!(Handling::Lenient, Handling::of(&headers));
        headers.insert("prefer", HeaderValue::from_static("handling=strict"));
        assert_eq!(Handling::Strict, Handling::of(&headers));
    }

    #[test]
    fn a_subject_reads_as_the_literal_reference_the_map_keys() {
        let own = "http://ferrobridge.invalid/fhir/";
        assert_eq!(
            Subject::Literal(String::from("Patient/p-1")),
            subject_of("p-1", None, own)
        );
        assert_eq!(
            Subject::Literal(String::from("Patient/p-1")),
            subject_of("http://ferrobridge.invalid/fhir/Patient/p-1", None, own)
        );
        assert_eq!(
            Subject::Literal(String::from("Patient/p-1")),
            subject_of("p-1", Some("Patient"), own)
        );
        assert_eq!(Subject::Never, subject_of("g-1", Some("Group"), own));
        assert_eq!(Subject::Never, subject_of("Group/g-1", None, own));
        assert_eq!(
            Subject::Literal(String::from("http://elsewhere.invalid/Patient/9")),
            subject_of("http://elsewhere.invalid/Patient/9", None, own)
        );
    }
}
