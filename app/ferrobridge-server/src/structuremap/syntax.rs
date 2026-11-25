// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A map as the FHIR Mapping Language writes it: the abstract syntax of the
//! R4 grammar (`mapping.g4`), which the `StructureMap` resource carries in
//! its own form (FHIR R4 §7.7, structuremap.html).

/// One map: `map "url" = name`, its structures, imports and groups.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StructureMap {
    /// The canonical URL.
    pub url: String,
    /// The name.
    pub name: String,
    /// The structure definitions it uses.
    pub uses: Vec<Uses>,
    /// The maps it imports, a URL that may hold a `*` wildcard.
    pub imports: Vec<String>,
    /// The groups, the first one the default entry.
    pub groups: Vec<Group>,
}

/// `uses "url" alias name as mode`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Uses {
    /// The structure definition's canonical URL.
    pub url: String,
    /// The name the map refers to its type by, when it is not the
    /// definition's own name.
    pub alias: Option<String>,
    /// How the map uses it.
    pub mode: ModelMode,
}

/// The `StructureMapModelMode` of a used structure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelMode {
    /// Instances passed in, read from.
    Source,
    /// Instances the map may ask for.
    Queried,
    /// Instances passed in, written to.
    Target,
    /// Instances the map may create.
    Produced,
}

/// One group of rules.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Group {
    /// The name the group is invoked by.
    pub name: String,
    /// The input variables, in order.
    pub parameters: Vec<Parameter>,
    /// The group whose rules also apply.
    pub extends: Option<String>,
    /// Whether the group is a default for its type pair.
    pub type_mode: Option<TypeMode>,
    /// The rules.
    pub rules: Vec<Rule>,
}

/// One input variable of a group: `source src : Type`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Parameter {
    /// Source or target.
    pub mode: InputMode,
    /// The variable name.
    pub name: String,
    /// The type, when declared.
    pub type_name: Option<String>,
}

/// The `StructureMapInputMode` of a group input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputMode {
    /// Read from.
    Source,
    /// Written to.
    Target,
}

/// The default-group stereotype of a group.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TypeMode {
    /// `<<types>>`: the default for its source and target types.
    Types,
    /// `<<type+>>`: also the default when the target type is open.
    TypePlus,
}

/// One rule: sources, targets, dependent rules and a name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rule {
    /// The name, when written.
    pub name: Option<String>,
    /// The source statements.
    pub sources: Vec<Source>,
    /// The target statements.
    pub targets: Vec<Target>,
    /// The rules or group applied in the rule's scope.
    pub dependent: Option<Dependent>,
}

/// One source statement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Source {
    /// The variable the statement reads.
    pub context: String,
    /// The child element it selects, when one is named.
    pub element: Option<String>,
    /// The type an element must have to be selected.
    pub type_name: Option<String>,
    /// The cardinality the selection must have, `None` for `*`.
    pub cardinality: Option<(u32, Option<u32>)>,
    /// The value used when nothing is selected.
    pub default: Option<Expression>,
    /// Which occurrences the rule applies to.
    pub list_mode: Option<SourceListMode>,
    /// The variable each occurrence is bound to.
    pub variable: Option<String>,
    /// The condition an occurrence must meet to be selected.
    pub condition: Option<Expression>,
    /// The condition every selected occurrence must meet, or the map fails.
    pub check: Option<Expression>,
    /// The expression the engine traces.
    pub log: Option<Expression>,
}

/// The `StructureMapSourceListMode` of a source statement.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceListMode {
    /// Only the first occurrence.
    First,
    /// Every occurrence but the first.
    NotFirst,
    /// Only the last occurrence.
    Last,
    /// Every occurrence but the last.
    NotLast,
    /// The one occurrence; more than one fails the map.
    OnlyOne,
}

/// One target statement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Target {
    /// The variable written into, absent for a bare invocation.
    pub context: Option<String>,
    /// The child element written, when one is named.
    pub element: Option<String>,
    /// How the value is made; absent for an auto-created element.
    pub transform: Option<Transform>,
    /// The variable the made value is bound to.
    pub variable: Option<String>,
    /// How repeated applications share the made value.
    pub list_mode: Option<TargetListMode>,
}

/// The `StructureMapTargetListMode` of a target statement.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TargetListMode {
    /// Only on the first application.
    First,
    /// One value shared by every application.
    Share,
    /// Only on the last application.
    Last,
    /// Every application adds to one collated value.
    Collate,
}

/// How a target value is made.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Transform {
    /// A constant.
    Literal(Literal),
    /// A copy of a variable, or of one of its elements.
    Copy {
        /// The variable.
        context: String,
        /// The element, when one is named.
        element: Option<String>,
    },
    /// A named transform.
    Invocation(Invocation),
    /// The `(expression)` shorthand of `evaluate`.
    Evaluate(Expression),
}

/// A call: `name(param, ...)`, a transform or a group.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Invocation {
    /// The transform or group name.
    pub name: String,
    /// The parameters.
    pub params: Vec<Param>,
}

/// One parameter of an invocation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Param {
    /// A constant.
    Literal(Literal),
    /// A variable, by name.
    Variable(String),
}

/// A literal, as FHIRPath writes it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Literal {
    /// An integer, in its lexical form.
    Integer(String),
    /// A decimal, in its lexical form.
    Decimal(String),
    /// A string, unescaped.
    String(String),
    /// A date or date-time, without its `@`.
    DateTime(String),
    /// A time, without its `@T`.
    Time(String),
    /// A boolean.
    Boolean(bool),
}

/// What a rule applies in its scope.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Dependent {
    /// A group, with the variables passed to it.
    Invocation(Invocation),
    /// Nested rules.
    Rules(Vec<Rule>),
}

/// A FHIRPath expression, as written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Expression(pub String);
