//! Third-party ROM indexes provide discovery metadata, never flash approval.
use anyhow::{bail, Context, Result};
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashSet;
use std::io::Read;
use std::time::Duration;

#[derive(Clone, Debug, Serialize)]
pub struct RomEntry {
    pub source: &'static str,
    pub device: String,
    pub region: String,
    pub version: String,
    pub filename: String,
    pub url: String,
    pub md5: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct SourceStatus {
    pub source: &'static str,
    pub available: bool,
    pub entries: usize,
}

#[derive(Debug, Serialize)]
pub struct CatalogReport {
    pub entries: Vec<RomEntry>,
    pub sources: Vec<SourceStatus>,
}

pub fn normalize_region(region: &str) -> Option<&'static str> {
    match region.to_ascii_lowercase().as_str() {
        "mi" | "global" => Some("MI"),
        "eu" | "eea" => Some("EU"),
        "in" | "india" => Some("IN"),
        "id" | "indonesia" => Some("ID"),
        "tw" | "taiwan" => Some("TW"),
        "tr" | "turkey" => Some("TR"),
        "ru" | "russia" => Some("RU"),
        "cn" | "china" => Some("CN"),
        "jp" | "japan" => Some("JP"),
        _ => None,
    }
}

fn version_region(version: &str) -> Option<&'static str> {
    let suffix = version.get(version.len().checked_sub(4)?..)?;
    suffix.strip_suffix("XM").and_then(normalize_region)
}

fn full_recovery(filename: &str) -> bool {
    filename.ends_with(".zip")
        && !filename.contains(['/', '\\', '?', '#', ':'])
        && (filename.contains("-ota_full-") || filename.starts_with("miui_"))
}

pub(crate) fn trusted_download(url: &str) -> bool {
    let Ok(url) = reqwest::Url::parse(url) else {
        return false;
    };
    url.scheme() == "https"
        && url.username().is_empty()
        && url.password().is_none()
        && matches!(
            url.host_str(),
            Some(
                "bigota.d.miui.com"
                    | "hugeota.d.miui.com"
                    | "cdnorg.d.miui.com"
                    | "bn.d.miui.com"
                    | "ultimateota.d.miui.com"
                    | "bkt-sgp-miui-ota-update-alisgp.oss-ap-southeast-1.aliyuncs.com"
            )
        )
}

fn fetch_text(client: &Client, url: &str) -> Result<String> {
    let response = client
        .get(url)
        .timeout(Duration::from_secs(12))
        .send()
        .map_err(reqwest::Error::without_url)?;
    if !response.status().is_success() {
        bail!("Catalog HTTP {}", response.status());
    }
    let mut text = String::new();
    response
        .take(4 * 1024 * 1024 + 1)
        .read_to_string(&mut text)?;
    if text.len() > 4 * 1024 * 1024 {
        bail!("Catalog exceeds size limit");
    }
    Ok(text)
}

#[derive(Deserialize)]
struct Rss {
    channel: Channel,
}
#[derive(Deserialize)]
struct Channel {
    #[serde(rename = "item", default)]
    items: Vec<Item>,
}
#[derive(Deserialize)]
struct Item {
    title: String,
    link: String,
    #[serde(default)]
    description: String,
}

fn parse_xfu(text: &str, device: &str) -> Result<Vec<RomEntry>> {
    let rss: Rss = quick_xml::de::from_str(text).context("Parsing XM Firmware Updater feed")?;
    let mut entries = Vec::new();
    for item in rss.channel.items {
        if !item.title.contains(" Recovery ")
            || !item.description.contains("Stable Recovery")
            || !trusted_download(&item.link)
        {
            continue;
        }
        let url = reqwest::Url::parse(&item.link)?;
        let filename = url
            .path_segments()
            .and_then(|mut p| p.next_back())
            .unwrap_or("");
        if !full_recovery(filename) {
            continue;
        }
        let version = item.title.split_whitespace().nth(1).unwrap_or("");
        let Some(region) = version_region(version) else {
            continue;
        };
        let md5 = item
            .description
            .split_once("MD5:")
            .and_then(|(_, tail)| {
                tail.split(|c: char| !c.is_ascii_hexdigit())
                    .find(|s| s.len() == 32)
            })
            .map(str::to_owned);
        entries.push(RomEntry {
            source: "XM Firmware Updater",
            device: device.into(),
            region: region.into(),
            version: version.into(),
            filename: filename.into(),
            url: item.link,
            md5,
        });
    }
    Ok(entries)
}

