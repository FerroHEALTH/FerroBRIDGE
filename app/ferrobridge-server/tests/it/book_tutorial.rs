// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The book's tutorial, "Your first mapping", run as written.
//!
//! Each fenced block after a `<!-- book-file: PATH -->` marker is a file the
//! reader writes; each `http` block after a `<!-- book-request: NAME ... -->`
//! marker is a request the reader sends, in page order, to a server started
//! through the shipped serve path from the page's own configuration. A
//! request marker states the status it expects (`expect=`) and the JSON
//! pointers the answer must hold (`holds=/pointer=value`). A page edit that changes what the server answers, or
//! a server change that breaks a documented request, fails here. No
//! specification governs the marker format: our own design.

use ferrobridge_server::boot::serve_settings;
use ferrobridge_server::config::{Config, Process};
use ferrobridge_server::retry::Backoff;
use std::collections::BTreeMap;
use std::error::Error;
use std::net::SocketAddr;
use std::path::Path;
use std::time::Duration;
use tokio::net::TcpListener;

/// The tutorial page.
const PAGE: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../website/book/src/tutorials/first-mapping.md"
));

/// The repository path the page downloads the template from, the fixture
/// `DIAGNOSE_OPT` reads.
const TEMPLATE_PATH: &str = "tools/ferrobridge-testkit/fixtures/opt/diagnose.opt";

/// The table that keeps the served page from sending the usage report, which
/// the page's configuration must carry, since a test server never reports.
const USAGE_REPORT_OFF: &str = "[usage_report]\nenabled = false";

/// The fewest requests the page holds; a page that loses its steps fails.
const MIN_REQUESTS: usize = 5;

/// A wait short enough for a test; the page's server needs no CDR.
const BACKOFF: Backoff = Backoff {
    initial: Duration::from_millis(20),
    max: Duration::from_millis(50),
};

/// A file the reader writes.
struct File {
    /// The path relative to the working directory.
    path: String,
    /// The content.
    content: String,
}

/// A request the reader sends and what its answer must hold.
struct Step {
    /// The marker's name.
    name: String,
    /// The status the answer carries.
    expect: u16,
    /// The JSON pointers the answer holds, each with its value as text.
    holds: Vec<(String, String)>,
    /// The `http` block: the request line, the headers and the body.
    block: String,
}

/// The page read into its files and its requests.
struct Tutorial {
    files: Vec<File>,
    steps: Vec<Step>,
}

/// Returns the content of the fenced block that opens on the next line of
/// `lines`, leaving `lines` after its closing fence.
fn fenced<'a>(
    lines: &mut impl Iterator<Item = &'a str>,
    marker: &str,
) -> Result<String, Box<dyn Error>> {
    let open = lines
        .next()
        .ok_or_else(|| format!("{marker} is followed by no block"))?;
    if !open.starts_with("```") {
        return Err(format!("{marker} is followed by `{open}`, not a fenced block").into());
    }
    let mut content = String::new();
    for line in lines.by_ref() {
        if line.starts_with("```") {
            return Ok(content);
        }
        content.push_str(line);
        content.push('\n');
    }
    Err(format!("the block after {marker} never closes").into())
}

/// Reads one request marker's attributes.
fn step(marker: &str, block: String) -> Result<Step, Box<dyn Error>> {
    let mut words = marker.split_whitespace();
    let name = words.next().ok_or("a request marker names its request")?;
    let mut parsed = Step {
        name: name.to_owned(),
        expect: 0,
        holds: Vec::new(),
        block,
    };
    for word in words {
        let (key, value) = word
            .split_once('=')
            .ok_or_else(|| format!("{name}: `{word}` is no key=value"))?;
        match key {
            "expect" => parsed.expect = value.parse()?,
            "holds" => {
                let (pointer, wanted) = value
                    .split_once('=')
                    .ok_or_else(|| format!("{name}: `{value}` is no /pointer=value"))?;
                parsed.holds.push((pointer.to_owned(), wanted.to_owned()));
            }
            _ => return Err(format!("{name}: unknown attribute `{key}`").into()),
        }
    }
    if parsed.expect == 0 {
        return Err(format!("{name} states no expected status").into());
    }
    Ok(parsed)
}

/// Reads the page into its files and its requests, in page order.
fn tutorial(page: &str) -> Result<Tutorial, Box<dyn Error>> {
    let mut files = Vec::new();
    let mut steps = Vec::new();
    let mut lines = page.lines();
    while let Some(line) = lines.next() {
        let line = line.trim();
        if let Some(path) = line
            .strip_prefix("<!-- book-file:")
            .and_then(|rest| rest.strip_suffix("-->"))
        {
            let content = fenced(&mut lines, line)?;
            files.push(File {
                path: path.trim().to_owned(),
                content,
            });
        } else if let Some(marker) = line
            .strip_prefix("<!-- book-request:")
            .and_then(|rest| rest.strip_suffix("-->"))
        {
            let block = fenced(&mut lines, line)?;
            steps.push(step(marker, block)?);
        }
    }
    Ok(Tutorial { files, steps })
}

