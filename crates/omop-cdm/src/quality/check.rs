// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The ported checks and one instance of a check.
//!
//! [`CheckName`] is the closed set of the Dashboard's checks the port runs;
//! everything the catalogue says about a check (its spelling, level, Kahn
//! classification and SQL template) is read from the generated catalogue.
//! [`Check`] is one check for one table or one field, with the parameters,
//! threshold and notes it runs with.

use crate::generated::dqd;
use crate::meta::{ColumnMeta, TableMeta};
use crate::quality::definitions::{
    CheckDescription, CheckLevel, KahnCategory, KahnContext, KahnSubcategory, Percentage,
};

/// A check of the Data Quality Dashboard the port runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub enum CheckName {
    /// `cdmTable`: the table exists.
    CdmTable,
    /// `cdmField`: the field exists.
    CdmField,
    /// `isRequired`: a required field holds no NULL.
    IsRequired,
    /// `cdmDatatype`: an integer field holds only integers.
    CdmDatatype,
    /// `isPrimaryKey`: a primary-key field holds no duplicate.
    IsPrimaryKey,
    /// `isForeignKey`: a foreign-key field names an existing row.
    IsForeignKey,
    /// `fkDomain`: a concept field names a concept of its domain.
    FkDomain,
    /// `fkClass`: a concept field names a concept of its class.
    FkClass,
    /// `isStandardValidConcept`: a concept field names a standard, valid
    /// concept.
    IsStandardValidConcept,
}

impl CheckName {
    /// Every check the port runs, in catalogue order.
    pub const ALL: [Self; 9] = [
        Self::CdmTable,
        Self::CdmField,
        Self::IsRequired,
        Self::CdmDatatype,
        Self::IsPrimaryKey,
        Self::IsForeignKey,
        Self::FkDomain,
        Self::FkClass,
        Self::IsStandardValidConcept,
    ];

    /// Returns the check's record in the generated catalogue.
    #[must_use]
    pub fn description(self) -> &'static CheckDescription {
        match self {
            Self::CdmTable => &dqd::CDM_TABLE,
            Self::CdmField => &dqd::CDM_FIELD,
            Self::IsRequired => &dqd::IS_REQUIRED,
            Self::CdmDatatype => &dqd::CDM_DATATYPE,
            Self::IsPrimaryKey => &dqd::IS_PRIMARY_KEY,
            Self::IsForeignKey => &dqd::IS_FOREIGN_KEY,
            Self::FkDomain => &dqd::FK_DOMAIN,
            Self::FkClass => &dqd::FK_CLASS,
            Self::IsStandardValidConcept => &dqd::IS_STANDARD_VALID_CONCEPT,
        }
    }

    /// Returns the check's name as the Dashboard spells it, for example
    /// `isStandardValidConcept`.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        self.description().name
    }

    /// Returns the level the check runs at.
    #[must_use]
    pub fn level(self) -> CheckLevel {
        self.description().level
    }

    /// Returns the check's Kahn context.
    #[must_use]
    pub fn context(self) -> KahnContext {
        self.description().kahn_context
    }

    /// Returns the check's Kahn category.
    #[must_use]
    pub fn category(self) -> KahnCategory {
        self.description().kahn_category
    }

    /// Returns the check's Kahn subcategory, when the catalogue names one.
    #[must_use]
    pub fn subcategory(self) -> Option<KahnSubcategory> {
        self.description().kahn_subcategory
    }

    /// Returns the Dashboard's SQL template the check translates.
    #[must_use]
    pub fn sql_file(self) -> &'static str {
        self.description().sql_file
    }

    /// Returns whether the check probes for a table or field rather than
    /// counting rows, so that a missing one fails the check instead of being
    /// an error.
    #[must_use]
    pub fn is_probe(self) -> bool {
        matches!(self, Self::CdmTable | Self::CdmField)
    }
}

/// What a check compares a field against, beyond the field itself.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum CheckParameters {
    /// The check reads only its table or field.
    Plain,
    /// The table and column a foreign key references (`isForeignKey`).
    ForeignKey {
        /// The referenced table.
        table: &'static str,
        /// The referenced column.
        field: &'static str,
    },
    /// The vocabulary domains a concept may belong to (`fkDomain`), the
    /// definitions' `fkDomain` cell split on commas.
    Domains(&'static [&'static str]),
    /// The concept class a concept must belong to (`fkClass`).
    ConceptClass(&'static str),
}

