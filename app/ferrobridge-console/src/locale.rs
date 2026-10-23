// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The locale the console renders in, and the catalog of its own words.
//!
//! The console's words are messages of the Fluent catalogs under the crate's
//! `locales/` directory, one directory per language
//! (`locales/en/ferrobridge-console.ftl`), embedded at build time by
//! `fluent_templates::static_loader!`, the mechanism `ferrohealth_ui`
//! catalogs its own words with. English is the one catalog that ships and the
//! one every lookup falls back to; a language joins as one catalog file and
//! no code change.
//!
//! The server chooses the locale once per request: the best language of the
//! request's `Accept-Language` the catalog holds, else the configured
//! `[server] locale`. [`crate::app::App`] provides it to both halves through
//! `ferrohealth_ui::i18n::provide_locale`, so the console's words and the
//! components' words follow the same choice, and the document writes it as
//! `<html lang>` (WCAG 2.2 SC 3.1.1). No specification governs the catalog
//! layout or the choice: our own design.

use ferrohealth_ui::i18n::{Arg, Locale, Message, locale};
use fluent_templates::Loader;

fluent_templates::static_loader! {
    static CATALOG = {
        locales: "./locales",
        fallback_language: "en",
        // NOTE: Fluent's guide, "Bidirectionality": the isolation marks around a
        // placeable are for mixed-direction text; the English catalog draws none.
        customise: |bundle| bundle.set_use_isolating(false),
    };
}

/// The console's catalogs, every language it ships.
#[must_use]
pub fn catalog() -> &'static (dyn Loader + Send + Sync) {
    &*CATALOG
}

