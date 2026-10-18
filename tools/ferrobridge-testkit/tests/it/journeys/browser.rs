// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! One headless Chrome session over `WebDriver`, with the explicit waits and
//! the console check every journey uses.
//!
//! Each session runs through a chromedriver of its own, started on a port
//! the system picks and stopped with the session, so no journey shares a
//! driver's listen queue or its lifetime with another run. A chromedriver
//! that exits because the port it picked is bound already is started again,
//! and any other exit before it listens fails the session with what it
//! wrote. Only when
//! [`WEBDRIVER_ENV`] names an endpoint does a session use that driver
//! instead.
//!
//! The session asks chromedriver for the browser log (`goog:loggingPrefs`),
//! and [`Browser::console_clean`] drains it and fails on any entry at the
//! `SEVERE` level: an error a script logged, an uncaught error or panic, a
//! refused Content-Security-Policy, or a resource the page could not load.
//! Every wait polls the page for an element or a state until
//! [`WAIT`] runs out, never a fixed sleep.

use std::error::Error;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use tempfile::NamedTempFile;
use thirtyfour::LoggingPrefsLogLevel;
use thirtyfour::prelude::*;
use tokio::io::{AsyncBufReadExt, BufReader, Lines};
use tokio::process::{Child, ChildStdout, Command};

/// The environment variable naming a running `WebDriver` endpoint to use
/// instead of a chromedriver of the session's own.
const WEBDRIVER_ENV: &str = "FERROBRIDGE_WEBDRIVER";

/// The environment variable naming the chromedriver binary, when it is not
/// the one in [`installed`], or else [`CHROMEDRIVER`] on `PATH`.
const CHROMEDRIVER_ENV: &str = "FERROBRIDGE_CHROMEDRIVER";

/// The chromedriver binary looked up on `PATH`.
const CHROMEDRIVER: &str = "chromedriver";

/// What chromedriver prints once it listens, before the port it chose.
const LISTENING: &str = "ChromeDriver was started successfully on port ";

/// What chromedriver logs when the port it picked is bound already.
const PORT_TAKEN: &str = "Address already in use";

/// How many times a chromedriver is started before a port bound already
/// fails the session.
const STARTS: u32 = 5;

/// The environment variable naming the Chrome binary, when it is not the one
/// in [`installed`], or else the one chromedriver finds itself.
const CHROME_ENV: &str = "FERROBRIDGE_CHROME";

/// Where `scripts/release/journey-tools.sh` unpacks chromedriver and Chrome
/// for this platform, under the workspace's `target/journey-tools`: the
/// Chrome for Testing platform name, and the Chrome binary inside its
/// directory.
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
const INSTALLED: Option<(&str, &str)> = Some(("linux64", "chrome"));
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
const INSTALLED: Option<(&str, &str)> = Some((
    "mac-arm64",
    "Google Chrome for Testing.app/Contents/MacOS/Google Chrome for Testing",
));
#[cfg(not(any(
    all(target_os = "linux", target_arch = "x86_64"),
    all(target_os = "macos", target_arch = "aarch64")
)))]
const INSTALLED: Option<(&str, &str)> = None;

/// The chromedriver and the Chrome binary `scripts/release/journey-tools.sh`
/// installed for this platform, each when it is there.
fn installed() -> (Option<PathBuf>, Option<PathBuf>) {
    let Some((platform, chrome)) = INSTALLED else {
        return (None, None);
    };
    let tools = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/journey-tools");
    let driver = tools.join(format!("chromedriver-{platform}/chromedriver"));
    let chrome = tools.join(format!("chrome-{platform}")).join(chrome);
    (
        driver.is_file().then_some(driver),
        chrome.is_file().then_some(chrome),
    )
}

/// How long a wait polls before it fails.
pub(crate) const WAIT: Duration = Duration::from_secs(20);

/// How often a wait polls.
const POLL: Duration = Duration::from_millis(100);

