// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! What each operation is asked: the request, and what it runs against.
//!
//! The operations framework invokes an operation on a type
//! (`[base]/ValueSet/$validate-code`) or on one instance
//! (`[base]/ValueSet/[id]/$validate-code`)
//! (<https://hl7.org/fhir/R4/operations.html#invoking>); at the type level the
//! canonical travels as an input, at the instance level the address names the
//! resource and no canonical is sent.

use crate::concept::Concept;

/// The resource an operation runs against.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub enum Target {
    /// The resource whose canonical URL this is, sent as the operation's
    /// `url` (or `system` for `$subsumes`) at the type level.
    Canonical(String),
    /// The stored resource with this logical id, invoked at the instance
    /// level, so the address carries the id and no canonical is sent.
    Instance(String),
}

impl Target {
    /// Returns the canonical, `None` for an instance.
    #[must_use]
    pub fn canonical(&self) -> Option<&str> {
        match self {
            Self::Canonical(url) => Some(url),
            Self::Instance(_) => None,
        }
    }

    /// Returns the logical id, `None` for a canonical.
    #[must_use]
    pub fn instance(&self) -> Option<&str> {
        match self {
            Self::Canonical(_) => None,
            Self::Instance(id) => Some(id),
        }
    }
}

/// The value set `ValueSet/$expand` expands.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub enum ValueSetSource {
    /// A value set the server holds, by canonical or by id.
    Stored(Target),
    /// A value set sent in the request as the `valueSet` input, which is a
    /// resource, so the request is a `POST`
    /// (<https://hl7.org/fhir/R4/operations.html#request>).
    Inline(InlineValueSet),
}

impl ValueSetSource {
    /// Returns the stored value set whose canonical URL is `url`.
    #[must_use]
    pub fn canonical(url: impl Into<String>) -> Self {
        Self::Stored(Target::Canonical(url.into()))
    }
}

/// A value set written by the caller, as its `compose.include` entries.
///
/// The client writes it as the release's own `ValueSet` with `status`
/// `active`, the one status a request-scoped value set can carry, since
/// `ValueSet.status` is `1..1` (<https://hl7.org/fhir/R4/valueset.html>).
#[derive(Debug, Clone, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct InlineValueSet {
    /// `compose.include`, in order.
    pub include: Vec<Include>,
}

/// One `ValueSet.compose.include`: codes of one system, selected by filter.
#[derive(Debug, Clone, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Include {
    /// `include.system`.
    pub system: String,
    /// `include.version`, when one version of the system is meant.
    pub version: Option<String>,
    /// `include.filter`, every one of which a code must pass.
    pub filters: Vec<Filter>,
}

/// One `ValueSet.compose.include.filter`.
#[derive(Debug, Clone, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Filter {
    /// `filter.property`.
    pub property: String,
    /// `filter.op`, a `FilterOperator` code such as `is-a` or `child-of`.
    pub op: String,
    /// `filter.value`.
    pub value: String,
}

/// One `CodeSystem/$validate-code` question: is the code in the code system.
///
/// The operation takes the code system by `url` or at the instance level, a
/// `code`, and the `display` the caller asserts
/// (<https://hl7.org/fhir/R4/codesystem-operation-validate-code.html>). STU3
/// defines no such operation, so an STU3 client refuses it before sending.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CodeSystemValidation {
    /// The code system, by canonical (`url`) or by id.
    pub code_system: Target,
    /// `version`, the version of the code system.
    pub version: Option<String>,
    /// `code`.
    pub code: String,
    /// `display`, the display the caller asserts, which the server checks.
    pub display: Option<String>,
    /// `displayLanguage`, the language `display` is asserted in.
    pub display_language: Option<String>,
    /// `date`, the date the validation is wanted for, as a FHIR `dateTime`.
    pub date: Option<String>,
}

impl CodeSystemValidation {
    /// Returns the question whether `code` is in `code_system`, with no other
    /// input set yet.
    #[must_use]
    pub fn new(code_system: Target, code: impl Into<String>) -> Self {
        Self {
            code_system,
            version: None,
            code: code.into(),
            display: None,
            display_language: None,
            date: None,
        }
    }
}

/// One operation to send, for a single call or as a batch entry.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Request {
    /// `CodeSystem/$lookup`.
    Lookup {
        /// The code system the code is in.
        system: String,
        /// The code to look up.
        code: String,
        /// The code system version, when the source data named one.
        version: Option<String>,
        /// `date`, the date the information is wanted for, as a FHIR
        /// `dateTime`, when the caller asks for the code system as it stood
        /// then.
        date: Option<String>,
        /// `displayLanguage`, the language the displays are wanted in.
        display_language: Option<String>,
    },
    /// `ConceptMap/$translate`.
    Translate(Translation),
    /// `ValueSet/$validate-code`.
    ValidateCode(Validation),
    /// `CodeSystem/$validate-code`.
    CodeSystemValidateCode(CodeSystemValidation),
    /// `ValueSet/$expand`.
    Expand(Expansion),
    /// `CodeSystem/$subsumes`.
    Subsumes(Subsumption),
}

