// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! What the tests share: settings built from a configuration text.

use std::collections::BTreeMap;
use std::path::Path;

use ferrobridge_viewer::config::Config;
use ferrobridge_viewer::config::settings::Settings;

/// The settings a console with its site bundle under `site_root` and its
/// bridge at `bridge` runs on.
pub(crate) fn settings(site_root: &Path, bridge: &str) -> Settings {
    let text = format!(
        "[server]\nlisten = \"127.0.0.1:0\"\nsite_root = {site_root:?}\n\n[bridge]\nbase_url = \"{bridge}\"\n",
        site_root = site_root.to_string_lossy(),
    );
    Config::from_sources(Some(&text), &BTreeMap::new())
        .expect("the configuration reads")
        .resolve()
        .expect("the configuration resolves")
}
