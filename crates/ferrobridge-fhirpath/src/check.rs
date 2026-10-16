// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A static check of the paths an expression navigates, against the type its
//! context will have (N1 §11).
//!
//! The check follows the FHIR types a path can reach, a choice element to
//! every type it admits, and stops at what N1 §11 says a compiler cannot type
//! (`children()`, `descendants()`, `resolve()`, a resource-typed element) and
//! at every system value. A member none of the reachable types defines, and a
//! leading type name that is not the context's, are refused before any
//! document is read. Anything the check cannot type passes, and the
//! evaluation's own checks apply.
//!
//! A type test of the focus narrows what the focus reaches where the test's
//! outcome is known: the right operand of `and` and `implies` sees the test
//! true, the right operand of `or` sees it false, the second argument of
//! `iif()` true and the third false, and `where()` answers the items its
//! criterion keeps. Where no type is left the operand cannot be reached and
//! nothing in it is refused.

use crate::ast::{BinaryOp, Expr, Literal, TypeSpecifier};
use crate::model::{FhirType, Model, field_type, is_a, named_type, variant_type};
use fhir_types::schema::Kind;

/// A path the model refuses.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SemanticError {
    /// A member none of the reachable types defines.
    #[error("{type_name} has no element {element}")]
    UnknownElement {
        /// The reachable types.
        type_name: String,
        /// The member name.
        element: String,
    },
    /// A leading type name that is not the context's type.
    #[error("the expression names the type {named}, the context is a {context}")]
    ContextType {
        /// The type the expression names.
        named: String,
        /// The context's type.
        context: String,
    },
    /// A type specifier that names no type.
    #[error("{name} names no type of the model")]
    UnknownType {
        /// The specifier as written.
        name: String,
    },
    /// A criterion that is a literal of a type other than Boolean, which
    /// `iif()` does not take (N1 §5.5.1, §11).
    #[error("{function}() takes a Boolean criterion, the expression gives a literal {found}")]
    Criterion {
        /// The function.
        function: &'static str,
        /// The literal's type.
        found: &'static str,
    },
    /// A function given an input none of whose reachable types it takes
    /// (N1 §11 "an input value ... not of the expected type").
    #[error("{function}() takes no {type_name}")]
    Input {
        /// The function.
        function: &'static str,
        /// The reachable types.
        type_name: String,
    },
}

/// The FHIR types an expression can reach; `None` when unknown, and none
/// at all where a type guard leaves the expression unreachable.
type Reach = Option<Vec<FhirType>>;

/// Checks `expr` with a context of type `context`.
///
/// # Errors
///
/// Returns the first [`SemanticError`] found.
pub fn check(expr: &Expr, model: &dyn Model, context: FhirType) -> Result<(), SemanticError> {
    check_any(expr, model, &[context])
}

/// Checks `expr` with a context that may hold any of the types `contexts`,
/// as an occurrence of a choice element does (R4 §2.9.1.1): a member is
/// accepted when one of them defines it.
///
/// # Errors
///
/// Returns the first [`SemanticError`] found.
pub fn check_any(
    expr: &Expr,
    model: &dyn Model,
    contexts: &[FhirType],
) -> Result<(), SemanticError> {
    // NOTE: N1 §11 lets an implementation choose its compile-time strictness, and lists a
    // choice[x] member as one a compiler cannot type, so the union of its types is the bound.
    let reach = Some(contexts.to_vec());
    Checker { model }.infer(expr, &reach, &reach).map(|_| ())
}

/// The checker over one model.
struct Checker<'m> {
    model: &'m dyn Model,
}

/// The union of two reaches.
fn union(a: Reach, b: Reach) -> Reach {
    let (mut a, b) = (a?, b?);
    for ty in b {
        if !a.contains(&ty) {
            a.push(ty);
        }
    }
    Some(a)
}

