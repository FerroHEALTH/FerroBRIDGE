// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A discriminator path: FHIRPath parsed by `ferrobridge-fhirpath` and held
//! to the simple subset R4 allows (<https://hl7.org/fhir/R4/fhirpath.html#simple>).

use ferrobridge_fhirpath::ast::Expr;
use ferrobridge_fhirpath::ast::Literal;

/// One step of a discriminator path inside the simple subset.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Step {
    /// A child element by name.
    Member(String),
    /// `extension('url')`: the extensions with that url.
    Extension(String),
    /// `ofType(T)` or `as(T)`: the items of type `T`.
    OfType(String),
    /// `resolve()`: the resource a reference points at.
    Resolve,
}

/// Parses `path` into its steps, `None` when it is outside the simple subset.
///
/// The subset is relative element names, `$this` at the start, and the
/// functions `resolve()`, `extension('url')`, `ofType(T)` and `as(T)`; any
/// operator, other function, literal or variable is outside it.
pub(crate) fn parse(path: &str) -> Option<Vec<Step>> {
    let expression = ferrobridge_fhirpath::parse::parse(path).ok()?;
    let mut steps = Vec::new();
    collect(&expression, &mut steps).then_some(steps)
}

/// Appends the steps of `expression` to `steps`, answering whether it is
/// inside the subset.
fn collect(expression: &Expr, steps: &mut Vec<Step>) -> bool {
    match expression {
        // `$this` stands only at the start of the path: the item itself.
        Expr::This => steps.is_empty(),
        Expr::Member { input, name } => {
            if let Some(input) = input
                && !collect(input, steps)
            {
                return false;
            }
            steps.push(Step::Member(name.clone()));
            true
        }
        Expr::Call { input, name, args } => {
            if let Some(input) = input
                && !collect(input, steps)
            {
                return false;
            }
            let step = match (name.as_str(), args.as_slice()) {
                ("resolve", []) => Step::Resolve,
                ("extension", [Expr::Literal(Literal::String(url))]) => {
                    Step::Extension(url.clone())
                }
                // NOTE: profiling.html §5.1.0.11 and fhirpath.html §2.9.2 name ofType(), the
                // ElementDefinition.slicing.discriminator.path comment names as(); both are read (#443).
                ("ofType" | "as", [argument]) => match argument.as_type_specifier() {
                    Some(specifier) => Step::OfType(specifier.name),
                    None => return false,
                },
                _ => return false,
            };
            steps.push(step);
            true
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::Step;
    use super::parse;

    #[test]
    fn the_subset_forms_parse_into_steps() {
        assert_eq!(
            parse("system"),
            Some(vec![Step::Member("system".to_owned())])
        );
        assert_eq!(parse("$this"), Some(Vec::new()));
        assert_eq!(
            parse("$this.resolve()"),
            Some(vec![Step::Resolve]),
            "resolve() from $this"
        );
        assert_eq!(
            parse("extension('http://x/e').value"),
            Some(vec![
                Step::Extension("http://x/e".to_owned()),
                Step::Member("value".to_owned())
            ])
        );
        assert_eq!(
            parse("value.ofType(Quantity)"),
            Some(vec![
                Step::Member("value".to_owned()),
                Step::OfType("Quantity".to_owned())
            ])
        );
        assert_eq!(
            parse("value.as(Quantity)"),
            Some(vec![
                Step::Member("value".to_owned()),
                Step::OfType("Quantity".to_owned())
            ])
        );
        assert_eq!(
            parse("code.coding.code"),
            Some(vec![
                Step::Member("code".to_owned()),
                Step::Member("coding".to_owned()),
                Step::Member("code".to_owned())
            ])
        );
    }

    #[test]
    fn a_path_outside_the_subset_is_refused() {
        assert_eq!(parse("value@$this.function"), None);
        assert_eq!(parse("code.where(system = 'x')"), None);
        assert_eq!(parse("a | b"), None);
        assert_eq!(parse("name.$this"), None);
        assert_eq!(parse("first()"), None);
    }
}
