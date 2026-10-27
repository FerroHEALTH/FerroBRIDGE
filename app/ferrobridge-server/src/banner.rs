// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! What FerroBRIDGE puts into the family's boot banner: the wordmark, the
//! product line, the pins it serves and the lanes its configuration switches
//! on.
//!
//! `ferrohealth_server::build_info::banner` draws the wordmark, the build
//! line and the facts under them. It prints to stdout before the log
//! subscriber exists, and only when the console renders `pretty`, so a log
//! pipeline reading `json` never sees it. No specification governs this: our
//! own design.

use ferrobridge_operator_api::health::Pin;
use ferrohealth_server::build_info::{self, BuildInfo};
use ferrohealth_telemetry::section::Rendering;

use crate::startup::Lane;

/// The wordmark in the `FIGlet` `standard` face, kept as text so the banner
/// needs no font file and no dependency.
const WORDMARK: &str = r"
 _____                   ____  ____  ___ ____   ____ _____
|  ___|__ _ __ _ __ ___ | __ )|  _ \|_ _|  _ \ / ___| ____|
| |_ / _ \ '__| '__/ _ \|  _ \| |_) || || | | | |  _|  _|
|  _|  __/ |  | | | (_) | |_) |  _ < | || |_| | |_| | |___
|_|  \___|_|  |_|  \___/|____/|_| \_\___|____/ \____|_____|
";

/// The product line printed under the build line.
pub const PRODUCT_LINE: &str = "openEHR bridge to HL7 FHIR and the OMOP CDM";

/// The maintainer named in the banner.
pub const MAINTAINER: &str = "Ruben Talstra";

/// The project site named in the banner.
pub const SITE: &str = "https://ferrobridge.eu";

/// The widest line the banner writes, in characters.
pub const MAX_WIDTH: usize = 100;

/// Renders the banner of `info` for the served `pins` and the `lanes`, or
/// `None` unless the console renders `pretty`.
#[must_use]
pub fn render(
    info: &BuildInfo,
    pins: &[Pin],
    lanes: &[Lane],
    rendering: Rendering,
) -> Option<String> {
    let mut facts: Vec<(&str, String)> = vec![
        ("product", PRODUCT_LINE.to_owned()),
        ("maintainer", MAINTAINER.to_owned()),
        ("site", SITE.to_owned()),
    ];
    facts.extend(
        pins.iter()
            .map(|pin| (pin.name.as_str(), pin.version.clone())),
    );
    facts.extend(lanes.iter().map(|lane| (lane.name, lane_value(lane))));
    build_info::banner(info, WORDMARK.trim_start_matches('\n'), &facts, rendering)
}

/// Returns what the banner says of `lane`: on or off, then the address it
/// listens on and the hosts it reaches.
fn lane_value(lane: &Lane) -> String {
    let state = if lane.enabled { "on " } else { "off" };
    let listens = lane
        .listen
        .as_ref()
        .map(|listen| format!("listens on {listen}"));
    let targets: Vec<&str> = listens.as_deref().into_iter().chain(lane.hosts()).collect();
    format!("{state}  {}", targets.join(", "))
        .trim_end()
        .to_owned()
}

/// Prints the banner of this build to stdout when the console renders
/// `pretty`.
#[expect(
    clippy::print_stdout,
    reason = "the banner is console output and prints before any log subscriber exists"
)]
pub fn print(lanes: &[Lane], rendering: Rendering) {
    if let Some(banner) = render(
        &crate::build_info::BUILD,
        &crate::build_info::pins(),
        lanes,
        rendering,
    ) {
        print!("{banner}");
    }
}

#[cfg(test)]
mod tests {
    use super::{MAINTAINER, MAX_WIDTH, PRODUCT_LINE, SITE, render};
    use crate::build_info::pins;
    use crate::startup::Lane;
    use ferrohealth_server::build_info::BuildInfo;
    use ferrohealth_telemetry::section::Rendering;

    const INFO: BuildInfo = BuildInfo::new("ferrobridge", "1.2.3").with_commit("1a2b3c4d5e6f");

    fn lanes() -> Vec<Lane> {
        vec![
            Lane {
                name: "facade",
                enabled: true,
                cdr_host: Some(String::from("cdr.invalid:8443")),
                ..Lane::default()
            },
            Lane {
                name: "etl",
                enabled: true,
                cdr_host: Some(String::from("cdr.invalid:8443")),
                cdm_host: Some(String::from("db.invalid:5432")),
                ..Lane::default()
            },
            Lane {
                name: "terminology",
                ..Lane::default()
            },
        ]
    }

    fn pretty(lanes: &[Lane]) -> String {
        render(&INFO, &pins(), lanes, Rendering::Pretty).expect("pretty prints the banner")
    }

    #[test]
    fn the_banner_names_the_version_the_maintainer_and_the_site() {
        let banner = pretty(&lanes());
        assert!(banner.contains("ferrobridge 1.2.3 (1a2b3c4)"), "{banner}");
        assert!(banner.contains(PRODUCT_LINE), "{banner}");
        assert!(banner.contains(MAINTAINER), "{banner}");
        assert!(banner.contains(SITE), "{banner}");
    }

    #[test]
    fn a_json_console_prints_no_banner() {
        assert_eq!(None, render(&INFO, &pins(), &lanes(), Rendering::Json));
    }

    #[test]
    fn every_line_fits_a_hundred_columns() {
        for line in pretty(&lanes()).lines() {
            assert!(line.chars().count() <= MAX_WIDTH, "{line:?}");
        }
    }

    #[test]
    fn every_pin_and_every_lane_has_its_line() {
        let banner = pretty(&lanes());
        for pin in pins() {
            assert!(
                banner
                    .lines()
                    .any(|line| line.contains(pin.name.as_str()) && line.ends_with(&pin.version)),
                "{pin:?}\n{banner}"
            );
        }
        let line = |name: &str| {
            banner
                .lines()
                .find(|line| line.trim_start().starts_with(name))
                .map(str::to_owned)
        };
        assert!(
            line("facade").is_some_and(|line| line.ends_with("on   cdr.invalid:8443")),
            "{banner}"
        );
        assert!(
            line("etl")
                .is_some_and(|line| line.ends_with("on   cdr.invalid:8443, db.invalid:5432")),
            "{banner}"
        );
        assert!(
            line("terminology").is_some_and(|line| line.ends_with("off")),
            "{banner}"
        );
    }

    #[test]
    fn a_face_with_a_listener_names_its_address_before_its_hosts() {
        let lanes = [Lane {
            name: "hl7v2",
            enabled: true,
            listen: Some(String::from("0.0.0.0:2575")),
            cdr_host: Some(String::from("cdr.invalid:8443")),
            ..Lane::default()
        }];
        let banner = pretty(&lanes);
        assert!(
            banner.contains("on   listens on 0.0.0.0:2575, cdr.invalid:8443\n"),
            "{banner}"
        );
    }
}
