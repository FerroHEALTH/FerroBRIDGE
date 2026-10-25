// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The shots the capture takes: every console page and state the book shows.

use super::Shot;

/// The sign-in card a visitor reads.
pub(super) const SIGN_IN: Shot = Shot {
    image: "sign-in",
    route: "/",
    url: "/",
    ready: "//a[normalize-space(.) = 'Sign in with your identity provider']",
    shell: false,
};

/// The card a signed-out operator reads.
pub(super) const SIGNED_OUT: Shot = Shot {
    image: "signed-out",
    route: "/signed-out",
    url: "/signed-out",
    ready: "//*[contains(., 'You are signed out of the console.')]",
    shell: false,
};

/// The wire path of the provisioned context mapping.
pub(super) const CONTEXT_FILE: &str =
    "/mappings/files/fhirconnect/ferrobridge_journey_diagnose.context.yml";

/// The pages a signed-in operator loads and reads as they are.
pub(super) const PAGES: [Shot; 33] = [
    Shot::page("dashboard", "/", "/", "//h1[. = 'Dashboard']"),
    Shot::page(
        "mappings",
        "/mappings",
        "/mappings",
        "//h1[. = 'Mapping set']",
    ),
    Shot::page(
        "mappings-omocl",
        "/mappings/:tab",
        "/mappings/omocl",
        "//h1[. = 'Mapping set']",
    ),
    Shot::page(
        "mappings-templates",
        "/mappings/:tab",
        "/mappings/templates",
        "//tr[contains(., 'ferrobridge.diagnose.v1')]",
    ),
    Shot::page(
        "mappings-files",
        "/mappings/:tab",
        "/mappings/files",
        "//table//tbody//tr",
    ),
    Shot::page(
        "mappings-diagnostics",
        "/mappings/:tab",
        "/mappings/diagnostics",
        "//h1[. = 'Mapping set']",
    ),
    Shot::page(
        "file-context",
        "/mappings/files/*path",
        CONTEXT_FILE,
        "//*[@id = 'L1']",
    ),
    Shot::page(
        "file-program",
        "/mappings/files/*path",
        "/mappings/files/fhirconnect/ferrobridge_journey_diagnose.context.yml?tab=program",
        "//h1",
    ),
    Shot::page(
        "file-model",
        "/mappings/files/*path",
        "/mappings/files/fhirconnect/ferrobridge_journey_diagnose.yml",
        "//*[@id = 'L1']",
    ),
    Shot::page(
        "template",
        "/mappings/templates/:id",
        "/mappings/templates/ferrobridge.diagnose.v1",
        "//h1",
    ),
    Shot::page(
        "edit-new",
        "/mappings/new",
        "/mappings/new",
        "//*[@id = 'draft']",
    ),
    Shot::page(
        "edit-provisioned",
        "/mappings/edit/*path",
        "/mappings/edit/fhirconnect/ferrobridge_journey_diagnose.yml",
        "//*[@id = 'draft']",
    ),
    Shot::page(
        "playground",
        "/playground",
        "/playground",
        "//*[@id = 'pg-run']",
    ),
    Shot::page(
        "playground-toopenehr",
        "/playground/:tab",
        "/playground/toopenehr",
        "//*[@id = 'pg-run']",
    ),
    Shot::page(
        "playground-validate",
        "/playground/:tab",
        "/playground/validate",
        "//h1[. = 'Playground']",
    ),
    Shot::page("facade", "/facade", "/facade", "//h1[. = 'FHIR facade']"),
    Shot::page(
        "facade-capability",
        "/facade/:tab",
        "/facade/capability",
        "//*[contains(., 'From the CapabilityStatement')]",
    ),
    Shot::page(
        "facade-security",
        "/facade/:tab",
        "/facade/security",
        "//h1[. = 'FHIR facade']",
    ),
    Shot::page(
        "facade-settings",
        "/facade/:tab",
        "/facade/settings",
        "//*[contains(., 'facade.base_url')]",
    ),
    Shot::page("hl7v2", "/hl7v2", "/hl7v2", "//h1[. = 'HL7 v2']"),
    Shot::page(
        "hl7v2-settings",
        "/hl7v2/:tab",
        "/hl7v2/settings",
        "//h1[. = 'HL7 v2']",
    ),
    Shot::page("omop", "/omop", "/omop", "//h1[. = 'OMOP ETL']"),
    Shot::page(
        "omop-settings",
        "/omop/:tab",
        "/omop/settings",
        "//h1[. = 'OMOP ETL']",
    ),
    Shot::page(
        "connections",
        "/connections",
        "/connections",
        "//h1[. = 'Connections']",
    ),
    Shot::page(
        "connection-cdr",
        "/connections/:name",
        "/connections/cdr",
        "//h1",
    ),
    Shot::page(
        "connection-cdr-settings",
        "/connections/:name/:tab",
        "/connections/cdr/settings",
        "//*[contains(., 'cdr.base_url')]",
    ),
    Shot::page(
        "configuration",
        "/configuration",
        "/configuration",
        "//h1[. = 'Configuration']",
    ),
    Shot::page(
        "configuration-mappings",
        "/configuration/:tab",
        "/configuration/mappings",
        "//*[contains(., 'mappings.directory')]",
    ),
    Shot::page(
        "configuration-core",
        "/configuration/:tab",
        "/configuration/core",
        "//*[contains(., 'server.listen')]",
    ),
    Shot::page(
        "preferences",
        "/preferences",
        "/preferences",
        "//h1[. = 'Your preferences']",
    ),
    Shot::page(
        "preferences-language",
        "/preferences/:section",
        "/preferences/language",
        "//h1[. = 'Your preferences']",
    ),
    Shot::page("about", "/about", "/about", "//h1"),
    Shot::page(
        "search",
        "/search",
        "/search?q=diagnose",
        "//section[@aria-label = 'Contexts']//a[contains(@class, 'fui-hit')]",
    ),
];