fn parse_miroms(text: &str, device: &str) -> Result<Vec<RomEntry>> {
    let root: Value = serde_json::from_str(text)?;
    if root.get("codename").and_then(Value::as_str) != Some(device) {
        bail!("Catalog device mismatch");
    }
    let branches = root
        .get("branches")
        .and_then(Value::as_array)
        .context("Catalog branches missing")?;
    let mut entries = Vec::new();
    for branch in branches {
        if branch.pointer("/tags/branchtag").and_then(Value::as_str) != Some("F") {
            continue;
        }
        let Some(region) = branch
            .get("region")
            .and_then(Value::as_str)
            .and_then(normalize_region)
        else {
            continue;
        };
        for rom in branch
            .get("roms")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let Some(filename) = rom
                .get("recovery")
                .and_then(Value::as_str)
                .filter(|s| full_recovery(s))
            else {
                continue;
            };
            let Some(version) = rom.get("miui").and_then(Value::as_str) else {
                continue;
            };
            if version_region(version) != Some(region) || !filename.contains(version) {
                continue;
            }
            entries.push(RomEntry {
                source: "miroms",
                device: device.into(),
                region: region.into(),
                version: version.into(),
                filename: filename.into(),
                url: format!("https://bigota.d.miui.com/{version}/{filename}"),
                md5: None,
            });
        }
    }
    Ok(entries)
}

pub fn fetch(client: &Client, device: &str, region: Option<&str>) -> Result<CatalogReport> {
    let device = device.split('_').next().unwrap_or("").to_ascii_lowercase();
    if device.is_empty()
        || device.len() > 64
        || !device
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'-')
    {
        bail!("Invalid catalog device codename");
    }
    let region = region
        .map(|r| normalize_region(r).context("Unknown catalog region"))
        .transpose()?;
    let xfu_url = format!("https://raw.githubusercontent.com/XiaomiFirmwareUpdater/miui-updates-tracker/master/rss/{device}.xml");
    let miroms_url = format!("https://api.miuier.com/api/v3/devices/{device}.json");
    let (xfu, miroms) = std::thread::scope(|scope| {
        let xfu =
            scope.spawn(|| fetch_text(client, &xfu_url).and_then(|text| parse_xfu(&text, &device)));
        let miroms = fetch_text(client, &miroms_url).and_then(|text| parse_miroms(&text, &device));
        (
            xfu.join()
                .unwrap_or_else(|_| Err(anyhow::anyhow!("Catalog worker failed"))),
            miroms,
        )
    });
    let mut report = CatalogReport {
        entries: Vec::new(),
        sources: Vec::new(),
    };
    for (source, result) in [("XM Firmware Updater", xfu), ("miroms", miroms)] {
        let available = result.is_ok();
        let entries: Vec<_> = result
            .unwrap_or_default()
            .into_iter()
            .filter(|e| region.is_none_or(|r| e.region == r))
            .collect();
        crate::diagnostics::event(
            "catalog.source",
            serde_json::json!({"source": source, "available": available, "entries": entries.len()}),
        );
        report.sources.push(SourceStatus {
            source,
            available,
            entries: entries.len(),
        });
        report.entries.extend(entries);
    }
    let mut seen = HashSet::new();
    report
        .entries
        .retain(|entry| seen.insert((entry.source, entry.url.clone())));
    Ok(report)
}