/// The log level chromedriver reports a console error, an uncaught error and
/// a failed load at.
const SEVERE: &str = "SEVERE";

/// A chromedriver one session runs through, in a process group of its own
/// that is killed when it is dropped, so the Chrome it launched goes with it
/// even when a journey panics.
#[derive(Debug)]
struct Chromedriver {
    /// The chromedriver process.
    child: Child,
    /// Its standard output, held open so a later line never meets a closed
    /// pipe.
    _stdout: Lines<BufReader<ChildStdout>>,
    /// The file its standard error goes to, read when it exits before it
    /// listens.
    _stderr: NamedTempFile,
}

/// How one start of a chromedriver ended.
#[derive(Debug)]
enum Start {
    /// It listens on the port.
    Listening(Box<Chromedriver>, u16),
    /// It exited because the port it picked was bound already, with what it
    /// wrote.
    PortTaken(String),
}

impl Chromedriver {
    /// Starts a chromedriver on a port the system picks, waits until it
    /// says it listens, and returns it with its endpoint.
    async fn start() -> Result<(Self, String), Box<dyn Error>> {
        let binary = std::env::var_os(CHROMEDRIVER_ENV)
            .map(PathBuf::from)
            .or_else(|| installed().0)
            .unwrap_or_else(|| PathBuf::from(CHROMEDRIVER));
        Self::start_from(&binary).await
    }

    /// Starts the chromedriver `binary` as [`Chromedriver::start`] does,
    /// starting it again, at most [`STARTS`] times in all, while it exits
    /// because the port it picked for itself was bound already.
    async fn start_from(binary: &Path) -> Result<(Self, String), Box<dyn Error>> {
        let mut last = String::new();
        for _ in 0..STARTS {
            match Self::start_once(binary).await? {
                Start::Listening(chromedriver, port) => {
                    return Ok((*chromedriver, format!("http://127.0.0.1:{port}")));
                }
                // NOTE: no specification governs this: our own design; chromedriver picks the
                // port of `--port=0` itself, and a port another process took meanwhile is retried.
                Start::PortTaken(output) => last = output,
            }
        }
        Err(format!("chromedriver found its port bound already {STARTS} times:\n{last}").into())
    }

    /// Starts the chromedriver `binary` once and waits until it says it
    /// listens, or exits.
    async fn start_once(binary: &Path) -> Result<Start, Box<dyn Error>> {
        let stderr = NamedTempFile::new()?;
        let mut command = Command::new(binary);
        #[cfg(unix)]
        command.process_group(0);
        let mut child = command
            .arg("--port=0")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::from(stderr.reopen()?))
            .kill_on_drop(true)
            .spawn()
            .map_err(|error| {
                format!(
                    "{} does not start (run scripts/release/journey-tools.sh target/journey-tools, or name chromedriver in {CHROMEDRIVER_ENV}): {error}",
                    binary.display()
                )
            })?;
        let stdout = child
            .stdout
            .take()
            .ok_or("chromedriver's standard output")?;
        let mut lines = BufReader::new(stdout).lines();
        let mut written = Vec::new();
        let port = tokio::time::timeout(WAIT, async {
            while let Some(line) = lines.next_line().await? {
                if let Some(port) = line.strip_prefix(LISTENING) {
                    return Ok::<Option<u16>, Box<dyn Error>>(Some(
                        port.trim_end_matches('.').parse::<u16>()?,
                    ));
                }
                written.push(line);
            }
            Ok(None)
        })
        .await
        .map_err(|_elapsed| format!("chromedriver never listened within {WAIT:?}"))??;
        if let Some(port) = port {
            let chromedriver = Self {
                child,
                _stdout: lines,
                _stderr: stderr,
            };
            return Ok(Start::Listening(Box::new(chromedriver), port));
        }
        tokio::time::timeout(WAIT, child.wait())
            .await
            .map_err(|_elapsed| {
                format!("chromedriver closed its output and never exited within {WAIT:?}")
            })??;
        written.push(std::fs::read_to_string(stderr.path())?);
        let output = written.join("\n");
        if output.contains(PORT_TAKEN) {
            return Ok(Start::PortTaken(output));
        }
        Err(format!("chromedriver exited before it listened:\n{output}").into())
    }
}