impl Checker<'_> {
    /// What `expr` reaches from `focus`, with `$this` reaching `this`.
    fn infer(&self, expr: &Expr, focus: &Reach, this: &Reach) -> Result<Reach, SemanticError> {
        match expr {
            Expr::Literal(_) | Expr::Empty | Expr::External(_) | Expr::Index | Expr::Total => {
                Ok(None)
            }
            Expr::This => Ok(this.clone()),
            Expr::Member { input, name } => {
                let from = match input {
                    Some(input) => self.infer(input, focus, this)?,
                    None => focus.clone(),
                };
                self.member(&from, name, input.is_none())
            }
            Expr::Call { input, name, args } => {
                let from = match input {
                    Some(input) => self.infer(input, focus, this)?,
                    None => focus.clone(),
                };
                self.call(&from, name, args, focus, this)
            }
            Expr::Invocation { input, .. } => {
                self.infer(input, focus, this)?;
                Ok(None)
            }
            Expr::Indexer { input, index } => {
                self.infer(index, focus, this)?;
                self.infer(input, focus, this)
            }
            Expr::Negate(operand) | Expr::Plus(operand) => {
                self.infer(operand, focus, this)?;
                Ok(None)
            }
            Expr::Binary { op, left, right } => {
                let a = self.infer(left, focus, this)?;
                let b = match op {
                    BinaryOp::And | BinaryOp::Implies => self.infer(
                        right,
                        &self.narrow(left, true, focus),
                        &self.narrow(left, true, this),
                    )?,
                    BinaryOp::Or => self.infer(
                        right,
                        &self.narrow(left, false, focus),
                        &self.narrow(left, false, this),
                    )?,
                    _ => self.infer(right, focus, this)?,
                };
                Ok(if *op == BinaryOp::Union {
                    union(a, b)
                } else {
                    None
                })
            }
            Expr::Is {
                operand,
                type_specifier,
            } => {
                self.infer(operand, focus, this)?;
                self.resolve(type_specifier)?;
                Ok(None)
            }
            Expr::As {
                operand,
                type_specifier,
            } => {
                self.infer(operand, focus, this)?;
                self.resolve(type_specifier)
            }
        }
    }

    /// The FHIR type a specifier names, `None` for a system type.
    fn resolve(&self, specifier: &TypeSpecifier) -> Result<Reach, SemanticError> {
        let system = crate::value::SYSTEM_TYPES.contains(&specifier.name.as_str());
        let fhir = || named_type(self.model, &specifier.name);
        match specifier.namespace.as_deref() {
            None | Some("FHIR") if fhir().is_some() => Ok(fhir().map(|ty| vec![ty])),
            None | Some("System") if system => Ok(None),
            _ => Err(SemanticError::UnknownType {
                name: specifier.to_string(),
            }),
        }
    }

    /// The types `reach` keeps when the Boolean `guard` is `assume`.
    ///
    /// A guard is a type test of the focus (`$this is T`, `$this.is(T)`,
    /// `is(T)`), its `not()`, or an `and` or `or` of guards; a test of any
    /// other expression, or of a system type, tells nothing.
    fn narrow(&self, guard: &Expr, assume: bool, reach: &Reach) -> Reach {
        match guard {
            Expr::Is {
                operand,
                type_specifier,
            } if matches!(operand.as_ref(), Expr::This) => {
                self.narrow_to(type_specifier, assume, reach)
            }
            Expr::Call { input, name, args }
                if name == "is" && input.as_deref().is_none_or(|i| matches!(i, Expr::This)) =>
            {
                match args.as_slice() {
                    [arg] => match arg.as_type_specifier() {
                        Some(specifier) => self.narrow_to(&specifier, assume, reach),
                        None => reach.clone(),
                    },
                    _ => reach.clone(),
                }
            }
            Expr::Call {
                input: Some(inner),
                name,
                args,
            } if name == "not" && args.is_empty() => self.narrow(inner, !assume, reach),
            Expr::Binary {
                op: BinaryOp::And,
                left,
                right,
            } => {
                if assume {
                    self.narrow(right, true, &self.narrow(left, true, reach))
                } else {
                    union(
                        self.narrow(left, false, reach),
                        self.narrow(right, false, reach),
                    )
                }
            }
            Expr::Binary {
                op: BinaryOp::Or,
                left,
                right,
            } => {
                if assume {
                    union(
                        self.narrow(left, true, reach),
                        self.narrow(right, true, reach),
                    )
                } else {
                    self.narrow(right, false, &self.narrow(left, false, reach))
                }
            }
            _ => reach.clone(),
        }
    }

    /// The types `reach` keeps when the focus `is` the type `specifier`
    /// names (`assume`), or is not (N1 §6.3.1: the type or a subclass).
    fn narrow_to(&self, specifier: &TypeSpecifier, assume: bool, reach: &Reach) -> Reach {
        let types = reach.as_ref()?;
        let Ok(Some(named)) = self.resolve(specifier) else {
            return reach.clone();
        };
        let Some(target) = named.first() else {
            return reach.clone();
        };
        let mut kept: Vec<FhirType> = Vec::new();
        for ty in types {
            let found = if is_a(self.model, ty, target.name) {
                assume.then_some(*ty)
            } else if assume && is_a(self.model, target, ty.name) {
                Some(*target)
            } else if assume {
                None
            } else {
                Some(*ty)
            };
            if let Some(found) = found
                && !kept.contains(&found)
            {
                kept.push(found);
            }
        }
        Some(kept)
    }

    /// What the member `name` reaches from `from`.
    fn member(&self, from: &Reach, name: &str, at_root: bool) -> Result<Reach, SemanticError> {
        let Some(types) = from else {
            return Ok(None);
        };
        if types.is_empty() {
            return Ok(Some(Vec::new()));
        }
        let mut reached: Vec<FhirType> = Vec::new();
        let mut known = true;
        let mut defined = false;
        for ty in types {
            if at_root && !ty.is_primitive() && is_a(self.model, ty, name) {
                defined = true;
                reached.push(*ty);
                continue;
            }
            let schema = match ty.schema() {
                Some(schema) => Some(schema),
                None => self.model.schemas().type_named("Element"),
            };
            let Some(field) =
                schema.and_then(|schema| schema.fields.iter().find(|field| field.name == name))
            else {
                continue;
            };
            defined = true;
            match field.kind {
                Kind::Choice(variants) => {
                    for (suffix, kind) in variants {
                        match variant_type(self.model, field, suffix, *kind) {
                            Some(found) if !reached.contains(&found) => reached.push(found),
                            Some(_) => {}
                            None => known = false,
                        }
                    }
                }
                kind => match field_type(self.model, field, kind) {
                    Some(found) if !reached.contains(&found) => reached.push(found),
                    Some(_) => {}
                    None => known = false,
                },
            }
        }
        if !defined {
            let names = types
                .iter()
                .map(|ty| ty.name)
                .collect::<Vec<_>>()
                .join(" | ");
            if at_root && named_type(self.model, name).is_some() {
                return Err(SemanticError::ContextType {
                    named: String::from(name),
                    context: names,
                });
            }
            return Err(SemanticError::UnknownElement {
                type_name: names,
                element: String::from(name),
            });
        }
        Ok(known.then_some(reached))
    }

    /// What the function `name` reaches from `from`, its arguments checked.
    fn call(
        &self,
        from: &Reach,
        name: &str,
        args: &[Expr],
        focus: &Reach,
        this: &Reach,
    ) -> Result<Reach, SemanticError> {
        let lambdas: &[usize] = match name {
            "where" | "exists" | "all" | "select" | "repeat" | "aggregate" => &[0],
            "trace" => &[1],
            "iif" => &[0, 1, 2],
            _ => &[],
        };
        if name == "iif"
            && let Some(Expr::Literal(literal)) = args.first()
        {
            let found = match literal {
                Literal::Boolean(_) => None,
                Literal::String(_) => Some("String"),
                Literal::Integer(_) => Some("Integer"),
                Literal::Decimal(_) => Some("Decimal"),
                Literal::Date(_) => Some("Date"),
                Literal::DateTime(_) => Some("DateTime"),
                Literal::Time(_) => Some("Time"),
                Literal::Quantity(..) => Some("Quantity"),
            };
            if let Some(found) = found {
                return Err(SemanticError::Criterion {
                    function: "iif",
                    found,
                });
            }
        }
        if name == "toString"
            && let Some(types) = from
            && !types.is_empty()
            && types
                .iter()
                .all(|ty| !ty.is_primitive() && !is_a(self.model, ty, "Quantity"))
        {
            // NOTE: N1 §5.5.8 toString() takes a primitive or a Quantity (R4 §2.9.1.3); N1 §11
            // lets a compiler refuse an input no reachable type of which it takes.
            return Err(SemanticError::Input {
                function: "toString",
                type_name: types
                    .iter()
                    .map(|ty| ty.name)
                    .collect::<Vec<_>>()
                    .join(" | "),
            });
        }
        let mut outputs = Vec::new();
        for (index, arg) in args.iter().enumerate() {
            let typed_argument = matches!(name, "ofType" | "as" | "is") && index == 0;
            if typed_argument {
                if let Some(specifier) = arg.as_type_specifier() {
                    self.resolve(&specifier)?;
                }
                continue;
            }
            let reach = if name == "iif" && index > 0 {
                let guarded = match args.first() {
                    Some(criterion) => self.narrow(criterion, index == 1, from),
                    None => from.clone(),
                };
                self.infer(arg, &guarded, &guarded)?
            } else if lambdas.contains(&index) {
                self.infer(arg, from, from)?
            } else {
                self.infer(arg, focus, this)?
            };
            outputs.push(reach);
        }
        let first = outputs.first().cloned().flatten();
        Ok(match name {
            "where" => match args.first() {
                Some(criterion) => self.narrow(criterion, true, from),
                None => from.clone(),
            },
            "first" | "last" | "tail" | "skip" | "take" | "single" | "distinct" | "intersect"
            | "exclude" | "trace" | "checkModifiers" | "slice" => from.clone(),
            "select" => first,
            "union" | "combine" => union(from.clone(), first),
            "extension" => named_type(self.model, "Extension").map(|ty| vec![ty]),
            "ofType" | "as" => match args.first().and_then(Expr::as_type_specifier) {
                Some(specifier) => self.resolve(&specifier)?,
                None => None,
            },
            _ => None,
        })
    }
}
