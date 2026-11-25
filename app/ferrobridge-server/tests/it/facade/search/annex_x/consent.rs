// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The advance directives of draft `EEHRxF` Annex X beside the access
//! directives the facade serves as `Consent`: a search naming `scope` and a
//! read of an id the access directives never issue reach the mapped record.

use std::error::Error;

use ferrobridge_server::consent::exchange::Settings as ExchangeSettings;
use ferrobridge_server::facade::endpoint::Endpoints;
use http::StatusCode;
use openehr_rm::draft::access_control::terminology::AccessDefault;

use super::super::super::VERSION_ONE;
use super::super::super::stub::mount_read;
use super::super::SUBJECT;
use super::super::get;
use super::super::matches;
use super::super::mount_rows;
use super::annex_x_over;
use super::composition_of;
use super::entry_row;
use super::group_named;

#[tokio::test]
async fn an_advance_directive_is_searched_by_its_scope_beside_the_access_directives()
-> Result<(), Box<dyn Error>> {
    let consent = ExchangeSettings {
        extensions_root: url::Url::parse("http://cdr.example/ferroehr/rest/extensions/v1")?,
        default_access: AccessDefault::Restricted,
    };
    let harness = annex_x_over(Endpoints::default(), Some(consent)).await?;
    let group = group_named("EHDSAdvanceDirective")?;
    let composition = composition_of(&harness, &group);
    mount_rows(&harness.cdr, &[entry_row(&composition, group.archetype)?]).await;
    mount_read(&harness.cdr, VERSION_ONE, &composition).await;

    let (status, body) = get(&harness, &format!("Consent?patient={SUBJECT}&scope=adr")).await?;
    assert_eq!(StatusCode::OK, status, "{body}");
    let found = matches(&body);
    assert_eq!(1, found.len(), "{body}");
    let id = found[0]["resource"]["id"].as_str().ok_or("an id")?;

    let (status, read) = get(&harness, &format!("Consent/{id}")).await?;
    assert_eq!(StatusCode::OK, status, "{read}");
    assert_eq!(Some("adr"), read["scope"]["coding"][0]["code"].as_str());

    let (status, statement) = get(&harness, "metadata").await?;
    assert_eq!(StatusCode::OK, status, "{statement}");
    let consents: Vec<&serde_json::Value> = statement["rest"][0]["resource"]
        .as_array()
        .map(|resources| {
            resources
                .iter()
                .filter(|resource| resource["type"] == "Consent")
                .collect()
        })
        .unwrap_or_default();
    assert_eq!(1, consents.len(), "{statement}");
    let declared = |name: &str| {
        consents[0]["searchParam"]
            .as_array()
            .is_some_and(|params| params.iter().any(|param| param["name"] == name))
    };
    assert!(declared("scope") && declared("patient"), "{statement}");
    assert!(
        consents[0]["operation"]
            .as_array()
            .is_some_and(|operations| operations
                .iter()
                .any(|operation| operation["name"] == "bppc")),
        "{statement}"
    );
    Ok(())
}
