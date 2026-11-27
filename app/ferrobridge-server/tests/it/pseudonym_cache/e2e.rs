// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The pseudonym cache against `FerroPIX`: a repeated lookup served from the
//! cache, a merge and a revise forgotten once their ITI-93 message arrives
//! (PMIR 1.6.0 §2:3.93), the receiver refusing a caller it does not list,
//! and every lookup asking `FerroPIX` once its subscription is gone. `FerroPIX`,
//! the test OpenID Provider and the bridge's receiver all listen on the
//! loopback interface, `FerroPIX` on the host network.

use std::error::Error;
use std::sync::Arc;
use std::time::{Duration, Instant};

use ferrobridge_server::identity_source::subscription::Subscription;
use ferrobridge_server::identity_source::{IdentitySource, Running, feed};
use ferrobridge_server::smart::{Smart, SmartSettings};
use ferrobridge_server::store::Store;
use ferrobridge_testkit::{containers, ferropix, oidc::Provider};
use ferrohealth_smart::kind::IssuerKind;
use ihe_atna::user::OnBehalfOf;
use ihe_iti::pixm::identifier::SourceIdentifier;
use ihe_iti::pmir::subscription::Search;
use reqwest::StatusCode;

use super::{Kept, identity_of};

/// The domain every seeded patient's identifier is in.
const SUBJECTS: &str = "urn:oid:2.999.1.42";

/// The domain `FerroPIX` answers each pseudonym in.
const PSEUDONYMS: &str = "urn:oid:2.999.9.42";

/// The bridge's client: the ITI-83 Consumer and the ITI-94 Subscriber.
const BRIDGE: &str = "ferrobridge-e2e";

/// The Patient Identity Source of [`SUBJECTS`], feeding over ITI-104.
const SOURCE: &str = "ferrobridge-e2e-source";

/// The client `FerroPIX` sends the feed as, which the receiver admits.
const FEED: &str = "ferropix-e2e";

/// A client the provider knows and the receiver does not list.
const UNLISTED: &str = "ferrobridge-e2e-unlisted";

/// The one synthetic secret of every client.
const SECRET: &str = "synthetic-e2e-secret";

/// How often the subscription's holder reads its status.
const STATUS_EVERY: &str = "2s";

/// How long any one awaited change may take.
const WAIT: Duration = Duration::from_secs(90);

/// How long one exchange with `FerroPIX` may take.
const TIMEOUT: Duration = Duration::from_secs(10);

