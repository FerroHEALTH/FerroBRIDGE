// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The SMART App Launch 2.x clinical scopes an access token grants.
//!
//! A scope reads `<context>/<resource>.<permissions>`: the context is
//! `patient`, `user` or `system`, the resource a FHIR resource type or `*`,
//! and the permissions "a subset of the in-order string `.cruds`"
//! (<https://hl7.org/fhir/smart-app-launch/scopes-and-launch-context.html#scopes-for-requesting-fhir-resources>).
//! Every other scope token (`openid`, `launch/patient`, `offline_access`, the
//! operator scope) grants no FHIR access and is passed over here.

use fhir_types::r4::schema::SCHEMAS;

/// One letter of the SMART v2 permission string.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Permission {
    /// `c`, the `create` interaction.
    Create,
    /// `r`, the `read` and `vread` interactions.
    Read,
    /// `u`, the `update` interaction.
    Update,
    /// `d`, the `delete` interaction.
    Delete,
    /// `s`, the `search` interaction.
    Search,
}

impl Permission {
    /// The letters in the order the permission string requires.
    const ORDER: [(char, Self); 5] = [
        ('c', Self::Create),
        ('r', Self::Read),
        ('u', Self::Update),
        ('d', Self::Delete),
        ('s', Self::Search),
    ];

    /// Returns the bit this permission occupies in a [`Permissions`] set.
    const fn bit(self) -> u8 {
        match self {
            Self::Create => 1,
            Self::Read => 2,
            Self::Update => 4,
            Self::Delete => 8,
            Self::Search => 16,
        }
    }
}

/// A set of [`Permission`]s.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
struct Permissions(u8);

impl Permissions {
    /// Returns whether the set holds `permission`.
    const fn holds(self, permission: Permission) -> bool {
        self.0 & permission.bit() != 0
    }

    /// Returns the set a permission string names, or `None` when the string
    /// is no SMART permission string.
    ///
    /// The v1 suffixes `read`, `write` and `*` read as `rs`, `cud` and
    /// `cruds`: "servers SHOULD process v1 scopes with the following
    /// semantics in v2"
    /// (<https://hl7.org/fhir/smart-app-launch/scopes-and-launch-context.html#scopes-for-requesting-fhir-resources>).
    fn parse(text: &str) -> Option<Self> {
        let letters = match text {
            "read" => "rs",
            "write" => "cud",
            "*" => "cruds",
            letters => letters,
        };
        if letters.is_empty() {
            return None;
        }
        let mut bits = 0u8;
        let mut next = 0usize;
        for letter in letters.chars() {
            let offset = Permission::ORDER
                .iter()
                .skip(next)
                .position(|(candidate, _)| *candidate == letter)?;
            let position = next.checked_add(offset)?;
            let (_, permission) = Permission::ORDER.get(position)?;
            bits |= permission.bit();
            next = position.checked_add(1)?;
        }
        Some(Self(bits))
    }
}

/// The context a clinical scope is granted in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Context {
    /// `patient/`: the data of the patient in the token's launch context.
    Patient,
    /// `user/`: the data the signed-in user may reach.
    User,
    /// `system/`: the data a backend service may reach.
    System,
}

/// How far one granted permission reaches.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reach {
    /// Only the compartment of the patient in the launch context.
    Patient,
    /// Every resource of the type.
    Any,
}

/// One clinical scope the token grants.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Grant {
    /// The context it was granted in.
    context: Context,
    /// The resource type, `None` for the `*` wildcard.
    resource_type: Option<String>,
    /// The interactions it permits.
    permissions: Permissions,
}

/// The clinical scopes of one access token.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Scopes {
    /// Every clinical scope the `scope` claim named.
    grants: Vec<Grant>,
}

impl Scopes {
    /// Returns the clinical scopes of the space-delimited `scope` value
    /// (RFC 6749 §3.3).
    ///
    /// A token that is no clinical scope, names a resource type R4 does not
    /// define, or carries permissions out of order grants nothing: such
    /// requests "MAY be ignored"
    /// (<https://hl7.org/fhir/smart-app-launch/scopes-and-launch-context.html#scopes-for-requesting-fhir-resources>).
    #[must_use]
    pub fn parse(scope: &str) -> Self {
        Self {
            grants: scope.split(' ').filter_map(grant).collect(),
        }
    }