pub(crate) fn matching_urls(report: &CatalogReport, filename: &str, md5: &str) -> Vec<String> {
    report
        .entries
        .iter()
        .filter(|entry| {
            entry.filename == filename
                && entry
                    .md5
                    .as_deref()
                    .is_none_or(|hash| hash.eq_ignore_ascii_case(md5))
                && trusted_download(&entry.url)
        })
        .map(|entry| entry.url.clone())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const FILE: &str = "garnet_tw_global-ota_full-OS1.0.1.0.UNRTWXM-user-14.0-example.zip";
    const MD5: &str = "0123456789abcdef0123456789abcdef";

    #[test]
    fn xfu_reads_full_recovery_links_and_full_checksums() {
        let xml = format!(
            r#"<rss><channel>
          <item><title>MIUI OS1.0.1.0.UNRTWXM Recovery update</title><link>https://bigota.d.miui.com/OS1.0.1.0.UNRTWXM/{FILE}</link>
          <description>New Stable Recovery &lt;b&gt;MD5:&lt;/b&gt; {MD5}&lt;/p&gt;</description></item>
          <item><title>MIUI OS1.0.1.0.UNRTWXM Fastboot update</title><link>https://bigota.d.miui.com/fastboot.tgz</link><description>Stable Fastboot</description></item>
          <item><title>MIUI OS1.0.1.0.UNRTWXM Recovery update</title><link>https://example.invalid/{FILE}</link><description>Stable Recovery</description></item>
          <item><title>MIUI OS1.0.1.0.UNRTWXM Recovery update</title><link>https://bigota.d.miui.com/garnet-ota_incremental-update.zip</link><description>Stable Recovery</description></item>
        </channel></rss>"#
        );
        let entries = parse_xfu(&xml, "garnet").unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].region, "TW");
        assert_eq!(entries[0].md5.as_deref(), Some(MD5));
    }

    #[test]
    fn miroms_does_not_invent_checksums_or_offer_fastboot_and_weekly_packages() {
        let json = serde_json::json!({"codename":"garnet", "branches":[
            {"region":"tw", "tags":{"branchtag":"F"}, "roms":[{"miui":"OS1.0.1.0.UNRTWXM", "recovery":FILE, "fastboot":"package.tgz"}]},
            {"region":"tw", "tags":{"branchtag":"X"}, "roms":[{"miui":"OS1.0.1.0.UNRTWXM", "recovery":FILE}]},
            {"region":"cn", "tags":{"branchtag":"F"}, "roms":[{"miui":"OS1.0.1.0.UNRTWXM", "recovery":FILE}]}
        ]});
        let entries = parse_miroms(&json.to_string(), "garnet").unwrap();
        assert_eq!(entries.len(), 1);
        assert!(entries[0].md5.is_none());
        assert!(parse_miroms(&json.to_string(), "ruby").is_err());
    }

    #[test]
    fn fallback_requires_the_exact_approved_filename_and_consistent_checksum() {
        let entry = RomEntry {
            source: "miroms",
            device: "garnet".into(),
            region: "TW".into(),
            version: "OS1.0.1.0.UNRTWXM".into(),
            filename: FILE.into(),
            url: format!("https://bigota.d.miui.com/{FILE}"),
            md5: None,
        };
        let mut report = CatalogReport {
            entries: vec![entry],
            sources: Vec::new(),
        };
        assert_eq!(matching_urls(&report, FILE, MD5).len(), 1);
        assert!(matching_urls(&report, "another-region.zip", MD5).is_empty());
        report.entries[0].md5 = Some("ffffffffffffffffffffffffffffffff".into());
        assert!(matching_urls(&report, FILE, MD5).is_empty());
        report.entries[0].md5 = Some(MD5.into());
        report.entries[0].url = "https://bigota.d.miui.com.evil.invalid/package.zip".into();
        assert!(matching_urls(&report, FILE, MD5).is_empty());
    }

    #[test]
    fn invalid_catalog_input_fails_before_any_network_access() {
        let client = Client::new();
        assert!(fetch(&client, "../private", None).is_err());
        assert!(fetch(&client, "garnet", Some("unknown-region")).is_err());
    }
}