/// Waits until `ready` holds, or fails naming `what`.
async fn until(what: &str, ready: impl Fn() -> bool) -> Result<(), Box<dyn Error>> {
    let started = Instant::now();
    while !ready() {
        if started.elapsed() > WAIT {
            return Err(format!("{what} did not happen within {}s", WAIT.as_secs()).into());
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    Ok(())
}

/// `FerroPIX`'s configuration: its issuer, the two subject domains' Source,
/// the bridge as the pseudonym Consumer, and the feed sent to the bridge's
/// receiver under `bridge`.
fn ferropix_config(pix: &str, issuer: &str, bridge: &str) -> String {
    format!(
        r#"profile = "development"
public_base_url = "{pix}"

[dev]

[pmir]
enabled = true

[[pmir.feed.consumers]]
url = "{bridge}/pmir/feed"

[pmir.feed.consumers.credentials.oauth]
issuer = "{issuer}"
client_id = "{FEED}"
client_secret = "{SECRET}"
scope = "system/Patient.rs"

[auth.issuer.e2e]
issuer = "{issuer}"

[[trust.issuer]]
issuer = "e2e"
clients = ["{SOURCE}"]

[[domain]]
system = "{SUBJECTS}"
name = "Synthetic subjects"
kind = "local"
identifier_type = "PI"
source_client_id = "{SOURCE}"
sensitive = false

[[pseudonym_domain]]
namespace = "{PSEUDONYMS}"
name = "Synthetic pseudonyms"
consumer_client_id = "{BRIDGE}"
"#
    )
}

/// The bridge's configuration of the identity source as `client`, with the
/// cache and the state store on `state` when `cached`.
fn bridge_config(
    pix: &str,
    issuer: &str,
    client: (&str, &str),
    cached: Option<(&str, &str)>,
) -> String {
    let (client, scope) = client;
    let base = format!(
        "[identity_source]\npseudonym_domain = \"{PSEUDONYMS}\"\nsource_domains = [\"{SUBJECTS}\"]\n\n\
         [identity_source.endpoint]\nurl = \"{pix}\"\n\n\
         [identity_source.endpoint.credentials.oauth]\nissuer = \"{issuer}\"\nclient_id = \"{client}\"\n\
         client_secret = \"{SECRET}\"\nscope = \"{scope}\"\n"
    );
    let cache = cached.map(|(bridge, state)| {
        format!(
            "\n[identity_source.cache]\nfeed_url = \"{bridge}/pmir/feed\"\nfeed_clients = [\"{FEED}\"]\n\
             status_every = \"{STATUS_EVERY}\"\n\n\
             [state]\nurl = \"{state}\"\n\n\
             [state.seal]\nlookup_key = \"QkJCQkJCQkJCQkJCQkJCQkJCQkJCQkJCQkJCQkJCQkI=\"\n\n\
             [[state.seal.keys]]\nid = \"e2e\"\nkey = \"QUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUE=\"\n"
        )
    });
    format!("{base}{}", cache.unwrap_or_default())
}

/// A synthetic patient known in [`SUBJECTS`] by `value`.
fn patient(
    value: &str,
    given: &str,
    born: &str,
) -> Result<fhir_types::r4::patient::Patient, Box<dyn Error>> {
    use fhir_types::codec::Json as _;
    let read = fhir_types::codec::Value::from_serde_json(serde_json::json!({
        "resourceType": "Patient",
        "identifier": [{ "system": SUBJECTS, "value": value }],
        "name": [{ "family": "Cachetest", "given": [given] }],
        "birthDate": born,
        "gender": "unknown",
    }));
    let fhir_types::codec::Value::Object(object) = read else {
        return Err("a patient is an object".into());
    };
    Ok(fhir_types::r4::patient::Patient::from_json(
        &object,
        &mut fhir_types::codec::Path::root("Patient"),
    )?)
}

/// The started stack: the provider, `FerroPIX`, the bridge's cache and
/// receiver, and the Source that feeds `FerroPIX`.
struct Stack {
    /// The test OpenID Provider.
    provider: Provider,
    /// `FerroPIX` and its database.
    pix: ferropix::FerroPix,
    /// The bridge's state store.
    _state: containers::Postgres,
    /// The bridge's state store, where the holder records the status.
    store: Store,
    /// The bridge's origin, `http://127.0.0.1:<port>`.
    origin: String,
    /// The identity source the facade would ask.
    identity: IdentitySource,
    /// The cache and the subscription's holder.
    running: Running,
    /// The subscription the holder keeps.
    subscription: Subscription,
    /// Every audited exchange of the bridge with `FerroPIX`.
    kept: Arc<Kept>,
    /// The Patient Identity Source of [`SUBJECTS`].
    source: IdentitySource,
    /// Stops the holder.
    stop: tokio::sync::watch::Sender<bool>,
    /// The holder's task.
    holder: tokio::task::JoinHandle<()>,
    /// The receiver's task.
    served: tokio::task::JoinHandle<std::io::Result<()>>,
}

impl Stack {
    /// Starts the stack; the bridge serves its receiver and holds its
    /// subscription.
    async fn start() -> Result<Self, Box<dyn Error>> {
        let provider = Provider::start().await?;
        provider.publish_metadata().await;
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let origin = format!("http://{}", listener.local_addr()?);
        let audience = format!("{origin}/fhir");
        let pix_port = std::net::TcpListener::bind("127.0.0.1:0")?
            .local_addr()?
            .port();
        let pix_origin = ferropix::origin_on(pix_port);
        let pix_audience = format!("{pix_origin}/fhir");
        let clients: Vec<(&str, &str)> = [BRIDGE, SOURCE, FEED, UNLISTED]
            .into_iter()
            .map(|client| (client, SECRET))
            .collect();
        provider
            .issue_client_credentials(&clients, &[pix_audience.as_str(), audience.as_str()])
            .await;
        let pix = ferropix::start(
            pix_port,
            &ferropix_config(&pix_origin, &provider.issuer(), &origin),
        )
        .await?;
        let state = containers::postgres().await?;
        let settings = ferrobridge_server::config::Config::from_sources(
            Some(&bridge_config(
                &pix.fhir_base(),
                &provider.issuer(),
                (BRIDGE, "system/Patient.rs system/Subscription.cruds"),
                Some((&origin, state.url())),
            )),
            &std::collections::BTreeMap::new(),
        )?
        .resolve(ferrobridge_server::config::Process::Job)?;
        let store = Store::open(
            settings
                .state
                .as_deref()
                .ok_or("the state store is configured")?,
        )
        .await?;
        let kept = Arc::new(Kept::default());
        let identity = settings
            .identity_source
            .as_deref()
            .ok_or("the identity source is resolved")?
            .clone()
            .audited(kept.clone());
        let cache_settings = identity
            .cache_settings()
            .ok_or("the cache is configured")?
            .clone();
        let running = Running::start(&cache_settings, &store)?;
        store.listeners(&[running.cache.listened()]).spawn();
        let (stop, stopped) = tokio::sync::watch::channel(false);
        let holder = running.holder.clone().spawn(&stopped);
        let smart = Smart::discover(SmartSettings::new(
            ferrohealth_smart::resource::Settings::new(
                provider.issuer(),
                &audience,
                IssuerKind::Smart,
            ),
        ))
        .await?;
        let receiver = feed::routes(
            running.cache.clone(),
            format!("{origin}{}", feed::PATH),
            &smart,
            vec![String::from(FEED)],
        );
        let served = tokio::spawn(async move { axum::serve(listener, receiver).await });
        let source = identity_of(&bridge_config(
            &pix.fhir_base(),
            &provider.issuer(),
            (SOURCE, "system/Patient.cud"),
            None,
        ))?;
        Ok(Self {
            provider,
            pix,
            _state: state,
            store,
            origin,
            identity: identity.with_cache(running.cache.clone()),
            subscription: cache_settings.subscription,
            running,
            kept,
            source,
            stop,
            holder,
            served,
        })
    }

    /// What the bridge and `FerroPIX` say about the subscription, for a
    /// case that waited for it in vain.
    async fn diagnosis(&self) -> String {
        let recorded: Result<Vec<(String, Option<String>)>, sqlx::Error> =
            sqlx::query_as("SELECT status, location FROM pmir_subscription")
                .fetch_all(self.store.pool())
                .await;
        let subscription = &self.subscription;
        let found = subscription
            .subscriber
            .find(&subscription.request, TIMEOUT)
            .await;
        let created = subscription
            .subscriber
            .subscribe(&subscription.request, TIMEOUT)
            .await;
        let created = match created {
            Ok(subscribed) => format!(
                "{:?}",
                subscription.subscriber.status(&subscribed, TIMEOUT).await
            ),
            Err(error) => format!("{error:?}"),
        };
        let log = self.pix.log().await;
        let tail: Vec<&str> = log.lines().rev().take(40).collect();
        format!(
            "listening: {}\nrecorded: {recorded:?}\nfound: {found:?}\ncreated: {created}\nFerroPIX log, newest first:\n{}",
            self.running.cache.listened().is_listening(),
            tail.join("\n")
        )
    }

    /// How many ITI-83 queries reached `FerroPIX`.
    fn asked(&self) -> usize {
        self.kept.count(&ihe_iti::pixm::audit::query_consumer())
    }

    /// The pseudonym of `source`.
    async fn pseudonym(&self, source: &SourceIdentifier) -> Result<String, Box<dyn Error>> {
        let person = self.identity.pseudonym(source, &OnBehalfOf::System).await?;
        Ok(person.id().to_owned())
    }

    /// Looks `source` up twice, and returns its pseudonym once the second
    /// lookup was served from the cache.
    async fn cached(&self, source: &SourceIdentifier) -> Result<String, Box<dyn Error>> {
        let before = self.asked();
        let first = self.pseudonym(source).await?;
        assert_eq!(before + 1, self.asked(), "the first lookup asks FerroPIX");
        let second = self.pseudonym(source).await?;
        assert_eq!(first, second, "the cache answers what FerroPIX answered");
        assert_eq!(before + 1, self.asked(), "the second lookup is the cache's");
        Ok(first)
    }

    /// Looks `source` up once a change after `seen` reached the cache, and
    /// returns its pseudonym, which `FerroPIX` was asked for again.
    async fn asked_again(
        &self,
        what: &str,
        seen: u64,
        source: &SourceIdentifier,
    ) -> Result<String, Box<dyn Error>> {
        until(what, || self.running.cache.changes() > seen).await?;
        let asked = self.asked();
        let pseudonym = self.pseudonym(source).await?;
        assert_eq!(
            asked + 1,
            self.asked(),
            "after {what}, FerroPIX is asked again"
        );
        Ok(pseudonym)
    }

    /// Stops the holder and the receiver.
    async fn stop(self) -> Result<(), Box<dyn Error>> {
        self.stop.send(true)?;
        self.holder.await?;
        self.served.abort();
        Ok(())
    }
}

/// Signs `client` in at `provider` and returns its access token.
async fn token(provider: &Provider, client: &str) -> Result<String, Box<dyn Error>> {
    let answer: serde_json::Value = reqwest::Client::new()
        .post(provider.token_endpoint())
        .basic_auth(client, Some(SECRET))
        .header("content-type", "application/x-www-form-urlencoded")
        .body("grant_type=client_credentials&scope=system%2FPatient.rs")
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    Ok(answer["access_token"]
        .as_str()
        .ok_or("the provider answers an access token")?
        .to_owned())
}

/// Posts an empty body to the receiver, bearing `bearer` when given.
async fn posted(stack: &Stack, bearer: Option<&str>) -> Result<StatusCode, Box<dyn Error>> {
    let mut request = reqwest::Client::new()
        .post(format!("{}{}", stack.origin, feed::PATH))
        .header("content-type", "application/fhir+json")
        .body("{}");
    if let Some(bearer) = bearer {
        request = request.bearer_auth(bearer);
    }
    Ok(request.send().await?.status())
}

/// Deletes the subscription the holder keeps.
async fn unsubscribe(stack: &Stack) -> Result<(), Box<dyn Error>> {
    let subscription = &stack.subscription;
    let Search::Found(listed) = subscription
        .subscriber
        .find(&subscription.request, TIMEOUT)
        .await?
    else {
        return Err("FerroPIX searches its subscriptions by channel endpoint".into());
    };
    assert!(!listed.is_empty(), "the holder subscribed");
    for found in &listed {
        subscription
            .subscriber
            .unsubscribe(&found.subscribed, TIMEOUT)
            .await?;
    }
    Ok(())
}

#[tokio::test]
async fn the_pseudonym_cache_follows_ferropix_through_its_pmir_feed() -> Result<(), Box<dyn Error>>
{
    if !containers::e2e_enabled() {
        return Ok(());
    }
    let stack = Box::pin(Stack::start()).await?;
    let source = &stack.source;
    let a = source.source(Some(SUBJECTS), "synthetic-cache-a")?;
    let b = source.source(Some(SUBJECTS), "synthetic-cache-b")?;
    let c = source.source(Some(SUBJECTS), "synthetic-cache-c")?;
    let patient_a = patient("synthetic-cache-a", "Alpha", "1971-02-03")?;
    source.feed(&a, &patient_a).await?;
    source
        .feed(&b, &patient("synthetic-cache-b", "Bravo", "1982-04-05")?)
        .await?;
    source
        .feed(&c, &patient("synthetic-cache-c", "Charlie", "1993-06-07")?)
        .await?;
    if let Err(error) = until("the subscription's first active announcement", || {
        stack.running.cache.serving()
    })
    .await
    {
        return Err(format!("{error}\n{}", stack.diagnosis().await).into());
    }

    // A merge: once its ITI-93 message arrives, the subsumed patient resolves to the survivor.
    let pseudonym_a = stack.cached(&a).await?;
    let pseudonym_b = stack.cached(&b).await?;
    assert_ne!(pseudonym_a, pseudonym_b, "two patients, two pseudonyms");
    let seen = stack.running.cache.changes();
    source.merge((&a, &b), &patient_a).await?;
    assert_eq!(
        pseudonym_b,
        stack
            .asked_again("the merge's ITI-93 message", seen, &a)
            .await?,
        "the subsumed patient resolves to the survivor's pseudonym"
    );

    // A revise forgets the patient it names.
    let pseudonym_c = stack.cached(&c).await?;
    let seen = stack.running.cache.changes();
    source
        .feed(
            &c,
            &patient("synthetic-cache-c", "Charlotte", "1993-06-07")?,
        )
        .await?;
    assert_eq!(
        pseudonym_c,
        stack
            .asked_again("the revise's ITI-93 message", seen, &c)
            .await?,
        "a revise keeps the patient's pseudonym"
    );

    // The receiver admits only the clients it lists.
    assert_eq!(StatusCode::UNAUTHORIZED, posted(&stack, None).await?);
    let unlisted = token(&stack.provider, UNLISTED).await?;
    assert_eq!(
        StatusCode::FORBIDDEN,
        posted(&stack, Some(&unlisted)).await?
    );

    // Once the subscription is gone, the cache stops serving and the next lookup asks FerroPIX.
    stack.cached(&b).await?;
    let seen = stack.running.cache.changes();
    unsubscribe(&stack).await?;
    until("the unreachable announcement", || {
        !stack.running.cache.serving()
    })
    .await?;
    assert_eq!(
        pseudonym_b,
        stack.asked_again("the gap", seen, &b).await?,
        "FerroPIX answers as before"
    );
    Box::pin(stack.stop()).await
}