/// One check's subject and parameters, each check's shape a variant of its
/// own so a field check always has its column.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Kind {
    /// `cdmTable`.
    CdmTable,
    /// `cdmField` on the column.
    CdmField(&'static ColumnMeta),
    /// `isRequired` on the column.
    IsRequired(&'static ColumnMeta),
    /// `cdmDatatype` on the column.
    CdmDatatype(&'static ColumnMeta),
    /// `isPrimaryKey` on the column.
    IsPrimaryKey(&'static ColumnMeta),
    /// `isForeignKey` on the column, referencing `table.field`.
    IsForeignKey {
        /// The checked column.
        column: &'static ColumnMeta,
        /// The referenced table.
        table: &'static str,
        /// The referenced column.
        field: &'static str,
    },
    /// `fkDomain` on the column.
    FkDomain {
        /// The checked column.
        column: &'static ColumnMeta,
        /// The domains a concept may belong to.
        domains: &'static [&'static str],
    },
    /// `fkClass` on the column.
    FkClass {
        /// The checked column.
        column: &'static ColumnMeta,
        /// The class a concept must belong to.
        class: &'static str,
    },
    /// `isStandardValidConcept` on the column.
    IsStandardValidConcept(&'static ColumnMeta),
}

/// One check for one table or one field.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Check {
    table: &'static TableMeta,
    kind: Kind,
    threshold: Option<Percentage>,
    notes: Option<&'static str>,
}

impl Check {
    /// Creates the check `kind` on `table`.
    pub(crate) fn new(
        table: &'static TableMeta,
        kind: Kind,
        threshold: Option<Percentage>,
        notes: Option<&'static str>,
    ) -> Self {
        Self {
            table,
            kind,
            threshold,
            notes,
        }
    }

    /// Returns the check's subject and parameters.
    pub(crate) fn kind(&self) -> &Kind {
        &self.kind
    }

    /// Returns which check this is.
    #[must_use]
    pub fn name(&self) -> CheckName {
        match self.kind {
            Kind::CdmTable => CheckName::CdmTable,
            Kind::CdmField(_) => CheckName::CdmField,
            Kind::IsRequired(_) => CheckName::IsRequired,
            Kind::CdmDatatype(_) => CheckName::CdmDatatype,
            Kind::IsPrimaryKey(_) => CheckName::IsPrimaryKey,
            Kind::IsForeignKey { .. } => CheckName::IsForeignKey,
            Kind::FkDomain { .. } => CheckName::FkDomain,
            Kind::FkClass { .. } => CheckName::FkClass,
            Kind::IsStandardValidConcept(_) => CheckName::IsStandardValidConcept,
        }
    }

    /// Returns the table the check reads.
    #[must_use]
    pub fn table(&self) -> &'static TableMeta {
        self.table
    }

    /// Returns the column the check reads, for a field check.
    #[must_use]
    pub fn column(&self) -> Option<&'static ColumnMeta> {
        match self.kind {
            Kind::CdmTable => None,
            Kind::CdmField(column)
            | Kind::IsRequired(column)
            | Kind::CdmDatatype(column)
            | Kind::IsPrimaryKey(column)
            | Kind::IsForeignKey { column, .. }
            | Kind::FkDomain { column, .. }
            | Kind::FkClass { column, .. }
            | Kind::IsStandardValidConcept(column) => Some(column),
        }
    }

    /// Returns what the check compares the field against.
    #[must_use]
    pub fn parameters(&self) -> CheckParameters {
        match &self.kind {
            Kind::IsForeignKey { table, field, .. } => CheckParameters::ForeignKey { table, field },
            Kind::FkDomain { domains, .. } => CheckParameters::Domains(domains),
            Kind::FkClass { class, .. } => CheckParameters::ConceptClass(class),
            Kind::CdmTable
            | Kind::CdmField(_)
            | Kind::IsRequired(_)
            | Kind::CdmDatatype(_)
            | Kind::IsPrimaryKey(_)
            | Kind::IsStandardValidConcept(_) => CheckParameters::Plain,
        }
    }

    /// Returns the threshold the check is evaluated against, when the
    /// Dashboard's settings carry one.
    #[must_use]
    pub fn threshold(&self) -> Option<Percentage> {
        self.threshold
    }

    /// Returns the Dashboard's notes for the check, when its settings carry
    /// any.
    #[must_use]
    pub fn notes(&self) -> Option<&'static str> {
        self.notes
    }

    /// Returns the table name as the Dashboard reports it, upper case.
    #[must_use]
    pub fn cdm_table_name(&self) -> String {
        self.table.name.to_uppercase()
    }

    /// Returns the field name as the Dashboard reports it, upper case, for a
    /// field check.
    #[must_use]
    pub fn cdm_field_name(&self) -> Option<String> {
        self.column().map(|column| column.name.to_uppercase())
    }

    /// Returns the check id the Dashboard gives this check, for example
    /// `field_isrequired_person_person_id`.
    ///
    /// The id is the level, the check name, the table and the field, each
    /// without spaces, the empty ones left out, joined by `_` and lower-cased
    /// (DQD v2.9.0 `R/getCheckId.R` lines 32 to 51).
    #[must_use]
    pub fn id(&self) -> String {
        let field = self.cdm_field_name();
        [
            Some(self.name().level().as_str()),
            Some(self.name().as_str()),
            Some(self.table.name),
            field.as_deref(),
        ]
        .into_iter()
        .flatten()
        .map(|part| part.replace(' ', ""))
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("_")
        .to_lowercase()
    }

    /// Returns the catalogue's description of the check with its placeholders
    /// filled in.
    ///
    /// Every placeholder takes the upper-cased value of the matching setting
    /// (DQD v2.9.0 `R/recordResult.R` lines 45 to 49 and 59): `@cdmTableName`
    /// the table, and for a field check `@cdmFieldName` the field and
    /// `@fkTableName`, `@fkFieldName`, `@fkDomain` and `@fkClass` the field's
    /// definitions, empty where the definitions name none.
    #[must_use]
    pub fn description(&self) -> String {
        let mut text = self
            .name()
            .description()
            .description
            .replace("@cdmTableName", &self.cdm_table_name());
        if let Some(column) = self.column() {
            let upper = |value: Option<&str>| value.map(str::to_uppercase).unwrap_or_default();
            let (fk_table, fk_field) = column.foreign_key.unzip();
            for (placeholder, value) in [
                ("@cdmFieldName", upper(Some(column.name))),
                ("@fkTableName", upper(fk_table)),
                ("@fkFieldName", upper(fk_field)),
                ("@fkDomain", column.fk_domain.join(", ").to_uppercase()),
                ("@fkClass", upper(column.fk_class)),
            ] {
                text = text.replace(placeholder, &value);
            }
        }
        text
    }
}