    /// Returns whether the token names no clinical scope at all.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.grants.is_empty()
    }

    /// Returns how far `permission` on `resource_type` reaches, or `None`
    /// when no scope grants it.
    ///
    /// A `user/` or `system/` scope reaches every resource of the type, so it
    /// wins over a `patient/` scope for the same permission.
    #[must_use]
    pub fn reach(&self, permission: Permission, resource_type: &str) -> Option<Reach> {
        let mut reach = None;
        for grant in &self.grants {
            let names_type = grant
                .resource_type
                .as_deref()
                .is_none_or(|named| named == resource_type);
            if !names_type || !grant.permissions.holds(permission) {
                continue;
            }
            match grant.context {
                Context::User | Context::System => return Some(Reach::Any),
                Context::Patient => reach = Some(Reach::Patient),
            }
        }
        reach
    }
}

/// Returns the clinical scope `token` names, or `None` for any other token.
fn grant(token: &str) -> Option<Grant> {
    let (context, rest) = token.split_once('/')?;
    let context = match context {
        "patient" => Context::Patient,
        "user" => Context::User,
        "system" => Context::System,
        _ => return None,
    };
    // NOTE: SMART App Launch 2.2.0, scopes-and-launch-context.html, calls the
    // `?param=value` constraint experimental; a constrained scope is not
    // evaluated here, so it grants nothing rather than more than it says.
    if rest.contains('?') {
        return None;
    }
    let (resource, permissions) = rest.rsplit_once('.')?;
    let resource_type = match resource {
        "*" => None,
        named if SCHEMAS.is_resource(named) => Some(named.to_owned()),
        _ => return None,
    };
    Some(Grant {
        context,
        resource_type,
        permissions: Permissions::parse(permissions)?,
    })
}

#[cfg(test)]
mod tests {
    use super::{Permission, Reach, Scopes};

    #[test]
    fn a_v2_scope_grants_exactly_its_letters() {
        let scopes = Scopes::parse("system/Condition.crs");
        assert_eq!(
            Some(Reach::Any),
            scopes.reach(Permission::Create, "Condition")
        );
        assert_eq!(
            Some(Reach::Any),
            scopes.reach(Permission::Read, "Condition")
        );
        assert_eq!(
            Some(Reach::Any),
            scopes.reach(Permission::Search, "Condition")
        );
        assert_eq!(None, scopes.reach(Permission::Update, "Condition"));
        assert_eq!(None, scopes.reach(Permission::Delete, "Condition"));
        assert_eq!(None, scopes.reach(Permission::Read, "Observation"));
    }

    #[test]
    fn the_wildcard_names_every_type() {
        let scopes = Scopes::parse("system/*.r");
        assert_eq!(
            Some(Reach::Any),
            scopes.reach(Permission::Read, "Observation")
        );
        assert_eq!(None, scopes.reach(Permission::Create, "Observation"));
    }

    #[test]
    fn letters_out_of_order_or_repeated_grant_nothing() {
        for token in [
            "system/Condition.rc",
            "system/Condition.rr",
            "system/Condition.",
            "system/Condition.x",
        ] {
            assert!(Scopes::parse(token).is_empty(), "{token}");
        }
    }

    #[test]
    fn the_v1_suffixes_read_as_their_v2_letters() {
        let read = Scopes::parse("user/Condition.read");
        assert!(read.reach(Permission::Read, "Condition").is_some());
        assert!(read.reach(Permission::Search, "Condition").is_some());
        assert!(read.reach(Permission::Create, "Condition").is_none());
        let write = Scopes::parse("user/Condition.write");
        assert!(write.reach(Permission::Create, "Condition").is_some());
        assert!(write.reach(Permission::Update, "Condition").is_some());
        assert!(write.reach(Permission::Delete, "Condition").is_some());
        assert!(write.reach(Permission::Read, "Condition").is_none());
        let all = Scopes::parse("user/*.*");
        assert!(all.reach(Permission::Delete, "Condition").is_some());
    }

    #[test]
    fn a_patient_scope_reaches_the_compartment_and_a_system_scope_wins() {
        let patient = Scopes::parse("patient/Condition.rs");
        assert_eq!(
            Some(Reach::Patient),
            patient.reach(Permission::Read, "Condition")
        );
        let both = Scopes::parse("patient/Condition.rs system/Condition.r");
        assert_eq!(Some(Reach::Any), both.reach(Permission::Read, "Condition"));
    }

    #[test]
    fn other_tokens_and_constrained_scopes_grant_nothing() {
        let scopes = Scopes::parse(
            "openid fhirUser launch/patient offline_access ferrobridge.operator \
             patient/Observation.rs?category=laboratory system/NotAType.r",
        );
        assert!(scopes.is_empty());
    }
}
