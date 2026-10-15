// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Canonical references and the version order the package set resolves by.
//!
//! A reference of type `canonical` is a URL with an optional `|version`
//! suffix and an optional `#fragment` naming a contained resource
//! (<https://hl7.org/fhir/R4/references.html#canonical>, §2.3.0.5 and
//! §2.3.0.6).

use std::cmp::Ordering;
use std::fmt;

/// A canonical reference split into its URL, version and fragment.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Canonical<'a> {
    url: &'a str,
    version: Option<&'a str>,
    fragment: Option<&'a str>,
}

impl<'a> Canonical<'a> {
    /// Splits `text` into the URL, the `|version` and the `#fragment`.
    ///
    /// The fragment follows the version (`url|1.0#vs1`, R4 §2.3.0.6), so it is
    /// split off first. An empty version or fragment counts as absent.
    #[must_use]
    pub fn parse(text: &'a str) -> Self {
        let (rest, fragment) = match text.split_once('#') {
            Some((rest, fragment)) => (rest, Some(fragment).filter(|f| !f.is_empty())),
            None => (text, None),
        };
        let (url, version) = match rest.split_once('|') {
            Some((url, version)) => (url, Some(version).filter(|v| !v.is_empty())),
            None => (rest, None),
        };
        Self {
            url,
            version,
            fragment,
        }
    }

    /// Returns the URL without version or fragment.
    #[must_use]
    pub const fn url(&self) -> &'a str {
        self.url
    }

    /// Returns the version the reference pins, if any.
    #[must_use]
    pub const fn version(&self) -> Option<&'a str> {
        self.version
    }

    /// Returns the fragment naming a contained resource, if any.
    #[must_use]
    pub const fn fragment(&self) -> Option<&'a str> {
        self.fragment
    }
}

impl fmt::Display for Canonical<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.url)?;
        if let Some(version) = self.version {
            write!(f, "|{version}")?;
        }
        if let Some(fragment) = self.fragment {
            write!(f, "#{fragment}")?;
        }
        Ok(())
    }
}

/// Orders two business versions, the later one greater.
///
/// R4 asks for "the latest version" when a canonical names none (§2.3.0.5)
/// and does not define the order, and every package version SHALL be a
/// Semantic Version (<https://hl7.org/fhir/packages.html>, §2.1.10.2), so the
/// order is the precedence of Semantic Versioning 2.0.0 §11
/// (<https://semver.org/spec/v2.0.0.html#spec-item-11>): the dot-separated
/// release identifiers numerically where both are numeric, a pre-release below
/// its release, and build metadata ignored. A version outside that shape still
/// orders, identifier by identifier, so the order is total.
#[must_use]
pub fn compare_versions(left: &str, right: &str) -> Ordering {
    let (left_release, left_pre) = split_version(left);
    let (right_release, right_pre) = split_version(right);
    compare_identifiers(left_release, right_release).then_with(|| match (left_pre, right_pre) {
        (None, None) => Ordering::Equal,
        (None, Some(_)) => Ordering::Greater,
        (Some(_), None) => Ordering::Less,
        (Some(left), Some(right)) => compare_identifiers(left, right),
    })
}

/// Splits a version into its release and its pre-release, dropping any build
/// metadata.
fn split_version(version: &str) -> (&str, Option<&str>) {
    let version = version.split_once('+').map_or(version, |(core, _)| core);
    match version.split_once('-') {
        Some((release, pre)) => (release, Some(pre)),
        None => (version, None),
    }
}

/// Compares dot-separated identifiers: numbers numerically and below any text,
/// text by its bytes, and a shorter list below a longer one it prefixes.
fn compare_identifiers(left: &str, right: &str) -> Ordering {
    let mut left = left.split('.');
    let mut right = right.split('.');
    loop {
        match (left.next(), right.next()) {
            (None, None) => return Ordering::Equal,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(l), Some(r)) => {
                let order = match (l.parse::<u64>(), r.parse::<u64>()) {
                    (Ok(l), Ok(r)) => l.cmp(&r),
                    (Ok(_), Err(_)) => Ordering::Less,
                    (Err(_), Ok(_)) => Ordering::Greater,
                    (Err(_), Err(_)) => l.cmp(r),
                };
                if order != Ordering::Equal {
                    return order;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::cmp::Ordering;

    use super::{Canonical, compare_versions};

    #[test]
    fn a_canonical_splits_into_url_version_and_fragment() {
        let canonical = Canonical::parse("http://example.org/Questionnaire/q|1.0#vs1");
        assert_eq!(canonical.url(), "http://example.org/Questionnaire/q");
        assert_eq!(canonical.version(), Some("1.0"));
        assert_eq!(canonical.fragment(), Some("vs1"));
    }

    #[test]
    fn a_bare_canonical_has_no_version() {
        let canonical = Canonical::parse("http://example.org/StructureDefinition/p");
        assert_eq!(canonical.version(), None);
        assert_eq!(canonical.fragment(), None);
    }

    #[test]
    fn versions_order_by_semver_precedence() {
        assert_eq!(compare_versions("2.0.10", "2.0.9"), Ordering::Greater);
        assert_eq!(compare_versions("1.0.0-ballot", "1.0.0"), Ordering::Less);
        assert_eq!(
            compare_versions("1.0.0-alpha.2", "1.0.0-alpha.10"),
            Ordering::Less
        );
        assert_eq!(compare_versions("1.0.0+build", "1.0.0"), Ordering::Equal);
        assert_eq!(compare_versions("4.0", "4.0.1"), Ordering::Less);
    }
}
