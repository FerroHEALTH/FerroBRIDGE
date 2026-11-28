// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The MyHealth@EU binding sets held against their guides and the value
//! sets they bind, read from the packages `scripts/vendor/profile-packages.sh`
//! fetches: each binding a set carries is a `required` binding of its guide
//! whose value set draws on exactly the systems the binding names, and each
//! Laboratory 9.2.0 binding left out draws on SNOMED CT, the system the source
//! records it in. The value sets resolve through MyHealth@EU Core 1.0.1 and
//! the Master Value Sets Catalogue 9.1.0 the guides depend on. The Imaging
//! 9.1.1 guide is read from its committed tarball, and its one required
//! binding left out on a written element draws on SNOMED CT too.

#![expect(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]

use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::error::Error;
use std::io::Read;
use std::path::PathBuf;

use eehrxf::terminology::ehn;
use eehrxf::terminology::translation::Binding;
use eehrxf::terminology::translation::MYHEALTH_EU_CORE;
use eehrxf::terminology::translation::MYHEALTH_EU_LABORATORY;
use flate2::read::GzDecoder;
use serde_json::Value;

use crate::support::profile_set;

/// The guide.
const LABORATORY: &str = "myhealth.eu.fhir.laboratory#9.2.0";

/// MyHealth@EU Core, the guide `MYHEALTH_EU_CORE` is held to.
const CORE: &str = "myhealth.eu.fhir.core#1.0.1";

/// The packages the guides' value sets resolve in.
const VALUE_SETS: [&str; 2] = [
    "myhealth.eu.fhir.core#1.0.1",
    "myhealth.eu.fhir.mvc-package#9.1.0",
];

/// The code system of the exceptional values every `WithExceptions` value set
/// adds, in which no source records a concept.
const NULL_FLAVOR: &str = "http://terminology.hl7.org/CodeSystem/v3-NullFlavor";

/// The guide's required bindings on elements the shipped mappings write that
/// the set leaves out, each element with its value set.
const LEFT_OUT: [(&str, &str); 5] = [
    ("Observation.value[x]", "eHDSIResultsCodedValueLaboratory"),
    ("Observation.method", "eHDSILabTechniqueWithExceptions"),
    ("Specimen.type", "eHDSILabSpecimenTypeWithExceptions"),
    (
        "Specimen.collection.bodySite",
        "eHDSIBodySiteWithExceptions",
    ),
    (
        "ServiceRequest.reasonCode",
        "eHDSIOrderReasonWithExceptions",
    ),
];

/// The MyHealth@EU Imaging 9.1.1 CI build, relative to `docs/specs/`,
/// committed outside the fetched package set because its dependency
/// `hl7.fhir.eu.imaging` 1.0.0-alpha is published nowhere.
const IMAGING: &str = "ehdsi-imaging/myhealth.eu.fhir.imaging-9.1.1.tgz";

/// The imaging guide's required binding on an element the component writes
/// that no set carries, with its value set. Its
/// `ImagingStudy.series.laterality` and `ServiceRequest` bindings sit on
/// elements the imaging mappings do not write, and its `modality` bindings
/// are extensible.
const IMAGING_LEFT_OUT: [(&str, &str); 1] = [(
    "ImagingStudy.series.bodySite",
    "eHDSIBodySiteWithExceptions",
)];

/// The root the packages are fetched under.
fn packages() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../docs/specs/fhir-packages")
}

/// Whether `name` is the file name of a JSON resource of `kind`.
fn is_resource(name: &str, kind: &str) -> bool {
    name.starts_with(&format!("{kind}-"))
        && std::path::Path::new(name)
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("json"))
}

/// Every JSON resource of `kind` in the committed tarball [`IMAGING`].
fn imaging_resources(kind: &str) -> Result<Vec<Value>, Box<dyn Error>> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../docs/specs")
        .join(IMAGING);
    let mut archive = tar::Archive::new(GzDecoder::new(std::fs::File::open(path)?));
    let mut found = Vec::new();
    for entry in archive.entries()? {
        let mut entry = entry?;
        let named = entry
            .path()?
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| is_resource(name, kind));
        if named {
            let mut text = String::new();
            entry.read_to_string(&mut text)?;
            found.push(serde_json::from_str(&text)?);
        }
    }
    Ok(found)
}

/// Every JSON resource of `kind` in `package`.
fn resources(package: &str, kind: &str) -> Result<Vec<Value>, Box<dyn Error>> {
    let mut found = Vec::new();
    for entry in std::fs::read_dir(packages().join(package).join("package"))? {
        let path = entry?.path();
        let named = path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| is_resource(name, kind));
        if named {
            found.push(serde_json::from_str(&std::fs::read_to_string(&path)?)?);
        }
    }
    Ok(found)
}

/// The `required` bindings of the guide `package`, by element path, each
/// with its value set.
fn required_bindings(package: &str) -> Result<BTreeSet<(String, String)>, Box<dyn Error>> {
    Ok(required(resources(package, "StructureDefinition")?))
}