impl Request {
    /// Returns the `CodeSystem/$lookup` of `code` in `system`, at `version`
    /// and `date` when given, with no display language.
    #[must_use]
    pub fn lookup(system: &str, code: &str, version: Option<&str>, date: Option<&str>) -> Self {
        Self::Lookup {
            system: system.to_owned(),
            code: code.to_owned(),
            version: version.map(str::to_owned),
            date: date.map(str::to_owned),
            display_language: None,
        }
    }

    /// Returns the operation, as `Resource/$code`.
    #[must_use]
    pub const fn operation(&self) -> &'static str {
        match self {
            Self::Lookup { .. } => crate::LOOKUP,
            Self::Translate(_) => crate::TRANSLATE,
            Self::ValidateCode(_) => crate::VALIDATE_CODE,
            Self::CodeSystemValidateCode(_) => crate::CODE_SYSTEM_VALIDATE_CODE,
            Self::Expand(_) => crate::EXPAND,
            Self::Subsumes(_) => crate::SUBSUMES,
        }
    }

    /// Returns the stored resource the request is invoked on, `None` at the
    /// type level.
    #[must_use]
    pub fn instance(&self) -> Option<&str> {
        match self {
            Self::Lookup { .. } | Self::Translate(_) => None,
            Self::ValidateCode(validation) => validation.value_set.instance(),
            Self::CodeSystemValidateCode(validation) => validation.code_system.instance(),
            Self::Expand(expansion) => match &expansion.value_set {
                ValueSetSource::Stored(target) => target.instance(),
                ValueSetSource::Inline(_) => None,
            },
            Self::Subsumes(subsumption) => subsumption.code_system.instance(),
        }
    }

    /// Returns the path segments the request is invoked at, relative to the
    /// service base: `[type, $code]` or `[type, id, $code]`
    /// (<https://hl7.org/fhir/R4/operations.html#invoking>).
    #[must_use]
    pub fn segments(&self) -> Vec<&str> {
        let (resource, code) = self
            .operation()
            .split_once('/')
            .unwrap_or((self.operation(), ""));
        match self.instance() {
            Some(id) => vec![resource, id, code],
            None => vec![resource, code],
        }
    }
}

/// One `ConceptMap/$translate` question: what `code` from `system` maps to.
///
/// Each input is `0..1` (<https://hl7.org/fhir/R4/conceptmap-operation-translate.html>)
/// and is sent only when it is set here, under the name the release's own
/// `OperationDefinition` gives it.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Translation {
    /// The code system the code is in: `system` up to R5, `sourceSystem` in
    /// R6.
    pub system: String,
    /// The code to translate: `code` in STU3 to R4B, `sourceCode` from R5.
    pub code: String,
    /// The version of the system, if one was provided in the source data:
    /// `version` up to R5, `sourceVersion` in R6.
    pub version: Option<String>,
    /// `url`, the canonical URL of the concept map to use, when one is named.
    ///
    /// STU3 `$translate` has no `url` parameter, so an STU3 client refuses a
    /// translation that names one.
    pub concept_map_url: Option<String>,
    /// `conceptMapVersion`, the business version of the concept map. STU3
    /// declares no such input.
    pub concept_map_version: Option<String>,
    /// The code system to translate into: `targetsystem` up to R4B,
    /// `targetSystem` from R5.
    pub target_system: Option<String>,
    /// `reverse`: "if this is true, then the operation should return all the
    /// codes that might be mapped to this code". R5 replaced it with the
    /// `target*` inputs, so an R5 or R6 client refuses it.
    pub reverse: Option<bool>,
}

impl Translation {
    /// Returns the translation of `code` from `system`, naming no concept
    /// map, no target system and no direction yet.
    #[must_use]
    pub fn new(system: impl Into<String>, code: impl Into<String>) -> Self {
        Self {
            system: system.into(),
            code: code.into(),
            version: None,
            concept_map_url: None,
            concept_map_version: None,
            target_system: None,
            reverse: None,
        }
    }

    /// Names the version of the code system.
    #[must_use]
    pub fn with_version(mut self, version: impl Into<String>) -> Self {
        self.version = Some(version.into());
        self
    }

    /// Names the concept map, sent as `url`.
    #[must_use]
    pub fn with_concept_map(mut self, url: impl Into<String>) -> Self {
        self.concept_map_url = Some(url.into());
        self
    }

    /// Names the concept map's business version, sent as
    /// `conceptMapVersion`.
    #[must_use]
    pub fn with_concept_map_version(mut self, version: impl Into<String>) -> Self {
        self.concept_map_version = Some(version.into());
        self
    }

