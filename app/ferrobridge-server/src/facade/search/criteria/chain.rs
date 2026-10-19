// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The chained subject parameters a search may name.

use crate::facade::search::criteria::CriteriaError;
use crate::facade::search::criteria::Reader;
use crate::facade::search::criteria::Refused;
use crate::facade::search::criteria::Subject;
use crate::facade::search::criteria::each;
use crate::facade::search::criteria::is_type_name;
use crate::facade::search::value;

impl Reader<'_> {
    /// Reads a chained subject parameter, `patient.identifier`,
    /// `subject.identifier` or `subject:Patient.identifier`
    /// (<https://hl7.org/fhir/R4/search.html>, §3.1.1.4.15), and answers
    /// whether `name` names a chain at all.
    ///
    /// The one chain supported is `identifier` on a declared subject
    /// parameter, and only where the identifier resolve is configured; any
    /// other chain on a subject parameter is a parameter this server does not
    /// support. A type modifier other than `Patient` names a target no EHR
    /// subject is, so it matches nothing, and a modifier on the chained
    /// parameter is refused (§3.1.1.4.4). No specification governs which
    /// chains a server supports (§3.1.1.6): our own design.
    pub(super) fn chained(
        &mut self,
        name: &str,
        base: &str,
        modifier: Option<&str>,
        text: &str,
    ) -> bool {
        let (reference, typed, chain, trailing) = match (base.split_once('.'), modifier) {
            (Some((reference, chain)), trailing) => (reference, None, chain, trailing),
            (None, Some(modifier)) => {
                let Some((typed, rest)) = modifier.split_once('.') else {
                    return false;
                };
                let (chain, trailing) = match rest.split_once(':') {
                    Some((chain, trailing)) => (chain, Some(trailing)),
                    None => (rest, None),
                };
                (base, Some(typed), chain, trailing)
            }
            (None, None) => return false,
        };
        let subject = self
            .declared
            .patient()
            .is_some_and(|scope| scope.names().iter().any(|known| known == reference));
        if !subject {
            return false;
        }
        let Some(identifier) = self.identifier.filter(|_| chain == "identifier") else {
            self.unknown(name);
            return true;
        };
        if let Some(trailing) = trailing {
            self.errors.push(CriteriaError::Modifier {
                name: format!("{reference}.{chain}"),
                modifier: trailing.to_owned(),
            });
            return true;
        }
        let tokens = match each(text, value::token) {
            Ok(tokens) => tokens,
            Err(Refused::Value(error)) => {
                self.invalid_value(name, &error);
                return true;
            }
            Err(Refused::Modifier(modifier)) => {
                self.errors.push(CriteriaError::Modifier {
                    name: name.to_owned(),
                    modifier,
                });
                return true;
            }
        };
        let subjects = match typed {
            Some(typed) if !is_type_name(typed) => {
                self.errors.push(CriteriaError::Modifier {
                    name: reference.to_owned(),
                    modifier: typed.to_owned(),
                });
                return true;
            }
            Some(typed) if typed != "Patient" => vec![Subject::Never],
            Some(_) | None => {
                let mut subjects = Vec::with_capacity(tokens.len());
                for token in tokens {
                    if let Err(error) = identifier.request(&token) {
                        self.errors.push(CriteriaError::Unsupported {
                            name: name.to_owned(),
                            reason: error.to_string(),
                        });
                        return true;
                    }
                    subjects.push(Subject::Identifier {
                        parameter: name.to_owned(),
                        token,
                    });
                }
                subjects
            }
        };
        self.criteria.subjects.push(subjects);
        self.criteria.used.push((name.to_owned(), text.to_owned()));
        true
    }
}
