// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The 13 published contexts of the FHIRconnect mapping library, each
//! compiled verbatim against the operational template it names.
//!
//! A context loads beside every model file of the library and the files of
//! its own project directory. A file the loader refuses is left out of the
//! set and named in the verdict, so a context that needs it fails on the
//! reference it can no longer resolve. The verdicts are pinned whole; a
//! context that starts or stops compiling is a finding to adjudicate against
//! the FHIRconnect specification
//! (`docs/specs/fhirconnect/modules/ROOT/pages/types-of-mapping-files/context-mappings.adoc`).
//! The library defects behind each refusal are recorded on #101.

use core::error::Error;
use core::fmt::Write as _;
use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::fs;
use std::path::Path;
use std::path::PathBuf;

use fhir_types::r4::schema::SCHEMAS;
use fhirconnect::model::load::load_set;
use fhirconnect::model::semantic::StaticMappingCodes;
use fhirconnect::resolve::compile::compile;
use openehr_mapping_core::diagnostic::Diagnostic;
use openehr_mapping_core::index::WebTemplateIndex;
use openehr_mapping_core::loader::load_str;
use openehr_mapping_core::template::TemplateSource;

use crate::support::OwnedChain;

/// The vendored mapping library, relative to this crate's manifest.
const CORPUS: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../docs/specs/fhirconnect-mapping-lib"
);

/// The vendored operational templates, relative to this crate's manifest.
const TEMPLATES: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tools/ferrobridge-testkit/fixtures/opt"
);