impl Drop for Chromedriver {
    /// Kills the chromedriver's process group: the driver, and every Chrome
    /// process it started.
    fn drop(&mut self) {
        #[cfg(unix)]
        if let Some(group) = self
            .child
            .id()
            .and_then(|id| i32::try_from(id).ok())
            .and_then(rustix::process::Pid::from_raw)
        {
            // NOTE: no specification governs this: our own design; a group
            // that has already exited (ESRCH) leaves nothing to stop.
            let _stopped: rustix::io::Result<()> =
                rustix::process::kill_process_group(group, rustix::process::Signal::KILL);
        }
    }
}

/// A browser session a journey drives.
#[derive(Debug)]
pub(crate) struct Browser {
    /// The `WebDriver` session.
    driver: WebDriver,
    /// The chromedriver of this session alone, unless [`WEBDRIVER_ENV`]
    /// named a running one.
    chromedriver: Option<Chromedriver>,
}

impl Browser {
    /// Starts a headless Chrome session through a chromedriver of its own,
    /// or at the endpoint [`WEBDRIVER_ENV`] names.
    pub(crate) async fn open() -> Result<Self, Box<dyn Error>> {
        let mut capabilities = DesiredCapabilities::chrome();
        capabilities.set_headless()?;
        capabilities.set_no_sandbox()?;
        capabilities.set_disable_dev_shm_usage()?;
        capabilities.add_arg("--window-size=1280,1024")?;
        capabilities.set_browser_log_level(LoggingPrefsLogLevel::Info)?;
        let chrome = std::env::var_os(CHROME_ENV)
            .map(PathBuf::from)
            .or_else(|| installed().1);
        if let Some(chrome) = chrome {
            let chrome = chrome
                .to_str()
                .ok_or_else(|| format!("{} is no UTF-8 path", chrome.display()))?;
            capabilities.set_binary(chrome)?;
        }
        let (chromedriver, endpoint) = match std::env::var(WEBDRIVER_ENV) {
            Ok(endpoint) => (None, endpoint),
            Err(_unset) => {
                let (chromedriver, endpoint) = Chromedriver::start().await?;
                (Some(chromedriver), endpoint)
            }
        };
        let driver = WebDriver::new(endpoint.as_str(), capabilities)
            .await
            .map_err(|error| format!("no WebDriver session at {endpoint}: {error}"))?;
        Ok(Self {
            driver,
            chromedriver,
        })
    }

    /// Ends the session, closes the browser and stops the session's own
    /// chromedriver.
    pub(crate) async fn quit(self) -> Result<(), Box<dyn Error>> {
        let quit = self.driver.quit().await;
        drop(self.chromedriver);
        quit?;
        Ok(())
    }

    /// The `WebDriver` session.
    pub(crate) fn driver(&self) -> &WebDriver {
        &self.driver
    }

    /// Loads `url` and waits for the document to load.
    pub(crate) async fn goto(&self, url: &str) -> Result<(), Box<dyn Error>> {
        self.driver.goto(url).await?;
        Ok(())
    }

    /// Drains the browser log and returns every `SEVERE` entry it held since
    /// the last call, each as its level and message.
    pub(crate) async fn console_errors(&self) -> Result<Vec<String>, Box<dyn Error>> {
        let entries = self.driver.browser_log().await?;
        Ok(entries
            .iter()
            .filter(|entry| entry.level == SEVERE)
            .map(|entry| format!("{} {}", entry.level, entry.message))
            .collect())
    }