    /// Names the code system to translate into.
    #[must_use]
    pub fn with_target_system(mut self, system: impl Into<String>) -> Self {
        self.target_system = Some(system.into());
        self
    }

    /// Sets `reverse`, which asks for the codes that map to this one.
    #[must_use]
    pub const fn with_reverse(mut self, reverse: bool) -> Self {
        self.reverse = Some(reverse);
        self
    }
}

/// One `ValueSet/$validate-code` question: is the coded value a member of
/// the value set.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Validation {
    /// The value set, by canonical (`url`, without a version) or by id.
    pub value_set: Target,
    /// `valueSetVersion`, the version of the value set, when the binding
    /// names one.
    ///
    /// STU3 `$validate-code` has no such parameter, so an STU3 client
    /// refuses a validation that names one.
    pub value_set_version: Option<String>,
    /// `date`, the date the validation is wanted for, as a FHIR `dateTime`,
    /// when the caller asks for the value set and code system as they stood
    /// then.
    pub date: Option<String>,
    /// The coded value, in one of the forms the operation accepts.
    pub coded: Coded,
    /// `displayLanguage`, the language the asserted display is in.
    pub display_language: Option<String>,
    /// `activeOnly`, whether an inactive code counts as not valid
    /// (<https://hl7.org/fhir/uv/tx-ecosystem/requirements.html>). STU3
    /// declares no such input.
    pub active_only: Option<bool>,
}

impl Validation {
    /// Returns the question whether `coded` is in the value set whose
    /// canonical URL is `value_set_url`, with no other input set yet.
    #[must_use]
    pub fn new(value_set_url: impl Into<String>, coded: Coded) -> Self {
        Self {
            value_set: Target::Canonical(value_set_url.into()),
            value_set_version: None,
            date: None,
            coded,
            display_language: None,
            active_only: None,
        }
    }
}

/// The coded value `ValueSet/$validate-code` tests, in the input forms the
/// operation declares
/// (<https://hl7.org/fhir/R4/valueset-operation-validate-code.html>).
///
/// R4 and R4B read "If a code is provided, a system or a context must be
/// provided" (`OperationDefinition-ValueSet-validate-code`, `code`), so a
/// bare code carries one of the two by construction: [`Coded::Code`] its
/// system, [`Coded::InContext`] its context.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub enum Coded {
    /// The `code`, `system`, `systemVersion` and `display` parameters. "If a
    /// code is provided, a system or a context must be provided", so the
    /// system is mandatory here.
    Code {
        /// `system`.
        system: String,
        /// `code`.
        code: String,
        /// `systemVersion`, the version of the system, if one was provided in
        /// the source data. STU3 names this parameter `version`.
        version: Option<String>,
        /// `display`, the display associated with the code, if provided.
        display: Option<String>,
    },
    /// The `code`, `context` and `display` parameters: a code whose system
    /// the context implies, such as an element bound to a value set.
    ///
    /// `context` is "the context of the value set, so that the server can
    /// resolve this to a value set to validate against", recommended as
    /// `[Structure Definition URL]#[name or path into structure definition]`.
    /// STU3 declares `context` but reads "If a code is provided, a system
    /// must be provided" (`hl7.fhir.r3.core` 3.0.2), so an STU3 client
    /// refuses this form with
    /// [`Error::SystemRequired`](crate::error::Error::SystemRequired) and
    /// sends nothing.
    InContext {
        /// `context`.
        context: String,
        /// `code`.
        code: String,
        /// `display`, the display associated with the code, if provided.
        display: Option<String>,
    },
    /// The `coding` parameter.
    Coding(Concept),
    /// The `codeableConcept` parameter: "the server returns true if one of the
    /// coding values is in the value set".
    CodeableConcept {
        /// `CodeableConcept.coding`.
        coding: Vec<Concept>,
        /// `CodeableConcept.text`.
        text: Option<String>,
    },
}

/// One `ValueSet/$expand` question: the codes the value set holds.
///
/// Every input used here is `0..1`
/// (<https://hl7.org/fhir/R4/valueset-operation-expand.html>), so each is
/// sent only when it is set.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Expansion {
    /// The value set: stored (by `url` or by id) or sent inline as
    /// `valueSet`.
    pub value_set: ValueSetSource,
    /// `valueSetVersion`, the version of the value set, when one is named.
    ///
    /// STU3 `$expand` has no such parameter, so an STU3 client refuses an
    /// expansion that names one.
    pub value_set_version: Option<String>,
    /// `date`, the date the expansion is wanted for, as a FHIR `dateTime`.
    pub date: Option<String>,
    /// `filter`, the text filter the server applies to the codes it returns.
    pub filter: Option<String>,
    /// `offset`, the number of codes to skip in a paged flat expansion.
    pub offset: Option<u32>,
    /// `count`, the number of codes a page holds; `0` asks for the size of
    /// the expansion alone.
    pub count: Option<u32>,
    /// `includeDesignations`, whether the codes carry their designations.
    pub include_designations: Option<bool>,
    /// `activeOnly`, whether inactive codes are left out.
    pub active_only: Option<bool>,
    /// `excludeNested`, whether the expansion is flat.
    pub exclude_nested: Option<bool>,
    /// `displayLanguage`, the language the displays are wanted in.
    pub display_language: Option<String>,
}