/// The page no route answers.
pub(super) const NOT_FOUND: Shot = Shot::page("not-found", "*", "/no/such/page", "//h1");

/// A `$toopenehr` the bridge answered.
pub(super) const PLAYGROUND_ANSWERED: Shot = Shot::page(
    "playground-answered",
    "/playground/:tab",
    "/playground/toopenehr",
    "//*[contains(., 'Synthetic problem one')]",
);

/// A `$tofhir` the bridge refused.
pub(super) const PLAYGROUND_REFUSED: Shot = Shot::page(
    "playground-refused",
    "/playground",
    "/playground",
    "//*[contains(., 'The bridge refused the request')]",
);

/// A bridge whose operations lane is off.
pub(super) const PLAYGROUND_OFF: Shot = Shot::page(
    "playground-off",
    "/playground/:tab",
    "/playground/toopenehr",
    "//*[contains(., 'Conversions are off on this bridge.')]",
);

/// A draft whose check found the set does not compile.
pub(super) const EDIT_CHECKED: Shot = Shot::page(
    "edit-checked",
    "/mappings/new",
    "/mappings/new",
    "//*[contains(., 'The set does not compile with this draft')]",
);

/// The confirmation a save asks for.
pub(super) const EDIT_CONFIRM: Shot = Shot::page(
    "edit-confirm",
    "/mappings/new",
    "/mappings/new",
    "//button[@name = 'decision' and @value = 'confirm']",
);

/// A draft the bridge refused to save.
pub(super) const EDIT_REFUSED: Shot = Shot::page(
    "edit-refused",
    "/mappings/new",
    "/mappings/new",
    "//*[contains(., 'Not saved: the set does not compile with your draft')]",
);

/// A draft the bridge saved and reloaded.
pub(super) const EDIT_SAVED: Shot = Shot::page(
    "edit-saved",
    "/mappings/new",
    "/mappings/new",
    "//*[contains(., 'Saved, and the set reloaded with it.')]",
);

/// A reload of the set from the Mapping set.
pub(super) const MAPPINGS_RELOADED: Shot = Shot::page(
    "mappings-reloaded",
    "/mappings",
    "/mappings",
    "//*[contains(., 'Reloaded: the set compiled and replaced the running one')]",
);

/// Every shot the capture takes.
pub(super) fn shots() -> impl Iterator<Item = &'static Shot> {
    [&SIGN_IN].into_iter().chain(PAGES.iter()).chain([
        &PLAYGROUND_ANSWERED,
        &PLAYGROUND_REFUSED,
        &PLAYGROUND_OFF,
        &EDIT_CHECKED,
        &EDIT_CONFIRM,
        &EDIT_REFUSED,
        &EDIT_SAVED,
        &MAPPINGS_RELOADED,
        &NOT_FOUND,
        &SIGNED_OUT,
    ])
}
