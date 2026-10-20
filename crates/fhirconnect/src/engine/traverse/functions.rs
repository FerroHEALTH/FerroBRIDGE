// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The `mappingCode` registry a run is handed.
//!
//! A PROGRAMMED mapping "will not auto-transform between the paths, since the
//! mapping logic is done in the program code"
//! (`docs/specs/fhirconnect/modules/ROOT/pages/types-of-mappings/concept-type/concept-mappings.adoc`,
//! §Programmed mappings), so a function sees the whole FHIR element the
//! mapping's `with.fhir` names and the openEHR values at the ports its
//! signature declares below the mapping's `with.openehr`. The compiler
//! resolves every port against the template at load, so the engine reads and
//! writes a port as it does any other node and never parses a path.

use core::fmt;
use std::collections::BTreeMap;

use fhir_types::codec::DecodeError;
use fhir_types::codec::EncodeError;
use fhir_types::codec::Value;
use openehr_its::json::JsonParseError;

/// The port that names the node the mapping's `with.openehr` resolves to.
pub const SELF_PORT: &str = "";

/// One openEHR data value a function reads or writes at one port.
///
/// The value is the canonical JSON of the class (openEHR ITS-JSON), which a
/// function decodes into the `openehr-rm` type it works in; the class decides
/// which value node of a polymorphic `ELEMENT` the engine reads or writes.
#[derive(Debug, Clone, PartialEq)]
pub struct Datum {
    class: String,
    value: serde_json::Value,
}

impl Datum {
    /// Pairs a reference-model class with the canonical JSON of its value.
    #[must_use]
    pub fn new(class: impl Into<String>, value: serde_json::Value) -> Self {
        Self {
            class: class.into(),
            value,
        }
    }

    /// Returns the reference-model class of the value, `DV_DURATION`.
    #[must_use]
    pub fn class(&self) -> &str {
        &self.class
    }

    /// Returns the canonical JSON of the value.
    #[must_use]
    pub const fn value(&self) -> &serde_json::Value {
        &self.value
    }
}

/// The openEHR values of one call, by the port each was read from or is
/// written to.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Ports(BTreeMap<String, Datum>);

impl Ports {
    /// Creates a call with no value at any port.
    #[must_use]
    pub const fn new() -> Self {
        Self(BTreeMap::new())
    }

    /// Returns these ports with `datum` at `port`, replacing what it held.
    #[must_use]
    pub fn with(mut self, port: impl Into<String>, datum: Datum) -> Self {
        self.0.insert(port.into(), datum);
        self
    }

    /// Returns the value at `port`, `None` when the composition holds none.
    #[must_use]
    pub fn get(&self, port: &str) -> Option<&Datum> {
        self.0.get(port)
    }

    /// Returns whether no port holds a value.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Returns every port and its value, in port order.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &Datum)> {
        self.0.iter().map(|(port, datum)| (port.as_str(), datum))
    }
}

/// The registry a `mappingCode` resolves against at run time.
///
/// The specification leaves the functions to the engine ("The way how this
/// code is provided and where is up to the vendor", `concept-mappings.adoc`
/// §Programmed mappings), so the registry is a seam rather than a fixed list.
/// [`crate::engine::library::Library`] is the one this crate ships; the
/// default seam registers nothing.
pub trait MappingFunctions: fmt::Debug + Send + Sync {
    /// Runs the function `code` names going out of openEHR, over the values
    /// its ports hold.
    ///
    /// Returns the FHIR element written at the mapping's `with.fhir`, merged
    /// into what the element already holds, or `None` when the ports hold
    /// nothing the function writes.
    ///
    /// # Errors
    ///
    /// Returns [`MappingCodeError`] when the registry does not hold `code` or
    /// the function refuses its input.
    fn to_fhir(&self, code: &str, ports: &Ports) -> Result<Option<Value>, MappingCodeError>;

    /// Runs the function `code` names going into openEHR, over the FHIR
    /// element the mapping's `with.fhir` names.
    ///
    /// Returns the value each port is written with; a port the result leaves
    /// out is written nothing.
    ///
    /// # Errors
    ///
    /// Returns [`MappingCodeError`] when the registry does not hold `code` or
    /// the function refuses its input.
    fn to_openehr(&self, code: &str, element: &Value) -> Result<Ports, MappingCodeError>;
}

/// A registry that holds no function, so every call is refused.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct NoMappingFunctions;

impl MappingFunctions for NoMappingFunctions {
    fn to_fhir(&self, code: &str, _ports: &Ports) -> Result<Option<Value>, MappingCodeError> {
        Err(MappingCodeError::Unknown {
            code: String::from(code),
        })
    }

    fn to_openehr(&self, code: &str, _element: &Value) -> Result<Ports, MappingCodeError> {
        Err(MappingCodeError::Unknown {
            code: String::from(code),
        })
    }
}

/// Why a registered function did not produce a value.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum MappingCodeError {
    /// The registry holds no function of that name.
    #[error("no registered function is named {code}")]
    Unknown {
        /// The name the mapping wrote.
        code: String,
    },
    /// The function refused its input.
    #[error("the function {code} refused its input: {reason}")]
    Refused {
        /// The name the mapping wrote.
        code: String,
        /// What the function refused.
        reason: String,
    },
    /// The FHIR element does not decode as the type the function reads.
    #[error("the function {code} cannot read the FHIR element as {element}")]
    FhirDecode {
        /// The name the mapping wrote.
        code: String,
        /// The FHIR type the function reads.
        element: &'static str,
        /// The refusal of the generated codec.
        #[source]
        source: Box<DecodeError>,
    },
    /// The FHIR element the function built has no JSON form.
    #[error("the function {code} built a FHIR element with no JSON form")]
    FhirEncode {
        /// The name the mapping wrote.
        code: String,
        /// The refusal of the generated codec.
        #[source]
        source: Box<EncodeError>,
    },
    /// A port's canonical JSON does not decode as the class it names.
    #[error("the function {code} cannot read the {class} at port `{port}`")]
    RmDecode {
        /// The name the mapping wrote.
        code: String,
        /// The port the value was read from.
        port: String,
        /// The class the value names.
        class: String,
        /// The refusal of the ITS-JSON reader.
        #[source]
        source: Box<JsonParseError>,
    },
    /// The function wrote a port its signature does not declare.
    #[error("the function {code} wrote the port `{port}` its signature does not declare")]
    UndeclaredPort {
        /// The name the mapping wrote.
        code: String,
        /// The port the function named.
        port: String,
    },
    /// The template admits no value of the class at the port.
    #[error("the template admits no {class} at the port `{port}` of {code}")]
    UnadmittedClass {
        /// The name the mapping wrote.
        code: String,
        /// The port the function wrote.
        port: String,
        /// The class the function wrote.
        class: String,
    },
}