impl Expansion {
    /// Returns the expansion of the stored value set whose canonical URL is
    /// `value_set_url`, with no other input set yet.
    #[must_use]
    pub fn new(value_set_url: impl Into<String>) -> Self {
        Self::of(ValueSetSource::canonical(value_set_url))
    }

    /// Returns the expansion of `value_set`, with no other input set yet.
    #[must_use]
    pub const fn of(value_set: ValueSetSource) -> Self {
        Self {
            value_set,
            value_set_version: None,
            date: None,
            filter: None,
            offset: None,
            count: None,
            include_designations: None,
            active_only: None,
            exclude_nested: None,
            display_language: None,
        }
    }

    /// Names the version of the value set, sent as `valueSetVersion`.
    #[must_use]
    pub fn with_version(mut self, version: impl Into<String>) -> Self {
        self.value_set_version = Some(version.into());
        self
    }

    /// Names the date the expansion is wanted for, sent as `date`.
    #[must_use]
    pub fn with_date(mut self, date: impl Into<String>) -> Self {
        self.date = Some(date.into());
        self
    }

    /// Sets the text filter, sent as `filter`.
    #[must_use]
    pub fn with_filter(mut self, filter: impl Into<String>) -> Self {
        self.filter = Some(filter.into());
        self
    }

    /// Asks for one page, sent as `offset` and `count`.
    #[must_use]
    pub const fn with_page(mut self, offset: u32, count: u32) -> Self {
        self.offset = Some(offset);
        self.count = Some(count);
        self
    }

    /// Sets `includeDesignations`.
    #[must_use]
    pub const fn with_designations(mut self, include: bool) -> Self {
        self.include_designations = Some(include);
        self
    }
}

/// One `CodeSystem/$subsumes` question: how `code_a` relates to `code_b` in
/// one code system.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Subsumption {
    /// The code system, by canonical (`system`) or by id.
    pub code_system: Target,
    /// `codeA`.
    pub code_a: String,
    /// `codeB`.
    pub code_b: String,
    /// `version`, the version of the code system, when one is named.
    pub version: Option<String>,
}

impl Subsumption {
    /// Returns the question of how `code_a` relates to `code_b` in the code
    /// system whose canonical URL is `system`.
    #[must_use]
    pub fn new(
        system: impl Into<String>,
        code_a: impl Into<String>,
        code_b: impl Into<String>,
    ) -> Self {
        Self::on(Target::Canonical(system.into()), code_a, code_b)
    }

    /// Returns the question of how `code_a` relates to `code_b` in
    /// `code_system`.
    #[must_use]
    pub fn on(code_system: Target, code_a: impl Into<String>, code_b: impl Into<String>) -> Self {
        Self {
            code_system,
            code_a: code_a.into(),
            code_b: code_b.into(),
            version: None,
        }
    }

    /// Names the version of the code system, sent as `version`.
    #[must_use]
    pub fn with_version(mut self, version: impl Into<String>) -> Self {
        self.version = Some(version.into());
        self
    }
}

#[cfg(test)]
mod tests {
    use super::{Expansion, Request, Subsumption, Target};

    #[test]
    fn a_request_names_its_operation() {
        let lookup = Request::Lookup {
            system: "http://example.org/cs".to_owned(),
            code: "a".to_owned(),
            version: None,
            date: None,
            display_language: None,
        };
        assert_eq!("CodeSystem/$lookup", lookup.operation());
        assert_eq!(
            "ValueSet/$expand",
            Request::Expand(Expansion::new("http://example.org/vs")).operation()
        );
        assert_eq!(
            "CodeSystem/$subsumes",
            Request::Subsumes(Subsumption::new("http://example.org/cs", "a", "b")).operation()
        );
    }

    #[test]
    fn an_instance_request_is_invoked_on_its_id() {
        let at_type = Request::Subsumes(Subsumption::new("http://example.org/cs", "a", "b"));
        assert_eq!(vec!["CodeSystem", "$subsumes"], at_type.segments());
        let on_instance = Request::Subsumes(Subsumption::on(
            Target::Instance("cs-1".to_owned()),
            "a",
            "b",
        ));
        assert_eq!(
            vec!["CodeSystem", "cs-1", "$subsumes"],
            on_instance.segments()
        );
    }
}