    /// Fails on any `SEVERE` entry the browser logged since the last call,
    /// naming `step` and every entry.
    pub(crate) async fn console_clean(&self, step: &str) -> Result<(), Box<dyn Error>> {
        let errors = self.console_errors().await?;
        if errors.is_empty() {
            return Ok(());
        }
        Err(format!(
            "the browser logged {} console error(s) during {step}:\n{}",
            errors.len(),
            errors.join("\n")
        )
        .into())
    }

    /// Waits for the element `by` names, and returns it.
    ///
    /// A wait that runs out names the page the browser is at and its text.
    pub(crate) async fn element(&self, by: By) -> Result<WebElement, Box<dyn Error>> {
        let found = self.driver.query(by.clone()).wait(WAIT, POLL).first().await;
        match found {
            Ok(element) => Ok(element),
            Err(error) => Err(format!(
                "no element {by} at {}: {error}\nthe page reads:\n{}",
                self.driver.current_url().await?,
                self.driver.find(By::Tag("body")).await?.text().await?
            )
            .into()),
        }
    }

    /// Waits for the element `by` names, then until it is enabled.
    pub(crate) async fn enabled(&self, by: By) -> Result<(), Box<dyn Error>> {
        self.element(by)
            .await?
            .wait_until()
            .wait(WAIT, POLL)
            .enabled()
            .await?;
        Ok(())
    }

    /// Waits for an element of `tag` whose text holds `text`, and returns it.
    ///
    /// `text` must hold no apostrophe, since it is quoted in an `XPath`.
    pub(crate) async fn text(&self, tag: &str, text: &str) -> Result<WebElement, Box<dyn Error>> {
        self.element(By::XPath(format!("//{tag}[contains(., '{text}')]")))
            .await
    }

    /// Waits for the link whose text is `text`, and follows it.
    pub(crate) async fn follow(&self, text: &str) -> Result<(), Box<dyn Error>> {
        self.element(By::LinkText(text)).await?.click().await?;
        Ok(())
    }

    /// Waits until the document's title is `title`.
    pub(crate) async fn titled(&self, title: &str) -> Result<(), Box<dyn Error>> {
        self.element(By::XPath(format!("//title[. = '{title}']")))
            .await
            .map_err(|error| format!("the page is never titled {title:?}: {error}"))?;
        Ok(())
    }

    /// Waits until the browser is at `url`.
    pub(crate) async fn at(&self, url: &str) -> Result<(), Box<dyn Error>> {
        let arrived = until(&format!("the browser is at {url}"), async || {
            Ok(self.driver.current_url().await?.as_str() == url)
        })
        .await;
        match arrived {
            Ok(()) => Ok(()),
            Err(error) => {
                let entries = self.driver.browser_log().await?;
                Err(format!(
                    "{error}; it is at {}, and logged {entries:?}",
                    self.driver.current_url().await?
                )
                .into())
            }
        }
    }

    /// The cookie the browser holds for the page under `name`, if any.
    pub(crate) async fn cookie(&self, name: &str) -> Result<Option<Cookie>, Box<dyn Error>> {
        Ok(self
            .driver
            .get_all_cookies()
            .await?
            .into_iter()
            .find(|cookie| cookie.name == name))
    }
}

/// Polls `check` every [`POLL`] until it holds, and fails naming `what` when
/// [`WAIT`] runs out first.
pub(crate) async fn until<F>(what: &str, mut check: F) -> Result<(), Box<dyn Error>>
where
    F: AsyncFnMut() -> Result<bool, Box<dyn Error>>,
{
    let mut polls = 0_u32;
    let mut interval = tokio::time::interval(POLL);
    loop {
        interval.tick().await;
        if check().await? {
            return Ok(());
        }
        polls = polls.saturating_add(1);
        if POLL.saturating_mul(polls) >= WAIT {
            return Err(format!("waited {WAIT:?} and it never held that {what}").into());
        }
    }
}