/// Writes the page's files under `root` and the template the page downloads,
/// and returns the configuration with its directories made absolute under
/// `root`, as running from the working directory resolves them.
fn working_directory(tutorial: &Tutorial, root: &Path) -> Result<String, Box<dyn Error>> {
    assert!(
        PAGE.contains(TEMPLATE_PATH),
        "the page downloads the template from {TEMPLATE_PATH}"
    );
    let templates = root.join("templates");
    std::fs::create_dir_all(&templates)?;
    std::fs::write(
        templates.join("diagnose.opt"),
        ferrobridge_testkit::fixtures::DIAGNOSE_OPT,
    )?;
    let mut configuration = None;
    for file in &tutorial.files {
        let path = root.join(&file.path);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&path, &file.content)?;
        if file.path == "ferrobridge.toml" {
            configuration = Some(file.content.clone());
        }
    }
    let configuration = configuration.ok_or("the page writes ferrobridge.toml")?;
    assert!(
        configuration.contains(USAGE_REPORT_OFF),
        "the page's configuration switches the usage report off"
    );
    let mut absolute = configuration.clone();
    for (key, directory) in [("directory", "mappings"), ("templates", "templates")] {
        let relative = format!("{key} = \"{directory}\"");
        assert!(
            configuration.contains(&relative),
            "the page's configuration names `{relative}`"
        );
        let full = root.join(directory);
        absolute = absolute.replace(
            &relative,
            &format!("{key} = {:?}", full.display().to_string()),
        );
    }
    Ok(absolute)
}

/// One request of an `http` block.
struct Request {
    method: String,
    path: String,
    headers: Vec<(String, String)>,
    body: String,
}

/// Splits an `http` block into its method, path, headers and body.
fn request(text: &str) -> Result<Request, Box<dyn Error>> {
    let (head, body) = text.split_once("\n\n").unwrap_or((text, ""));
    let mut head_lines = head.lines();
    let request_line = head_lines
        .next()
        .ok_or("an http block opens with its request line")?;
    let (method, path) = request_line
        .split_once(' ')
        .ok_or_else(|| format!("`{request_line}` is no request line"))?;
    let mut headers = Vec::new();
    for line in head_lines {
        let (name, value) = line
            .split_once(':')
            .ok_or_else(|| format!("`{line}` is no header"))?;
        headers.push((name.trim().to_owned(), value.trim().to_owned()));
    }
    Ok(Request {
        method: method.to_owned(),
        path: path.trim().to_owned(),
        headers,
        body: body.trim_end().to_owned(),
    })
}

/// The text a JSON value compares as: a string as itself, anything else as
/// its JSON form.
fn text_of(value: &serde_json::Value) -> String {
    value
        .as_str()
        .map_or_else(|| value.to_string(), str::to_owned)
}

/// Sends every request of `tutorial` to `address` in order and checks each
/// answer against its marker.
async fn send_all(tutorial: &Tutorial, address: SocketAddr) -> Result<(), Box<dyn Error>> {
    let client = reqwest::Client::new();
    for step in &tutorial.steps {
        let sent = request(&step.block)?;
        let mut call = client.request(
            reqwest::Method::from_bytes(sent.method.as_bytes())?,
            format!("http://{address}{}", sent.path),
        );
        for (name, value) in sent.headers {
            call = call.header(name, value);
        }
        let response = call.body(sent.body).send().await?;
        let status = response.status().as_u16();
        let text = response.text().await?;
        assert_eq!(step.expect, status, "{}: {text}", step.name);
        let answer: serde_json::Value = serde_json::from_str(&text)
            .map_err(|error| format!("{}: the answer is no JSON ({error}): {text}", step.name))?;
        for (pointer, wanted) in &step.holds {
            let found = answer
                .pointer(pointer)
                .ok_or_else(|| format!("{}: the answer holds no {pointer}: {text}", step.name))?;
            assert_eq!(
                wanted,
                &text_of(found),
                "{}: {pointer} in {text}",
                step.name
            );
        }
    }
    Ok(())
}

#[tokio::test]
async fn every_request_of_the_tutorial_answers_as_the_page_says() -> Result<(), Box<dyn Error>> {
    let tutorial = tutorial(PAGE)?;
    assert!(
        tutorial.steps.len() >= MIN_REQUESTS,
        "the page holds {} requests, at least {MIN_REQUESTS} expected",
        tutorial.steps.len()
    );
    let root = tempfile::tempdir()?;
    let configuration = working_directory(&tutorial, root.path())?;
    let settings =
        Config::from_sources(Some(&configuration), &BTreeMap::new())?.resolve(Process::Server)?;
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let address = listener.local_addr()?;
    let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
    let serving = serve_settings(
        &settings,
        move || async move { Ok(listener) },
        BACKOFF,
        async move {
            let _closed = stopped.await.is_err();
        },
    );
    let sending = async {
        let sent = send_all(&tutorial, address).await;
        stop.send(()).map_err(|()| "the server is gone")?;
        sent
    };
    let (served, sent) = tokio::join!(serving, sending);
    sent?;
    served?;
    Ok(())
}

#[test]
fn the_tutorial_mapping_passes_the_check_the_page_runs() -> Result<(), Box<dyn Error>> {
    let tutorial = tutorial(PAGE)?;
    let root = tempfile::tempdir()?;
    let configuration = working_directory(&tutorial, root.path())?;
    let path = root.path().join("ferrobridge.toml");
    std::fs::write(&path, configuration)?;
    assert!(
        PAGE.contains("ferrobridge --config ferrobridge.toml mapping check"),
        "the page runs the check this test runs"
    );
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_ferrobridge"))
        .arg("--config")
        .arg(&path)
        .args(["mapping", "check"])
        .env_remove("FERROBRIDGE_CONFIG")
        .output()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "{stdout}{}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(())
}