/// Where an operational template is read from.
#[derive(Debug, Clone, Copy)]
enum Opt {
    /// A template under the testkit's vendored fixtures.
    Testkit(&'static str),
    /// A template the mapping library ships beside its context.
    Library(&'static str),
}

/// Every published context, by path relative to the library, with the
/// template that carries the `template.id` it names.
const CONTEXTS: &[(&str, Opt)] = &[
    (
        "projects/org.highmed/KDS/diagnose/KDS_diagnose.context.yaml",
        Opt::Testkit("kds/KDS_Diagnose.opt"),
    ),
    (
        "projects/org.highmed/KDS/fall/KDS_fall_einfach.context.yaml",
        Opt::Testkit("kds-fall-einfach/KDS_Fall_einfach.opt"),
    ),
    (
        "projects/org.highmed/KDS/laborauftrag/KDS_laborauftrag.context.yaml",
        Opt::Testkit("kds-laborauftrag/KDS_Laborauftrag.opt"),
    ),
    (
        "projects/org.highmed/KDS/laborbericht/KDS_laborbericht.context.yaml",
        Opt::Testkit("kds-laborbericht/KDS_Laborbericht.opt"),
    ),
    (
        "projects/org.highmed/KDS/medikationseintrag/KDS_medikationseintrag.context.yaml",
        Opt::Testkit("kds-medikationseintrag/KDS_Medikationseintrag.opt"),
    ),
    (
        "projects/org.highmed/KDS/medikationsverabreichung/KDS_medikationsverabreichung.context.yaml",
        Opt::Testkit("kds-medikamentenverabreichungen/KDS_Medikamentenverabreichungen.opt"),
    ),
    (
        "projects/org.highmed/KDS/person/person.context.yaml",
        Opt::Testkit("kds-person/KDS_Person.opt"),
    ),
    (
        "projects/org.highmed/KDS/person_pseudo/pseudo_person.context.yaml",
        Opt::Testkit("kds-person-pseudonymisiert/KDS_Person_Pseudonymisiert.opt"),
    ),
    (
        "projects/org.highmed/KDS/procedure/procedure.context.yaml",
        Opt::Testkit("kds-prozedur/KDS_Prozedur.opt"),
    ),
    (
        "projects/org.highmed/KDS/studienteilnahme/studienteilnahme.context.yaml",
        Opt::Testkit("kds-studienteilnahme/Studienteilnahme.opt"),
    ),
    (
        "projects/org.highmed/KDS/todesursache/KDS_Todesursache.context.yaml",
        Opt::Testkit("kds-todesursache/KDS_Todesursache.opt"),
    ),
    (
        "projects/org.highmed/KDS/vitalstatus/KDS_Vitalstatus.context.yaml",
        Opt::Testkit("kds-vitalstatus/KDS_Vitalstatus.opt"),
    ),
    (
        "projects/org.openehr/EEHRxF/lab/bundle/lab.context.yml",
        Opt::Library("projects/org.openehr/EEHRxF/lab/resources/EHDS - Laboratory report.opt"),
    ),
];

/// What one published context came to.
#[derive(Debug)]
struct Verdict {
    /// The context, relative to the library.
    context: String,
    /// The files the loader refused, left out of the set, each with the
    /// first code raised against it.
    dropped: BTreeMap<String, String>,
    /// The program, or the refusals as `file:line code` rows.
    outcome: Result<OwnedChain, Vec<String>>,
}

/// Returns the absolute path of a file of the library.
fn library(relative: &str) -> PathBuf {
    PathBuf::from(CORPUS).join(relative)
}

/// Returns `path` relative to the library, or as written when it lies
/// outside it.
fn relative(path: &Path) -> String {
    path.strip_prefix(CORPUS)
        .unwrap_or(path)
        .to_string_lossy()
        .trim_start_matches('/')
        .replace('\\', "/")
}

/// Walks `directory`, collecting every YAML file relative to the library.
fn yaml_under(directory: &Path, found: &mut Vec<String>) -> Result<(), Box<dyn Error>> {
    for entry in fs::read_dir(directory)? {
        let path = entry?.path();
        if path.is_dir() {
            yaml_under(&path, found)?;
            continue;
        }
        let is_yaml = path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extension == "yml" || extension == "yaml");
        if is_yaml {
            found.push(relative(&path));
        }
    }
    Ok(())
}

/// Renders diagnostics as sorted, distinct `file:line code` rows.
fn rows(diagnostics: &[Diagnostic]) -> Vec<String> {
    let found: BTreeSet<String> = diagnostics
        .iter()
        .map(|diagnostic| {
            let line = diagnostic
                .position()
                .map_or(0, openehr_mapping_core::position::Position::line);
            format!(
                "{}:{line} {}",
                relative(diagnostic.file()),
                diagnostic.code()
            )
        })
        .collect();
    found.into_iter().collect()
}

/// Builds the Web Template index over a context's template.
fn template(opt: Opt) -> Result<WebTemplateIndex, Box<dyn Error>> {
    let parsed = openehr_its::opt14::from_xml(&template_xml(opt)?)?;
    Ok(WebTemplateIndex::build(&TemplateSource::Opt14(Box::new(
        parsed,
    )))?)
}

/// Returns the text of a context's template.
fn template_xml(opt: Opt) -> Result<String, Box<dyn Error>> {
    let path = match opt {
        Opt::Testkit(file) => PathBuf::from(TEMPLATES).join(file),
        Opt::Library(file) => library(file),
    };
    Ok(fs::read_to_string(path)?)
}

/// Returns the first `template_id` value an operational template declares.
fn declared_template_id(xml: &str) -> Option<&str> {
    let (_, after) = xml.split_once("<template_id>")?;
    let (_, value) = after.split_once("<value>")?;
    let (id, _) = value.split_once("</value>")?;
    Some(id)
}

/// Loads and compiles one published context against its template.
fn verdict(context: &str, opt: Opt) -> Result<Verdict, Box<dyn Error>> {
    let mut files = Vec::new();
    yaml_under(&library("model"), &mut files)?;
    let directory = Path::new(context)
        .parent()
        .ok_or("a context path has a directory")?;
    yaml_under(&library(&directory.to_string_lossy()), &mut files)?;
    files.sort();
    files.dedup();

    let mut dropped = BTreeMap::new();
    let set = loop {
        match load_set(
            files.iter().map(|file| library(file)),
            &StaticMappingCodes::default(),
        ) {
            Ok(set) => break set,
            Err(diagnostics) => {
                let refused: BTreeMap<String, String> =
                    diagnostics
                        .iter()
                        .fold(BTreeMap::new(), |mut refused, diagnostic| {
                            refused
                                .entry(relative(diagnostic.file()))
                                .or_insert_with(|| diagnostic.code().to_string());
                            refused
                        });
                let stuck = refused.keys().any(|file| !files.contains(file));
                let only_context = refused.keys().all(|file| file == context);
                if only_context || stuck {
                    return Ok(Verdict {
                        context: context.to_owned(),
                        dropped,
                        outcome: Err(rows(&diagnostics)),
                    });
                }
                let mut refused = refused;
                refused.remove(context);
                files.retain(|file| !refused.contains_key(file));
                dropped.extend(refused);
            }
        }
    };

    let source = fs::read_to_string(library(context))?;
    let name = load_str(context, &source)?.header().name().value().clone();
    let index = match template(opt) {
        Ok(index) => index,
        Err(error) => {
            return Ok(Verdict {
                context: context.to_owned(),
                dropped,
                outcome: Err(vec![format!("the template does not build: {error}")]),
            });
        }
    };
    let outcome = compile(
        &set,
        &name,
        &index,
        &SCHEMAS,
        &StaticMappingCodes::default(),
    )
    .map(|program| (files, program))
    .map_err(|diagnostics| rows(&diagnostics));
    Ok(Verdict {
        context: context.to_owned(),
        dropped,
        outcome,
    })
}

/// Returns the verdict on every published context, in library order.
fn verdicts() -> Result<Vec<Verdict>, Box<dyn Error>> {
    CONTEXTS
        .iter()
        .map(|(context, opt)| verdict(context, *opt))
        .collect()
}

/// Returns every published context that compiles, with the library files
/// its set loaded, relative to the library.
pub(crate) fn published_chains() -> Result<Vec<OwnedChain>, Box<dyn Error>> {
    Ok(verdicts()?
        .into_iter()
        .filter_map(|verdict| verdict.outcome.ok())
        .collect())
}

/// Renders the verdicts as the pinned text.
fn render(verdicts: &[Verdict]) -> Result<String, Box<dyn Error>> {
    let mut text = String::new();
    for verdict in verdicts {
        writeln!(text, "{}", verdict.context)?;
        for (file, code) in &verdict.dropped {
            writeln!(text, "  left out {file} ({code})")?;
        }
        match &verdict.outcome {
            Ok((_, program)) => writeln!(
                text,
                "  compiles: {} against {}",
                program.context().as_str(),
                program.template().id().as_str()
            )?,
            Err(refusals) => {
                writeln!(text, "  refused")?;
                for row in refusals {
                    writeln!(text, "    {row}")?;
                }
            }
        }
    }
    Ok(text)
}

#[test]
fn every_published_context_names_a_vendored_template() -> Result<(), Box<dyn Error>> {
    for (context, opt) in CONTEXTS {
        let source = fs::read_to_string(library(context))?;
        let document = load_str(*context, &source)?;
        let declared = document
            .document()
            .get("context")
            .and_then(|context| context.get("template"))
            .and_then(|template| template.get("id"))
            .and_then(|id| id.as_text())
            .ok_or_else(|| format!("{context} names no template id"))?
            .to_owned();
        let xml = template_xml(*opt)?;
        assert_eq!(
            declared_template_id(&xml),
            Some(declared.as_str()),
            "{context}"
        );
    }
    Ok(())
}

/// The EHDS laboratory report template the library ships constrains a
/// `DV_IDENTIFIER` with an empty default value, which the `openehr-its`
/// OPT 1.4 reader refuses for its missing `id`, so the template is refused
/// before any mapping meets it.
#[test]
fn the_ehds_laboratory_template_is_refused_for_an_empty_identifier_default()
-> Result<(), Box<dyn Error>> {
    let Some((_, opt)) = CONTEXTS
        .iter()
        .find(|(context, _)| context.ends_with("lab.context.yml"))
    else {
        return Err("the EHDS laboratory context is listed".into());
    };
    let Err(error) = openehr_its::opt14::from_xml(&template_xml(*opt)?) else {
        return Err("the EHDS laboratory template is expected not to parse".into());
    };
    let message = format!("{error:?}");
    assert!(message.contains("DV_IDENTIFIER"), "{message}");
    assert!(message.contains("child: \"id\""), "{message}");
    Ok(())
}

#[test]
fn every_vendored_kds_template_builds_a_web_template() -> Result<(), Box<dyn Error>> {
    for (context, opt) in CONTEXTS {
        if let Opt::Testkit(_) = opt {
            let index = template(*opt).map_err(|error| format!("{context}: {error}"))?;
            assert_eq!(
                Some(index.template_id()),
                declared_template_id(&template_xml(*opt)?),
                "{context}"
            );
        }
    }
    Ok(())
}

#[test]
fn the_published_contexts_hold_their_pinned_verdicts() -> Result<(), Box<dyn Error>> {
    let mut files = Vec::new();
    yaml_under(&library("projects"), &mut files)?;
    let contexts: BTreeSet<&str> = files
        .iter()
        .map(String::as_str)
        .filter(|file| file.contains(".context."))
        .collect();
    let listed: BTreeSet<&str> = CONTEXTS.iter().map(|(context, _)| *context).collect();
    assert_eq!(
        contexts, listed,
        "the library's contexts and the table differ"
    );
    insta::assert_snapshot!("published_contexts", render(&verdicts()?)?);
    Ok(())
}