/// Runs `journey` in a fresh browser, closes the browser whatever the
/// journey did, and fails on a console error the browser logged last.
pub(crate) async fn with_browser<F>(journey: F) -> Result<(), Box<dyn Error>>
where
    F: AsyncFnOnce(&Browser) -> Result<(), Box<dyn Error>>,
{
    let browser = Browser::open().await?;
    let outcome = match journey(&browser).await {
        Ok(()) => browser.console_clean("the end of the journey").await,
        Err(error) => Err(error),
    };
    browser.quit().await?;
    outcome
}

/// Fake chromedrivers: shell scripts that answer `--port=0` the way
/// chromedriver does, to hold the start to its retry rule without a browser.
#[cfg(unix)]
mod fake {
    use std::error::Error;
    use std::os::unix::fs::PermissionsExt;
    use std::path::{Path, PathBuf};

    use super::Chromedriver;
    use super::STARTS;

    /// Writes the script `body` as an executable `chromedriver` in `dir`.
    fn script(dir: &Path, body: &str) -> Result<PathBuf, Box<dyn Error>> {
        let path = dir.join("chromedriver");
        std::fs::write(&path, format!("#!/bin/sh\n{body}"))?;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))?;
        Ok(path)
    }

    /// How many times a script in `dir` ran, by the lines it appended to
    /// `runs`.
    fn runs(dir: &Path) -> Result<usize, Box<dyn Error>> {
        Ok(std::fs::read_to_string(dir.join("runs"))?.lines().count())
    }

    /// A script that counts its runs, finds its port bound on the first
    /// `taken` of them, and listens on port 4444 after.
    fn taken_then_listening(dir: &Path, taken: u32) -> Result<PathBuf, Box<dyn Error>> {
        let runs = dir.join("runs");
        script(
            dir,
            &format!(
                "echo run >> '{runs}'\n\
                 if [ \"$(wc -l < '{runs}')\" -le {taken} ]; then\n\
                 echo '[1.0][SEVERE]: bind() failed: Address already in use (98)' >&2\n\
                 exit 1\n\
                 fi\n\
                 echo 'ChromeDriver was started successfully on port 4444.'\n\
                 exec sleep 30\n",
                runs = runs.display()
            ),
        )
    }

    #[tokio::test]
    async fn a_port_bound_already_starts_chromedriver_again() -> Result<(), Box<dyn Error>> {
        let dir = tempfile::tempdir()?;
        let binary = taken_then_listening(dir.path(), 2)?;
        let (chromedriver, endpoint) = Chromedriver::start_from(&binary).await?;
        drop(chromedriver);
        assert_eq!(endpoint, "http://127.0.0.1:4444");
        assert_eq!(runs(dir.path())?, 3);
        Ok(())
    }

    #[tokio::test]
    async fn a_port_bound_on_every_start_fails_after_the_last() -> Result<(), Box<dyn Error>> {
        let dir = tempfile::tempdir()?;
        let binary = taken_then_listening(dir.path(), STARTS)?;
        let error = Chromedriver::start_from(&binary)
            .await
            .expect_err("every start finds its port bound");
        assert!(
            error.to_string().contains("Address already in use"),
            "{error}"
        );
        assert_eq!(runs(dir.path())?, usize::try_from(STARTS)?);
        Ok(())
    }

    #[tokio::test]
    async fn another_early_exit_fails_at_once_with_its_output() -> Result<(), Box<dyn Error>> {
        let dir = tempfile::tempdir()?;
        let runs_file = dir.path().join("runs");
        let binary = script(
            dir.path(),
            &format!(
                "echo run >> '{}'\necho 'no Chrome binary' >&2\nexit 1\n",
                runs_file.display()
            ),
        )?;
        let error = Chromedriver::start_from(&binary)
            .await
            .expect_err("chromedriver exits before it listens");
        let message = error.to_string();
        assert!(
            message.contains("exited before it listened") && message.contains("no Chrome binary"),
            "{message}"
        );
        assert_eq!(runs(dir.path())?, 1);
        Ok(())
    }
}
