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

use crate::ast::{BinaryOp, Expr, Literal, TypeSpecifier};
use crate::model::{FhirType, Model, choice_code, field_type, is_a, named_type};
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
}

/// The FHIR types an expression can reach; `None` when unknown.
type Reach = Option<Vec<FhirType>>;

/// Checks `expr` with a context of type `context`.
///
/// # Errors
///
/// Returns the first [`SemanticError`] found.
pub fn check(expr: &Expr, model: &dyn Model, context: FhirType) -> Result<(), SemanticError> {
    let reach = Some(vec![context]);
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
                let (a, b) = (
                    self.infer(left, focus, this)?,
                    self.infer(right, focus, this)?,
                );
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

    /// What the member `name` reaches from `from`.
    fn member(&self, from: &Reach, name: &str, at_root: bool) -> Result<Reach, SemanticError> {
        let Some(types) = from else {
            return Ok(None);
        };
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
                        let reached_type =
                            field_type(self.model, field, *kind).map(|found| {
                                match (kind, choice_code(field, suffix)) {
                                    (Kind::Complex(_), _) | (_, None) => found,
                                    (_, Some(code)) => FhirType {
                                        name: code,
                                        ..found
                                    },
                                }
                            });
                        match reached_type {
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
        let mut outputs = Vec::new();
        for (index, arg) in args.iter().enumerate() {
            let typed_argument = matches!(name, "ofType" | "as" | "is") && index == 0;
            if typed_argument {
                if let Some(specifier) = arg.as_type_specifier() {
                    self.resolve(&specifier)?;
                }
                continue;
            }
            let reach = if lambdas.contains(&index) {
                self.infer(arg, from, from)?
            } else {
                self.infer(arg, focus, this)?
            };
            outputs.push(reach);
        }
        let first = outputs.first().cloned().flatten();
        Ok(match name {
            "where" | "first" | "last" | "tail" | "skip" | "take" | "single" | "distinct"
            | "intersect" | "exclude" | "trace" | "checkModifiers" | "slice" => from.clone(),
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
