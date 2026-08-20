// Copyright (C) 2026 HasX
// Licensed under the GNU AGPL v3.0. See LICENSE file for details.

use std::io::Read;

use anyhow::{Context, Result};
use sensitivity::download;
use sensitivity::mi::DeviceInfo;
use sensitivity::validate;

const XIAOMI_ENDPOINT: &str = "https://update.miui.com/updates/miotaV3.php";

fn live_device_info() -> Result<DeviceInfo> {
    let required = |name| {
        std::env::var(name).with_context(|| {
            format!(
                "{name} is required for the live test; read it from your own Recovery device and keep it out of logs"
            )
        })
    };
    Ok(DeviceInfo {
        device: required("SENSITIVITY_LIVE_DEVICE")?,
        sn: required("SENSITIVITY_LIVE_SN")?,
        version: required("SENSITIVITY_LIVE_VERSION")?,
        codebase: required("SENSITIVITY_LIVE_CODEBASE")?,
        branch: required("SENSITIVITY_LIVE_BRANCH")?,
        language: std::env::var("SENSITIVITY_LIVE_LANGUAGE").unwrap_or_else(|_| "en-US".to_owned()),
        region: required("SENSITIVITY_LIVE_REGION")?,
        romzone: required("SENSITIVITY_LIVE_ROMZONE")?,
    })
}

#[test]
#[ignore = "requires Xiaomi's live service; run with cargo test --test live_xiaomi -- --ignored"]
fn resolves_an_official_rom_url_and_reads_a_small_https_range() -> Result<()> {
    let info = live_device_info()?;
    let request = validate::build_request_json(&info, None)?;
    let response = validate::validate(XIAOMI_ENDPOINT, &request)
        .context("requesting the latest Recovery ROM metadata from Xiaomi")?;
    let json = response
        .full_json
        .context("Xiaomi did not provide downloadable ROM metadata")?;
    let root: serde_json::Value = serde_json::from_str(&json)?;
    let top_level_keys = root
        .as_object()
        .map(|object| object.keys().cloned().collect::<Vec<_>>())
        .unwrap_or_default();
    let (latest, mirrors) = download::parse_latest_from_json(&json)
        .with_context(|| format!("LatestRom/PkgRom missing; top-level keys: {top_level_keys:?}"))?;
    let candidates = download::candidate_urls(&mirrors, &latest.filename)
        .into_iter()
        .filter(|url| url.starts_with("https://"))
        .collect::<Vec<_>>();
    anyhow::ensure!(
        !candidates.is_empty(),
        "Xiaomi did not provide an HTTPS Recovery ROM URL"
    );
    anyhow::ensure!(
        latest.md5.len() == 32 && latest.md5.chars().all(|ch| ch.is_ascii_hexdigit()),
        "Xiaomi returned an invalid MD5: {}",
        latest.md5
    );

    let client = download::official_download_client()?;
    // Match the production downloader. Some Xiaomi mirrors reject byte-range
    // probes even though a normal Recovery-ROM GET is permitted. Read just one
    // small buffer and drop the response instead of downloading the full file.
    let mut failures = Vec::new();
    let mut selected = None;
    for url in candidates {
        let parsed_url = reqwest::Url::parse(&url).context("parsing Xiaomi ROM URL")?;
        let host = parsed_url.host_str().unwrap_or("unknown");
        let response = client
            .get(&url)
            .send()
            .with_context(|| format!("opening the official HTTPS ROM URL {url}"))?;
        if response.status().is_success() {
            selected = Some(response);
            break;
        }
        let status = response.status();
        let body = response.text().unwrap_or_default();
        let detail = body
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .chars()
            .take(180)
            .collect::<String>();
        failures.push(format!("{host}: HTTP {status} ({detail})"));
    }
    let mut response = selected.ok_or_else(|| {
        anyhow::anyhow!(
            "all Xiaomi HTTPS mirrors rejected the ROM probe: {}",
            failures.join(", ")
        )
    })?;
    let mut sample = [0u8; 4096];
    let read = response.read(&mut sample)?;
    anyhow::ensure!(read > 0, "ROM URL returned an empty body");
    Ok(())
}