/// Declares the console's messages: each variant names one message of the
/// catalog by its id.
macro_rules! messages {
    ($($(#[$meta:meta])+ $name:ident = $id:literal,)+) => {
        /// A string the console draws, by its message in the catalog.
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub enum Msg {
            $($(#[$meta])+ $name,)+
        }

        impl Msg {
            /// Every message, in declaration order.
            pub const ALL: &[Self] = &[$(Self::$name,)+];

            /// The message's id in the catalog.
            #[must_use]
            pub const fn id(self) -> &'static str {
                match self {
                    $(Self::$name => $id,)+
                }
            }
        }
    };
}

messages! {
    /// The sign-in card's button to the operator's provider.
    SignIn = "sign-in",
    /// The title of the page for a session store the console cannot read.
    UnreadableTitle = "unreadable-title",
    /// The heading of that page.
    UnreadableHeading = "unreadable-heading",
    /// The fault that page states.
    UnreadableFault = "unreadable-fault",
    /// The title and heading of the page for a path the console does not serve.
    NotFoundTitle = "not-found-title",
    /// That page's way back.
    NotFoundBack = "not-found-back",
    /// `Business Source License 1.1`
    BusinessSourceLicense11 = "business-source-license-1-1",
    /// `Licence`
    Licence = "licence",
    /// `Made by`
    MadeBy = "made-by",
    /// `Version`
    Version = "version",
    /// `About`
    About = "about",
    /// `Bridge`
    Bridge = "bridge",
    /// `Operator`
    Operator = "operator",
    /// `Asking the bridge for the file`
    AskingBridgeFile = "asking-bridge-file",
    /// `Check a draft against the running set, save it beside the provisioned tree, and reload.`
    CheckDraftAgainstRunningSetSave = "check-draft-against-running-set-save",
    /// `Save`
    Save = "save",
    /// `This file is provisioned, so the console checks a draft of it but never saves over it.`
    FileProvisionedSoConsoleChecksDraft = "file-provisioned-so-console-checks-draft",
    /// `Reload`
    Reload = "reload",
    /// `Compiles the files on disk and replaces the running set when they compile.`
    CompilesFilesDiskReplacesRunningSet = "compiles-files-disk-replaces-running-set",
    /// `Check`
    Check = "check",
    /// `The file`
    TheFile = "the-file",
    /// `Path, below fhirconnect/ or omocl/`
    PathBelowFhirconnectOmocl = "path-below-fhirconnect-omocl",
    /// `Create the file`
    CreateTheFile = "create-the-file",
    /// `Save this draft and reload the set. The bridge refuses a draft that does not compile.`
    SaveDraftReloadSetBridgeRefuses = "save-draft-reload-set-bridge-refuses",
    /// `Two hundred characters at most.`
    TwoHundredCharactersMost = "two-hundred-characters-most",
    /// `Why you save it, one line`
    WhyYouSaveOneLine = "why-you-save-one-line",
    /// `Confirm the save before it is sent.`
    ConfirmSaveBeforeSent = "confirm-save-before-sent",
    /// `Not saved`
    NotSaved = "not-saved",
    /// `The file is not UTF-8, so the console does not open it.`
    FileNotUtf8SoConsole = "file-not-utf-8-so-console",
    /// `The files on disk do not compile, so the running set is unchanged.`
    FilesDiskDoNotCompileSo = "files-disk-do-not-compile-so",
    /// `The files on disk compiled and replaced the running set.`
    FilesDiskCompiledReplacedRunningSet = "files-disk-compiled-replaced-running-set",
    /// `409: a provisioned file holds this path, and the console never writes over one. Save...`
    N409ProvisionedFileHoldsPathConsole = "n-409-provisioned-file-holds-path-console",
    /// `412: the file changed since you opened it, so nothing was written. Open the file again...`
    N412FileChangedSinceYouOpened = "n-412-file-changed-since-you-opened",
    /// `The draft does not compile with the set, so nothing was written and the running set is...`
    DraftDoesNotCompileSetSo = "draft-does-not-compile-set-so",
    /// `The file's SHA-256 is now`
    FileSSha256Now = "file-s-sha-256-now",
    /// `Saved, and the set reloaded with it.`
    SavedSetReloaded = "saved-set-reloaded",
    /// `Saved`
    Saved = "saved",
    /// `not stated`
    NotStated = "not-stated",
    /// `The set does not compile with this draft; a save would be refused.`
    SetDoesNotCompileDraftSave = "set-does-not-compile-draft-save",
    /// `The set compiles with this draft.`
    SetCompilesDraft = "set-compiles-draft",
    /// `Mappings`
    Mappings = "mappings",
    /// `Capability`
    Capability = "capability",
    /// `Dashboard`
    Dashboard = "dashboard",
    /// `Mapping file`
    MappingFile = "mapping-file",
    /// `Mapping files`
    MappingFiles = "mapping-files",
    /// `Asking the bridge`
    AskingTheBridge = "asking-the-bridge",
    /// `The discovery document as JSON`
    DiscoveryDocumentJson = "discovery-document-json",
    /// `Scopes`
    Scopes = "scopes",
    /// `Capabilities`
    Capabilities = "capabilities",
    /// `Token endpoint`
    TokenEndpoint = "token-endpoint",
    /// `Authorization endpoint`
    AuthorizationEndpoint = "authorization-endpoint",
    /// `Issuer`
    Issuer = "issuer",
    /// `SMART discovery`
    SmartDiscovery = "smart-discovery",
    /// `Interactions`
    Interactions = "interactions",
    /// `Profile`
    Profile = "profile",
    /// `Search parameters`
    SearchParameters = "search-parameters",
    /// `Search parameter`
    SearchParameter = "search-parameter",
    /// `Type`
    Type = "type",
    /// `The statement declares no security.`
    StatementDeclaresNoSecurity = "statement-declares-no-security",
    /// `Description`
    Description = "description",
    /// `CORS`
    Cors = "cors",
    /// `Services`
    Services = "services",
    /// `The CapabilityStatement as JSON`
    TheCapabilitystatementAsJson = "the-capabilitystatement-as-json",
    /// `Formats`
    Formats = "formats",
    /// `Status`
    Status = "status",
    /// `Kind`
    Kind = "kind",
    /// `FHIR version`
    FhirVersion = "fhir-version",
    /// `Software`
    Software = "software",
    /// `CapabilityStatement`
    Capabilitystatement = "capabilitystatement",
    /// `Readiness`
    Readiness = "readiness",
    /// `Indicators`
    Indicators = "indicators",
    /// `The bridge registers no readiness indicator.`
    BridgeRegistersNoReadinessIndicator = "bridge-registers-no-readiness-indicator",
    /// `State`
    State = "state",
    /// `Detail`
    Detail = "detail",
    /// `Not ready`
    NotReady = "not-ready",
    /// `Ready`
    Ready = "ready",
    /// `Configured`
    Configured = "configured",
    /// `Listener`
    Listener = "listener",
    /// `none of its own`
    NoneOfItsOwn = "none-of-its-own",
    /// `Lanes and listeners`
    LanesAndListeners = "lanes-and-listeners",
    /// `Indicator`
    Indicator = "indicator",
    /// `Lane`
    Lane = "lane",
    /// `Pins`
    Pins = "pins",
    /// `Specification or crate`
    SpecificationOrCrate = "specification-or-crate",
    /// `The mappings overview`
    TheMappingsOverview = "the-mappings-overview",
    /// `OMOCL files`
    OmoclFiles = "omocl-files",
    /// `FHIRconnect files`
    FhirconnectFiles = "fhirconnect-files",
    /// `Templates`
    Templates = "templates",
    /// `Compiler`
    Compiler = "compiler",
    /// `Built`
    Built = "built",
    /// `Commit`
    Commit = "commit",
    /// `Product`
    Product = "product",
    /// `Warnings`
    Warnings = "warnings",
    /// `Errors`
    Errors = "errors",
    /// `Models`
    Models = "models",
    /// `Contexts loaded`
    ContextsLoaded = "contexts-loaded",
    /// `Open it in the editor`
    OpenEditor = "open-editor",
    /// `SHA-256`
    Sha256 = "sha-256",
    /// `The file is not UTF-8, so the console does not show it: a YAML file is UTF-8 (YAML...`
    FileNotUtf8SoConsole2 = "file-not-utf-8-so-console-2",
    /// `Diagnostics`
    Diagnostics = "diagnostics",
    ///  at
    At = "at",
    /// `The running load raised no diagnostic about this file.`
    RunningLoadRaisedNoDiagnosticAbout = "running-load-raised-no-diagnostic-about",
    /// `No diagnostic`
    NoDiagnostic = "no-diagnostic",
    /// `Writable`
    Writable = "writable",
    /// `Owner`
    Owner = "owner",
    /// `Language`
    Language = "language",
    /// `File`
    File = "file",
    /// `not read`
    NotRead = "not-read",
    /// `The bridge serves no mapping file.`
    BridgeServesNoMappingFile = "bridge-serves-no-mapping-file",
    /// `No mapping file`
    NoMappingFile = "no-mapping-file",
    /// `Search`
    Search = "search",
    /// `Models and extensions`
    ModelsAndExtensions = "models-and-extensions",
    /// `Resource`
    Resource = "resource",
    /// `Template`
    Template = "template",
    /// `none`
    None = "none",
    ///  in
    In = "in",
    /// `extended by`
    ExtendedBy = "extended-by",
    /// `Source`
    Source = "source",
    /// `the CDR`
    TheCdr = "the-cdr",
    /// `template directory`
    TemplateDirectory = "template-directory",
    /// `OMOCL`
    Omocl = "omocl",
    /// `Contexts`
    Contexts = "contexts",
    /// `The OMOCL files are`
    TheOmoclFilesAre = "the-omocl-files-are",
    /// `The running load reads no template.`
    RunningLoadReadsNoTemplate = "running-load-reads-no-template",
    /// `No template`
    NoTemplate = "no-template",
    /// `Context`
    Context = "context",
    /// `The running load compiled no context.`
    RunningLoadCompiledNoContext = "running-load-compiled-no-context",
    /// `No context`
    NoContext = "no-context",
    /// `no file`
    NoFile = "no-file",
    /// `Severity`
    Severity = "severity",
    /// `Code`
    Code = "code",
    /// `Where`
    Where = "where",
    /// `Message`
    Message = "message",
    /// `Mapping`
    Mapping = "mapping",
    /// `No diagnostics.`
    NoDiagnostics = "no-diagnostics",
    /// `401: the bridge did not accept your sign-in. Sign in again.`
    N401BridgeDidNotAcceptYour = "n-401-bridge-did-not-accept-your",
    /// `The reason must be one line of at most 200 printable ASCII characters, so the save is...`
    ReasonMustOneLineMost200 = "reason-must-one-line-most-200",
    /// `the resource type is not a FHIR resource type name, so it is not asked for`
    ResourceTypeNotFhirResourceType = "resource-type-not-fhir-resource-type",
    /// `the mapping file path is not plain names below a root, so it is not asked for`
    MappingFilePathNotPlainNames = "mapping-file-path-not-plain-names",
    /// `Pages`
    Pages = "pages",
    /// `Bridge UNKNOWN`
    BridgeUnknown = "bridge-unknown",
    /// `Bridge DOWN`
    BridgeDown = "bridge-down",
    /// `Bridge refused 401`
    BridgeRefused401 = "bridge-refused-401",
    /// `Signed in to this console`
    SignedConsole = "signed-console",
    /// `Console`
    Console = "console",
    /// `Validation dry run`
    ValidationDryRun = "validation-dry-run",
    /// `Playground`
    Playground = "playground",
    /// `Tools`
    Tools = "tools",
    /// `New file`
    NewFile = "new-file",
    /// `Files`
    Files = "files",
    /// `Overview`
    Overview = "overview",
    /// `The bridge declares $validate on no resource type.`
    BridgeDeclaresValidateNoResourceType = "bridge-declares-validate-no-resource-type",
    /// `Reading the resource types the bridge validates`
    ReadingResourceTypesBridgeValidates = "reading-resource-types-bridge-validates",
    /// `$validate`
    Validate = "validate",
    /// `Runs $validate, which commits nothing. Nothing you enter or the bridge answers is kept.`
    RunsValidateWhichCommitsNothingNothing = "runs-validate-which-commits-nothing-nothing",
    /// `Run $validate`
    RunValidate = "run-validate",
    /// `Resource, JSON`
    ResourceJson = "resource-json",
    /// `Resource type`
    ResourceType = "resource-type",
    /// `Runs a transform against the bridge's running programs. Nothing you enter or the...`
    RunsTransformAgainstBridgeSRunning = "runs-transform-against-bridge-s-running",
    /// `Transform playground`
    TransformPlayground = "transform-playground",
    /// `Run $toopenehr`
    RunToopenehr = "run-toopenehr",
    /// `flat`
    Flat = "flat",
    /// `canonical`
    Canonical = "canonical",
    /// `Composition format`
    CompositionFormat = "composition-format",
    /// `Template id`
    TemplateId = "template-id",
    /// `Bundle, JSON`
    BundleJson = "bundle-json",
    /// `FHIR to openEHR: $toopenehr`
    FhirToOpenehrToopenehr = "fhir-to-openehr-toopenehr",
    /// `Run $tofhir`
    RunTofhir = "run-tofhir",
    /// `context.patient, a reference such as Patient/123`
    ContextPatientReferenceSuchPatient123 = "context-patient-reference-such-patient-123",
    /// `Template id (required for FLAT)`
    TemplateIdRequiredFlat = "template-id-required-flat",
    /// `Composition, canonical or FLAT JSON`
    CompositionCanonicalFlatJson = "composition-canonical-flat-json",
    /// `openEHR to FHIR: $tofhir`
    OpenehrToFhirTofhir = "openehr-to-fhir-tofhir",
    /// `Issues, by severity`
    IssuesBySeverity = "issues-by-severity",
    /// `$validate answered`
    ValidateAnswered = "validate-answered",
    /// `The OperationOutcome as JSON`
    TheOperationoutcomeAsJson = "the-operationoutcome-as-json",
    /// `$toopenehr answered`
    ToopenehrAnswered = "toopenehr-answered",
    /// `The Parameters as JSON`
    TheParametersAsJson = "the-parameters-as-json",
    /// `The composition`
    TheComposition = "the-composition",
    /// `Composition`
    Composition = "composition",
    /// `The answer carries no composition.`
    AnswerCarriesNoComposition = "answer-carries-no-composition",
    /// `$tofhir answered`
    TofhirAnswered = "tofhir-answered",
    /// `The Bundle as JSON`
    TheBundleAsJson = "the-bundle-as-json",
    /// `an entry with no resource`
    EntryNoResource = "entry-no-resource",
    /// `The resource's resourceType is not the type chosen.`
    ResourceSResourcetypeNotTypeChosen = "resource-s-resourcetype-not-type-chosen",
    /// `The resource is not JSON.`
    ResourceNotJson = "resource-not-json",
    /// `The Bundle is not a resource whose resourceType is Bundle.`
    BundleNotResourceWhoseResourcetypeBundle = "bundle-not-resource-whose-resourcetype-bundle",
    /// `The Bundle is not JSON.`
    BundleNotJson = "bundle-not-json",
    /// `The format is not canonical or flat.`
    FormatNotCanonicalFlat = "format-not-canonical-flat",
    /// `The composition is empty: enter a canonical or FLAT composition.`
    CompositionEmptyEnterCanonicalFlatComposition = "composition-empty-enter-canonical-flat-composition",
    /// The About page's line: `$product` is the console's name.
    AboutBody = "about-body",
    /// The heading of a REST entry's security, in the `$mode` mode.
    RestSecurity = "rest-security",
    /// The heading of a REST entry's system interactions, in the `$mode` mode.
    RestSystem = "rest-system",
    /// A stat's value: `$loaded` of `$total`.
    LoadedOf = "loaded-of",
    /// A file's tally of `$warnings` warnings.
    WarningsCount = "warnings-count",
    /// A file's tally of `$errors` errors and `$warnings` warnings.
    ErrorsWarningsCount = "errors-warnings-count",
    /// Where a diagnostic points: `$line` and `$column`.
    LineColumn = "line-column",
    /// The name of the numbered view of the file at `$path`.
    NumberedLines = "numbered-lines",
    /// The top bar's status while the bridge answers.
    BridgeUp = "bridge-up",
    /// The top bar's status while the bridge answers with its `$version`.
    BridgeUpVersion = "bridge-up-version",
    /// The top bar's status when the bridge refused with `$status`.
    BridgeRefusedCode = "bridge-refused-code",
    /// The count of the resources the Bundle holds.
    BundleHolds = "bundle-holds",
    /// The title of the file editor: `$mode` new or edit, `$name` the path of the file changed.
    FileEditor = "file-editor",
    /// The sentence of that page.
    NotFoundBody = "not-found-body",
    /// A refusal from the bridge with `$status`.
    BridgeRefusedStatus = "bridge-refused-status",
    /// The fault of a view asked for with no live session.
    ViewSignedOut = "view-signed-out",
    /// The fault of a view whose answer, with `$status`, the console cannot read.
    ViewUnreadable = "view-unreadable",
    /// The fault of a view whose bridge gave no answer.
    ViewUnreachable = "view-unreachable",
    /// The fault of a tool sent as a plain form post before its page loaded.
    ViewPlainPost = "view-plain-post",
    /// The fault of a view the console could not serve.
    ViewUnavailable = "view-unavailable",
    /// The fault of a view its server function did not deliver, for `$cause`.
    ViewFetch = "view-fetch",
    /// The fault of an answer that lacks the field at `$path`.
    ShapeMissing = "shape-missing",
    /// The refusal of a theme the console does not offer.
    ThemeInvalid = "theme-invalid",
    /// The word of a diagnostic of severity error.
    SeverityError = "severity-error",
    /// The word of a diagnostic of severity warning.
    SeverityWarning = "severity-warning",
    /// The answer to a sign-in or sign-out on a console with no `[oidc]`.
    SignInNotConfigured = "sign-in-not-configured",
    /// The answer to a sign-out whose session could not be ended.
    SessionNotEnded = "session-not-ended",
    /// The answer to a sign-out whose redirect could not be written.
    SignOutRedirectUnwritten = "sign-out-redirect-unwritten",
    /// The answer to a sign-in whose redirect could not be written.
    SignInRedirectUnwritten = "sign-in-redirect-unwritten",
    /// The answer to a redirect back with no sign-in pending.
    NoSignInPending = "no-sign-in-pending",
    /// The answer to a redirect back whose `state` is not the sign-in's.
    SignInStateMismatch = "sign-in-state-mismatch",
    /// The answer to a redirect back that carries an error.
    ProviderDidNotSignIn = "provider-did-not-sign-in",
    /// The answer to a redirect back that carries no code.
    RedirectCarriesNoCode = "redirect-carries-no-code",
    /// The answer to a sign-in whose ID Token did not verify.
    IdTokenDidNotVerify = "id-token-did-not-verify",
    /// The answer to a sign-in the provider could not complete.
    SignInNotCompleted = "sign-in-not-completed",
    /// The answer to a sign-in whose session could not be begun.
    SessionNotBegun = "session-not-begun",
    /// The answer to a sign-in while the session pool is full.
    SessionsFull = "sessions-full",
    /// The answer to a sign-in whose pending session could not be begun.
    SignInSessionNotBegun = "sign-in-session-not-begun",
    /// The answer to a state-changing request from outside the console's pages.
    ForeignRequestRefused = "foreign-request-refused",
}

/// The console's message `msg` in the page's locale.
#[must_use]
pub fn tr(msg: Msg) -> String {
    tr_in(catalog(), &locale(), msg)
}

/// The console's message `msg` in the page's locale, with the placeables
/// `args` filled in.
#[must_use]
pub fn tr_with(msg: Msg, args: &[(&'static str, Arg)]) -> String {
    locale().lookup_with(catalog(), msg.id(), args)
}

/// The console's message `msg` in `locale`, with the placeables `args`
/// filled in: for a handler or a task, where no page tree is in reach.
#[must_use]
pub fn tr_in_with(locale: &Locale, msg: Msg, args: &[(&'static str, Arg)]) -> String {
    locale.lookup_with(catalog(), msg.id(), args)
}

/// The console's message `msg` as a [`Message`] a component looks up as it
/// draws, such as a listing's noun or an editor's title.
#[must_use]
pub fn message(msg: Msg) -> Message {
    Message::new(catalog(), msg.id())
}

/// The message `msg` of `catalog` in `locale`.
#[must_use]
pub fn tr_in(catalog: &dyn Loader, locale: &Locale, msg: Msg) -> String {
    locale.lookup(catalog, msg.id())
}

/// The console's message `msg` in the locale of the request being served:
/// for a server function, which runs outside the page tree.
#[cfg(not(target_arch = "wasm32"))]
#[must_use]
pub fn tr_served(msg: Msg) -> String {
    requested().lookup(catalog(), msg.id())
}

/// The console's message `msg` with the placeables `args` filled in, in the
/// locale of the request being served.
#[cfg(not(target_arch = "wasm32"))]
#[must_use]
pub fn tr_served_with(msg: Msg, args: &[(&'static str, Arg)]) -> String {
    requested().lookup_with(catalog(), msg.id(), args)
}

/// The console's message `msg`: in the browser, where a server function's
/// body is never compiled, the page's locale.
#[cfg(target_arch = "wasm32")]
#[must_use]
pub fn tr_served(msg: Msg) -> String {
    tr(msg)
}

/// The console's message `msg` with the placeables `args` filled in, in the
/// page's locale: the browser's half of [`tr_served_with`].
#[cfg(target_arch = "wasm32")]
#[must_use]
pub fn tr_served_with(msg: Msg, args: &[(&'static str, Arg)]) -> String {
    tr_with(msg, args)
}

/// The locale of the request being served: the best language of its
/// `Accept-Language` the catalog holds, else the configured one, English
/// with no configuration in reach.
#[cfg(not(target_arch = "wasm32"))]
#[must_use]
pub fn requested() -> Locale {
    use leptos::context::use_context;
    let configured = use_context::<crate::server::ConsoleState>()
        .map_or_else(Locale::english, |state| state.settings().locale.clone());
    match use_context::<http::request::Parts>() {
        Some(parts) => negotiated(&parts.headers, configured),
        None => Locale::negotiate_with("", catalog(), configured),
    }
}

/// The locale of a request with `headers`: the best language of its
/// `Accept-Language` the catalog holds, else `configured`. For a plain axum
/// handler, which runs outside the page tree.
#[cfg(not(target_arch = "wasm32"))]
#[must_use]
pub fn negotiated(headers: &http::HeaderMap, configured: Locale) -> Locale {
    // NOTE: RFC 9110 §12.5.4: a header that is no visible ASCII names no
    // language, so the request falls to the configured one.
    let header = headers
        .get(http::header::ACCEPT_LANGUAGE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();
    Locale::negotiate_with(header, catalog(), configured)
}

#[cfg(test)]
mod tests {
    use std::borrow::Cow;
    use std::collections::HashMap;
    use std::path::Path;

    use ferrohealth_ui::i18n::Locale;
    use fluent_templates::fluent_bundle::FluentValue;

    use super::{Msg, catalog, tr_in};

    fluent_templates::static_loader! {
        static THROWAWAY = {
            locales: "./tests/locales",
            fallback_language: "qaa",
            customise: |bundle| bundle.set_use_isolating(false),
        };
    }

    /// Every placeable the catalog at `path` names, each set to a number, so
    /// a message that selects a form by one formats whole.
    fn placeables(path: &Path) -> HashMap<Cow<'static, str>, FluentValue<'static>> {
        let source = std::fs::read_to_string(path).expect("the catalog");
        source
            .split("{ $")
            .skip(1)
            .filter_map(|rest| rest.split([' ', '}']).next())
            .map(|name| (Cow::Owned(name.to_owned()), FluentValue::from(3)))
            .collect()
    }

    /// The directories under `locales/`, each a language.
    fn catalog_directories(root: &Path) -> Vec<String> {
        let mut found: Vec<String> = std::fs::read_dir(root)
            .expect("the locales directory")
            .map(|entry| {
                entry
                    .expect("an entry")
                    .file_name()
                    .to_string_lossy()
                    .into_owned()
            })
            .collect();
        found.sort();
        found
    }

    #[test]
    fn every_catalog_directory_is_a_language_holding_every_message() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("locales");
        let found = catalog_directories(&root);
        let mut shipped: Vec<String> = catalog().locales().map(ToString::to_string).collect();
        shipped.sort();
        assert_eq!(found, shipped, "a catalog directory is a language");
        assert!(
            found.contains(&String::from("en")),
            "English is the reference"
        );
        for tag in found {
            let language = tag.parse().expect("a language");
            let args = placeables(&root.join(&tag).join("ferrobridge-console.ftl"));
            for msg in Msg::ALL {
                let words = catalog().try_lookup_with_args(&language, msg.id(), &args);
                assert!(
                    words.is_some_and(|words| !words.trim().is_empty()),
                    "{tag} holds {}",
                    msg.id()
                );
            }
        }
    }

    #[test]
    fn a_throwaway_catalog_changes_the_words_with_no_code_change() {
        let english = Locale::english();
        assert_eq!("Not found", tr_in(catalog(), &english, Msg::NotFoundTitle));
        let throwaway = Locale::parse("qaa").expect("a private-use language");
        assert_eq!(
            "Qot qound",
            tr_in(&*THROWAWAY, &throwaway, Msg::NotFoundTitle)
        );
        assert_eq!(
            "Back to the console",
            tr_in(catalog(), &throwaway, Msg::NotFoundBack),
            "the shipped catalogs fall back to English"
        );
    }
}