/// The `required` bindings the differentials of `profiles` declare, by
/// element path, each with its value set.
fn required(profiles: Vec<Value>) -> BTreeSet<(String, String)> {
    let mut found = BTreeSet::new();
    for profile in profiles {
        let elements = profile["differential"]["element"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        for element in elements {
            if element["binding"]["strength"] == "required"
                && let (Some(path), Some(value_set)) = (
                    element["path"].as_str(),
                    element["binding"]["valueSet"].as_str(),
                )
            {
                found.insert((path.to_owned(), value_set.to_owned()));
            }
        }
    }
    found
}

/// The value sets of [`VALUE_SETS`], by canonical URL.
fn value_sets() -> Result<BTreeMap<String, Value>, Box<dyn Error>> {
    let mut found = BTreeMap::new();
    for package in VALUE_SETS {
        insert_value_sets(&mut found, resources(package, "ValueSet")?);
    }
    Ok(found)
}

/// Adds each of `value_sets` to `found` by its canonical URL, keeping the
/// first of a URL.
fn insert_value_sets(found: &mut BTreeMap<String, Value>, value_sets: Vec<Value>) {
    for value_set in value_sets {
        if let Some(url) = value_set["url"].as_str() {
            found.entry(url.to_owned()).or_insert(value_set);
        }
    }
}

/// The code systems the value set `url` draws on, through the value sets it
/// includes, without the exceptional values.
fn systems(sets: &BTreeMap<String, Value>, url: &str) -> Result<BTreeSet<String>, Box<dyn Error>> {
    let value_set = sets
        .get(url.split('|').next().unwrap_or(url))
        .ok_or_else(|| format!("no package holds the value set {url}"))?;
    let mut found = BTreeSet::new();
    let includes = value_set["compose"]["include"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    for include in includes {
        if let Some(system) = include["system"].as_str()
            && system != NULL_FLAVOR
        {
            found.insert(system.to_owned());
        }
        for nested in include["valueSet"].as_array().cloned().unwrap_or_default() {
            let nested = nested.as_str().ok_or("a value set canonical")?;
            found.extend(systems(sets, nested)?);
        }
    }
    Ok(found)
}

/// Holds each of `bindings` to a `required` binding of `package` whose value
/// set draws on exactly its systems, with its targets among them.
fn hold(package: &str, bindings: &[Binding]) -> Result<(), Box<dyn Error>> {
    let required = required_bindings(package)?;
    let sets = value_sets()?;
    for binding in bindings {
        assert!(
            binding
                .targets
                .iter()
                .all(|target| binding.systems.contains(target)),
            "{}: a target outside its systems",
            binding.element
        );
        assert!(
            required.contains(&(binding.element.to_owned(), binding.value_set.to_owned())),
            "{} is not bound required to {}",
            binding.element,
            binding.value_set
        );
        let expected: BTreeSet<String> = binding.systems.iter().map(|s| (*s).to_owned()).collect();
        assert_eq!(
            systems(&sets, binding.value_set)?,
            expected,
            "{}",
            binding.element
        );
    }
    Ok(())
}

#[test]
fn each_laboratory_binding_is_a_required_binding_of_the_guide_on_its_systems()
-> Result<(), Box<dyn Error>> {
    if profile_set()?.is_none() {
        return Ok(());
    }
    hold(LABORATORY, &MYHEALTH_EU_LABORATORY)
}

#[test]
fn each_core_binding_is_a_required_binding_of_core_on_its_systems() -> Result<(), Box<dyn Error>> {
    if profile_set()?.is_none() {
        return Ok(());
    }
    hold(CORE, &MYHEALTH_EU_CORE)
}

#[test]
fn each_laboratory_binding_left_out_draws_on_snomed_ct() -> Result<(), Box<dyn Error>> {
    if profile_set()?.is_none() {
        return Ok(());
    }
    let required = required_bindings(LABORATORY)?;
    let sets = value_sets()?;
    for (element, name) in LEFT_OUT {
        let url = format!("http://fhir.ehdsi.eu/core/ValueSet/{name}");
        assert!(
            required.contains(&(element.to_owned(), url.clone())),
            "{element} is not bound required to {name}"
        );
        assert!(
            systems(&sets, &url)?.contains(ehn::SNOMED_CT),
            "{element}: {name} takes no SNOMED CT code"
        );
        assert!(
            MYHEALTH_EU_LABORATORY
                .iter()
                .all(|binding| binding.element != element),
            "{element} is left out"
        );
    }
    Ok(())
}

#[test]
fn the_imaging_binding_left_out_draws_on_snomed_ct() -> Result<(), Box<dyn Error>> {
    if profile_set()?.is_none() {
        return Ok(());
    }
    let required = required(imaging_resources("StructureDefinition")?);
    let sets = value_sets()?;
    for (element, name) in IMAGING_LEFT_OUT {
        let url = format!("http://fhir.ehdsi.eu/core/ValueSet/{name}");
        assert!(
            required.contains(&(element.to_owned(), url.clone())),
            "{element} is not bound required to {name}"
        );
        assert!(
            systems(&sets, &url)?.contains(ehn::SNOMED_CT),
            "{element}: {name} takes no SNOMED CT code"
        );
        assert!(
            MYHEALTH_EU_CORE
                .iter()
                .chain(&MYHEALTH_EU_LABORATORY)
                .all(|binding| binding.element != element),
            "{element} is left out"
        );
    }
    Ok(())
}

#[test]
fn each_myhealth_eu_value_set_the_imaging_guide_requires_resolves() -> Result<(), Box<dyn Error>> {
    if profile_set()?.is_none() {
        return Ok(());
    }
    let mut sets = value_sets()?;
    insert_value_sets(&mut sets, imaging_resources("ValueSet")?);
    let required = required(imaging_resources("StructureDefinition")?);
    assert!(
        required.contains(&(
            String::from("ServiceRequest.code"),
            String::from(
                "http://fhir.ehdsi.eu/imaging/ValueSet/eHDSIImagingProcedureWithExceptions"
            )
        )),
        "ServiceRequest.code is bound required under the imaging canonical"
    );
    for (element, url) in required
        .iter()
        .filter(|(_, url)| url.starts_with("http://fhir.ehdsi.eu/"))
    {
        assert!(
            !systems(&sets, url)?.is_empty(),
            "{element}: {url} draws on no system"
        );
    }
    Ok(())
}
