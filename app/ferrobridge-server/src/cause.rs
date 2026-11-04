// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The one-line form of an error and its causes, which every refusal and log
//! line of the server renders a cause through.
//!
//! No specification governs this: our own design.

/// Returns `error` and every cause behind it as one line.
///
/// This is how a refusal's cause travels in `issue.diagnostics`: the cause
/// chain of `Error::source`, walked to its root.
#[must_use]
pub fn chain(error: &dyn core::error::Error) -> String {
    let mut line = error.to_string();
    let mut cause = error.source();
    while let Some(source) = cause {
        line.push_str(": ");
        line.push_str(&source.to_string());
        cause = source.source();
    }
    line
}

#[cfg(test)]
mod tests {
    use super::chain;

    #[test]
    fn a_cause_chain_renders_as_one_line() {
        let error = crate::facade::identity::IdError::Empty { kind: "FHIR id" };
        assert_eq!("a FHIR id cannot be empty", chain(&error));
    }
}
