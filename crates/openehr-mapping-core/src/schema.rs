// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The bridge between a positioned value tree and a JSON Schema validator.
//!
//! Both mapping languages validate their files against JSON Schema, and a
//! validator reads JSON, so each projects the loader's positioned tree into
//! `serde_json::Value` for the validation call and maps every error's instance
//! pointer back onto the tree to position its diagnostic. Both halves live
//! here once, with the draft-07 compilation of a schema document and the
//! validation step that turns each schema error into a diagnostic; nothing
//! downstream sees the projection.

use std::path::Path;

use crate::diagnostic::Diagnostic;
use crate::diagnostic::DiagnosticCode;
use crate::diagnostic::ModelPath;
use crate::position::Position;
use crate::value::MappingValue;
use crate::value::PositionedValue;

/// Projects a positioned tree into JSON for a validation call.
///
/// # Errors
///
/// Returns the position of the offending node when the tree holds a float JSON
/// cannot represent, which is the only value the projection can refuse.
pub fn to_json(node: &PositionedValue) -> Result<serde_json::Value, Position> {
    match *node.value() {
        MappingValue::Null => Ok(serde_json::Value::Null),
        MappingValue::Bool(value) => Ok(serde_json::Value::Bool(value)),
        MappingValue::Signed(value) => Ok(serde_json::Value::Number(value.into())),
        MappingValue::Unsigned(value) => Ok(serde_json::Value::Number(value.into())),
        MappingValue::Float(value) => serde_json::Number::from_f64(value.get())
            .map(serde_json::Value::Number)
            .ok_or_else(|| node.position()),
        MappingValue::Text(ref value) => Ok(serde_json::Value::String(value.clone())),
        MappingValue::Sequence(ref items) => items
            .iter()
            .map(to_json)
            .collect::<Result<Vec<serde_json::Value>, Position>>()
            .map(serde_json::Value::Array),
        MappingValue::Mapping(ref entries) => {
            let mut object = serde_json::Map::with_capacity(entries.len());
            for (key, entry) in entries {
                object.insert(key.clone(), to_json(entry.value())?);
            }
            Ok(serde_json::Value::Object(object))
        }
    }
}

/// Resolves a JSON Pointer against the positioned tree.
///
/// Returns the model path of the pointer and the position of the deepest node
/// it reaches, so a schema error about `/mappings/3/link` points at the `link`
/// key rather than at the document root. A mapping entry is positioned at its
/// key and a sequence item at the item. The pointer grammar is RFC 6901
/// (<https://www.rfc-editor.org/rfc/rfc6901>): `~1` is `/` and `~0` is `~`.
#[must_use]
pub fn locate(root: &PositionedValue, pointer: &str) -> (ModelPath, Position) {
    let mut path = ModelPath::root();
    let mut node = root;
    let mut position = root.position();
    for raw in pointer.split('/').skip(1) {
        let token = raw.replace("~1", "/").replace("~0", "~");
        match node.value() {
            MappingValue::Mapping(_) => {
                let Some(entry) = node.entry(&token) else {
                    break;
                };
                path = path.field(&token);
                position = entry.key_position();
                node = entry.value();
            }
            MappingValue::Sequence(items) => {
                let Ok(index) = token.parse::<usize>() else {
                    break;
                };
                let Some(item) = items.get(index) else {
                    break;
                };
                path = path.index(index);
                position = item.position();
                node = item;
            }
            _ => break,
        }
    }
    (path, position)
}

/// Why a JSON Schema document could not be compiled into a validator.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum SchemaError {
    /// The schema document is not JSON.
    #[error("the {schema} schema is not JSON")]
    Json {
        /// The schema's name, as the message renders it.
        schema: &'static str,
        /// The underlying failure.
        #[source]
        source: serde_json::Error,
    },
    /// The schema document is JSON but is not a usable JSON Schema.
    #[error("the {schema} schema does not compile")]
    Compile {
        /// The schema's name, as the message renders it.
        schema: &'static str,
        /// The underlying failure.
        #[source]
        source: Box<jsonschema::ValidationError<'static>>,
    },
}

/// Compiles the schema document `source`, named `schema` in an error, as
/// draft-07, the draft every schema of both languages declares in its
/// `$schema` keyword.
///
/// # Errors
///
/// Returns [`SchemaError::Json`] when the document is not JSON and
/// [`SchemaError::Compile`] when it is not a usable draft-07 schema.
pub fn compile(source: &str, schema: &'static str) -> Result<jsonschema::Validator, SchemaError> {
    let document: serde_json::Value =
        serde_json::from_str(source).map_err(|source| SchemaError::Json { schema, source })?;
    jsonschema::draft7::options()
        .build(&document)
        .map_err(|source| SchemaError::Compile {
            schema,
            source: Box::new(source),
        })
}

/// Validates the positioned `document` of `file` against `validator`.
///
/// The returned list is empty when the document validates, and otherwise
/// carries one diagnostic per schema error under `code`, its message the
/// error after `label`, positioned at the YAML node the error is about and
/// carrying its model path. A document holding a number JSON cannot represent
/// is one diagnostic at that number.
#[must_use]
pub fn validate(
    validator: &jsonschema::Validator,
    file: &Path,
    document: &PositionedValue,
    code: &DiagnosticCode,
    label: &str,
) -> Vec<Diagnostic> {
    let instance = match to_json(document) {
        Ok(instance) => instance,
        Err(position) => {
            return vec![
                Diagnostic::error(
                    file,
                    code.clone(),
                    "the document holds a number JSON cannot represent, so it cannot be \
                     validated against a JSON Schema",
                )
                .with_position(position),
            ];
        }
    };
    validator
        .iter_errors(&instance)
        .map(|error| {
            let pointer = error.instance_path().to_string();
            let (path, position) = locate(document, &pointer);
            Diagnostic::error(file, code.clone(), format!("{label}: {error}"))
                .with_position(position)
                .with_model_path(path)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::locate;
    use super::to_json;
    use crate::loader::parse_str;
    use crate::position::Position;

    const SOURCE: &str = "root:\n  items:\n    - a: 1\n    - b~c/d: true\n";

    #[test]
    fn a_tree_projects_into_the_json_it_holds() {
        let tree = parse_str("t.yml", SOURCE).expect("well-formed YAML");
        let json = to_json(&tree).expect("no unrepresentable float");
        assert_eq!(
            json,
            serde_json::json!({"root": {"items": [{"a": 1}, {"b~c/d": true}]}})
        );
    }

    #[test]
    fn a_pointer_resolves_to_its_key_and_item_positions() {
        let tree = parse_str("t.yml", SOURCE).expect("well-formed YAML");
        let (path, position) = locate(&tree, "/root/items/1/b~0c~1d");
        assert_eq!(path.to_string(), "root.items[1].b~c/d");
        assert_eq!(position, Position::new(4, 7));
    }

    #[test]
    fn a_pointer_past_the_tree_stops_at_the_deepest_node() {
        let tree = parse_str("t.yml", SOURCE).expect("well-formed YAML");
        let (path, position) = locate(&tree, "/root/items/7");
        assert_eq!(path.to_string(), "root.items");
        assert_eq!(position, Position::new(2, 3));
    }
}
