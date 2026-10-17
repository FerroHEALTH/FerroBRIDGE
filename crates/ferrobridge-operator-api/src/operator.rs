// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The operator documents: the running load `GET /operator/state` answers,
//! the `GET /operator/mappings` listing, and the report of a check, a save
//! and a reload.
//!
//! A path in a document is a wire path (`fhirconnect/...`, `omocl/...`) or
//! a file name, never a host directory, and every diagnostic is the mapping
//! foundation's own [`Diagnostic`] in its serde form, its `file` the wire
//! path.

use openehr_mapping_core::diagnostic::Diagnostic;
use serde::{Deserialize, Serialize};

/// How much one load read and compiled.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Counts {
    /// The FHIRconnect files read.
    pub fhirconnect_files: usize,
    /// The FHIRconnect context files of a set that loaded.
    pub contexts: usize,
    /// The contexts that compiled into a program.
    pub programs: usize,
    /// The templates the contexts compiled against.
    pub templates: usize,
    /// The OMOCL files read.
    pub omocl_files: usize,
}

/// What `GET /operator/state` answers with: the running load.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EngineState {
    /// Each context's program.
    pub contexts: Vec<Program>,
    /// The templates the contexts compiled against.
    pub templates: Vec<Template>,
    /// The OMOCL files.
    pub omocl: Omocl,
    /// How much the load read and compiled.
    pub counts: Counts,
    /// Every diagnostic the load raised.
    pub diagnostics: Vec<Diagnostic>,
}

/// One context's program.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Program {
    /// The context's mapping name.
    pub context: String,
    /// The file it was read from, when the set names one.
    pub file: Option<String>,
    /// The template the program compiled against.
    pub template: String,
    /// The profile's canonical URL.
    pub profile: String,
    /// The profile's version.
    pub profile_version: String,
    /// The resource type the program writes.
    pub resource_type: String,
    /// The models the program reads with.
    pub models: Vec<Model>,
    /// The search declaration answering searches of the context, when one
    /// is declared.
    pub search: Option<String>,
}

/// One model of a program.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Model {
    /// The model's mapping name.
    pub name: String,
    /// The file it was read from, when the set names one.
    pub file: Option<String>,
    /// The extensions applied to it, in declaration order.
    pub extensions: Vec<Extension>,
}

/// One extension of a model.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Extension {
    /// The extension's mapping name.
    pub name: String,
    /// The file it was read from, when the set names one.
    pub file: Option<String>,
}

/// One template, by where it came from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "source", rename_all = "lowercase", deny_unknown_fields)]
pub enum Template {
    /// Read from the configured template directory.
    Directory {
        /// The template id.
        id: String,
        /// The file it was read from.
        file: String,
    },
    /// Fetched from the CDR.
    Cdr {
        /// The template id.
        id: String,
    },
}

/// The OMOCL files of the load.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Omocl {
    /// Their wire paths.
    pub files: Vec<String>,
    /// Whether none of them raised an error.
    pub valid: bool,
}

/// The language of the files under one mapping root.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Language {
    /// FHIRconnect model, extension, context and search files.
    Fhirconnect,
    /// OMOCL files.
    Omocl,
}

/// Who owns a mapping file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Owner {
    /// The deployment provisioned it; the operator API never writes it.
    Provisioned,
    /// The console saved it under `[operator] mapping_dir`.
    Console,
}

/// One mapping file as `GET /operator/mappings` lists it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Listed {
    /// The wire path.
    pub path: String,
    /// The language of its root.
    pub language: Language,
    /// The mapping type its header declares, `search` for a search
    /// declaration and `mapping` for an OMOCL file; absent for a file the
    /// running load did not read.
    pub kind: Option<String>,
    /// The lowercase hexadecimal SHA-256 of its bytes.
    pub sha256: String,
    /// Who owns it.
    pub owner: Owner,
    /// Whether the operator API refuses to write it.
    pub read_only: bool,
    /// The diagnostics the running load raised about it.
    pub diagnostics: Vec<Diagnostic>,
}

/// What a check, a save and a reload answer with: whether the set compiled,
/// how much it read, and every diagnostic.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LoadReport {
    /// Whether the set compiled with no error.
    pub clean: bool,
    /// How much the load read and compiled.
    pub counts: Counts,
    /// Every diagnostic, each file by its wire path.
    pub diagnostics: Vec<Diagnostic>,
}

#[cfg(test)]
mod tests {
    use super::{EngineState, LoadReport, Template};

    #[test]
    fn a_template_is_tagged_by_its_source() {
        let directory = Template::Directory {
            id: String::from("synthetic.v1"),
            file: String::from("t.opt"),
        };
        assert_eq!(
            serde_json::json!({"id": "synthetic.v1", "source": "directory", "file": "t.opt"}),
            serde_json::to_value(&directory).expect("serializes")
        );
        let cdr: Template =
            serde_json::from_value(serde_json::json!({"id": "synthetic.v2", "source": "cdr"}))
                .expect("reads");
        assert_eq!(
            Template::Cdr {
                id: String::from("synthetic.v2")
            },
            cdr
        );
    }

    #[test]
    fn a_document_reads_back_and_an_unknown_field_is_refused() {
        let text = serde_json::json!({
            "contexts": [], "templates": [], "omocl": {"files": [], "valid": true},
            "counts": {"fhirconnect_files": 0, "contexts": 0, "programs": 0, "templates": 0, "omocl_files": 0},
            "diagnostics": [{"file": "fhirconnect/a.yml", "line": 1, "column": 2, "severity": "error",
                "code": "missing-key", "message": "synthetic", "mapping": null, "model_path": []}]
        });
        let state: EngineState = serde_json::from_value(text.clone()).expect("reads");
        assert_eq!(text, serde_json::to_value(&state).expect("serializes"));
        let mut extra = text;
        extra["unknown"] = serde_json::json!(1);
        assert!(serde_json::from_value::<EngineState>(extra).is_err());
        let report = serde_json::json!({"clean": true, "counts": {"fhirconnect_files": 1, "contexts": 1, "programs": 1, "templates": 1, "omocl_files": 0}, "diagnostics": []});
        assert!(serde_json::from_value::<LoadReport>(report).is_ok());
    }
}
