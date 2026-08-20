// Copyright (C) 2026 HasX
// Licensed under the GNU AGPL v3.0. See LICENSE file for details.
// Website: https://hasx.dev

use anyhow::{anyhow, bail, Context, Result};
use indicatif::{ProgressBar, ProgressStyle};
use md5::Digest;
use reqwest::blocking::Client;
use std::collections::HashSet;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf}; // brings Md5::new() into scope

/// Xiaomi's ROM mirrors reject generic clients for some Recovery packages.
pub const XIAOMI_DOWNLOAD_USER_AGENT: &str = "MiTunes_UserAgent_v3.0";

const XIAOMI_CDN_FALLBACK_HOSTS: &[&str] = &[
    "bn.d.miui.com",
    "bkt-sgp-miui-ota-update-alisgp.oss-ap-southeast-1.aliyuncs.com",
    "hugeota.d.miui.com",
];

const XIAOMI_PRIMARY_CDN_HOSTS: &[&str] = &["ultimateota.d.miui.com", "superota.d.miui.com"];

fn version_directory_from_filename(filename: &str) -> Option<&str> {
    filename
        .split_once("-ota_full-")?
        .1
        .split_once("-user-")
        .map(|(version, _)| version)
        .filter(|version| !version.is_empty())
}

pub fn official_download_client() -> Result<Client> {
    Client::builder()
        .user_agent(XIAOMI_DOWNLOAD_USER_AGENT)
        .connect_timeout(std::time::Duration::from_secs(20))
        .timeout(std::time::Duration::from_secs(60))
        .build()
        .context("Creating Xiaomi ROM download client")
}

#[derive(Clone, Debug)]
pub struct LatestInfo {
    pub filename: String, // may contain ?t=...&s=...
    pub md5: String,
}

pub fn parse_latest_from_json(json: &str) -> Result<(LatestInfo, Vec<String>)> {
    let v: serde_json::Value = serde_json::from_str(json)?;
    let latest = v
        .get("LatestRom")
        .or_else(|| v.get("PkgRom"))
        .ok_or_else(|| anyhow!("LatestRom/PkgRom missing in JSON"))?;
    let filename = latest
        .get("filename")
        .and_then(|x| x.as_str())
        .ok_or_else(|| anyhow!("filename missing in LatestRom/PkgRom"))?
        .to_string();
    let md5 = latest
        .get("md5")
        .and_then(|x| x.as_str())
        .ok_or_else(|| anyhow!("md5 missing in LatestRom/PkgRom"))?
        .to_string();
    let mirrors = v
        .get("MirrorList")
        .and_then(|x| x.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|s| s.as_str().map(|t| t.to_string()))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    Ok((LatestInfo { filename, md5 }, mirrors))
}

pub fn candidate_urls(mirrors: &[String], filename: &str) -> Vec<String> {
    let mut candidates = Vec::new();
    let mut seen = HashSet::new();
    for require_https in [true, false] {
        for base in mirrors {
            if require_https != base.starts_with("https://") {
                continue;
            }
            let url = format!(
                "{}/{}",
                base.trim_end_matches('/'),
                filename.trim_start_matches('/')
            );
            if seen.insert(url.clone()) {
                candidates.push(url);
            }
        }
    }

    // Xiaomi's API commonly returns ultimateota and superota. Both can reject
    // a valid client by CDN route, while these first-party mirrors serve the
    // identical URL path. Only expand known Xiaomi hosts, never arbitrary URLs
    // returned by a server or supplied by a user.
    let fallback_path = {
        let filename = filename.trim_start_matches('/');
        version_directory_from_filename(filename).map(|version| {
            if filename.starts_with(&format!("{version}/")) {
                filename.to_owned()
            } else {
                format!("{version}/{filename}")
            }
        })
    };
    let primary_candidates = candidates.clone();
    for url in primary_candidates {
        let Ok(parsed) = reqwest::Url::parse(&url) else {
            continue;
        };
        let Some(host) = parsed.host_str() else {
            continue;
        };
        if !XIAOMI_PRIMARY_CDN_HOSTS.contains(&host) {
            continue;
        }
        let Some(path) = &fallback_path else {
            continue;
        };
        for fallback_host in XIAOMI_CDN_FALLBACK_HOSTS {
            let fallback = format!("https://{fallback_host}/{path}");
            if seen.insert(fallback.clone()) {
                candidates.push(fallback);
            }
        }
    }
    candidates
}

pub fn choose_url(mirrors: &[String], filename: &str) -> Option<String> {
    candidate_urls(mirrors, filename).into_iter().next()
}

pub fn download_with_md5(
    client: &Client,
    url: &str,
    dest_dir: &Path,
    expect_md5: &str,
) -> Result<PathBuf> {
    download_with_md5_inner(client, url, dest_dir, expect_md5, true, |_, _| {})
}

/// Downloads from Xiaomi's HTTPS mirrors in the supplied order. A failed mirror
/// never leaves a partial ROM behind and does not prevent the next HTTPS mirror
/// from being tried.
pub fn download_from_https_mirrors_with_md5<F>(
    client: &Client,
    mirrors: &[String],
    latest: &LatestInfo,
    dest_dir: &Path,
    mut on_progress: F,
) -> Result<PathBuf>
where
    F: FnMut(u64, Option<u64>),
{
    let candidates = candidate_urls(mirrors, &latest.filename)
        .into_iter()
        .filter(|url| url.starts_with("https://"))
        .collect::<Vec<_>>();
    if candidates.is_empty() {
        bail!("Xiaomi did not provide an HTTPS ROM mirror");
    }

    let mut failures = Vec::new();
    for url in candidates {
        match download_with_md5_inner(client, &url, dest_dir, &latest.md5, false, &mut on_progress)
        {
            Ok(path) => return Ok(path),
            Err(error) => failures.push(format!("{url}: {error:#}")),
        }
    }
    bail!(
        "All Xiaomi HTTPS ROM mirrors failed. {}",
        failures.join(" | ")
    )
}

pub fn download_with_md5_with_progress<F>(
    client: &Client,
    url: &str,
    dest_dir: &Path,
    expect_md5: &str,
    on_progress: F,
) -> Result<PathBuf>
where
    F: FnMut(u64, Option<u64>),
{
    download_with_md5_inner(client, url, dest_dir, expect_md5, false, on_progress)
}

fn download_with_md5_inner<F>(
    client: &Client,
    url: &str,
    dest_dir: &Path,
    expect_md5: &str,
    show_terminal_progress: bool,
    mut on_progress: F,
) -> Result<PathBuf>
where
    F: FnMut(u64, Option<u64>),
{
    fs::create_dir_all(dest_dir)
        .with_context(|| format!("Creating download directory {}", dest_dir.display()))?;
    // derive file name (strip query)
    let base = url.split('/').next_back().unwrap_or("download.zip");
    let base = base.split('?').next().unwrap_or(base);
    let dest = dest_dir.join(base);
    if dest.exists() {
        bail!("Refusing to overwrite existing ROM file {}", dest.display());
    }
    let temporary = dest.with_file_name(format!("{base}.part"));

    let result = (|| -> Result<PathBuf> {
        let resp = client
            .get(url)
            .send()
            .with_context(|| format!("GET {}", url))?;
        if !resp.status().is_success() {
            bail!("Download failed: HTTP {} from {}", resp.status(), url);
        }
        let len = resp.content_length();
        let pb = if show_terminal_progress {
            let progress = ProgressBar::new(len.unwrap_or(0));
            progress.set_style(
                ProgressStyle::default_bar()
                    .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({percent}%)")
                    .expect("static progress-bar template is valid")
                    .progress_chars("=>-"),
            );
            progress
        } else {
            ProgressBar::hidden()
        };

        let mut hasher = md5::Md5::new();
        let mut file = File::create(&temporary)
            .with_context(|| format!("Creating temporary ROM file {}", temporary.display()))?;
        let mut src = resp;
        let mut buf = [0u8; 128 * 1024];
        let mut downloaded = 0;
        on_progress(downloaded, len);
        loop {
            let n = src.read(&mut buf)?;
            if n == 0 {
                break;
            }
            file.write_all(&buf[..n])?;
            hasher.update(&buf[..n]);
            downloaded += n as u64;
            if let Some(total) = len {
                pb.set_position(std::cmp::min(pb.position() + (n as u64), total));
            } else {
                pb.inc(n as u64);
            }
            on_progress(downloaded, len);
        }
        pb.finish_and_clear();
        let got = format!("{:x}", hasher.finalize());
        if got.to_lowercase() != expect_md5.to_lowercase() {
            bail!(
                "MD5 mismatch after download: got {}, expected {}",
                got,
                expect_md5
            );
        }
        drop(file);
        fs::rename(&temporary, &dest).with_context(|| {
            format!(
                "Finalizing verified ROM from {} to {}",
                temporary.display(),
                dest.display()
            )
        })?;
        Ok(dest)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;
    use std::thread;

    #[test]
    fn parses_latest_rom_and_prefers_https_mirror() {
        let json = r#"{
            "LatestRom": {"filename": "rom.zip?t=1", "md5": "abc123"},
            "MirrorList": ["http://insecure.example", "https://official.example/roms"]
        }"#;

        let (latest, mirrors) = parse_latest_from_json(json).expect("response should parse");
        assert_eq!(latest.filename, "rom.zip?t=1");
        assert_eq!(latest.md5, "abc123");
        assert_eq!(
            choose_url(&mirrors, &latest.filename),
            Some("https://official.example/roms/rom.zip?t=1".to_owned())
        );
        assert_eq!(
            candidate_urls(&mirrors, &latest.filename),
            vec![
                "https://official.example/roms/rom.zip?t=1".to_owned(),
                "http://insecure.example/rom.zip?t=1".to_owned(),
            ]
        );
    }

    #[test]
    fn expands_known_xiaomi_cdn_route_with_first_party_fallbacks() {
        let mirrors = vec!["https://ultimateota.d.miui.com/OS3.0.302.0.WNREUXM".to_owned()];
        let urls = candidate_urls(
            &mirrors,
            "garnet_eea_global-ota_full-OS3.0.302.0.WNREUXM-user-16.0.zip",
        );
        assert_eq!(urls.len(), 4);
        assert_eq!(
            urls[1],
            "https://bn.d.miui.com/OS3.0.302.0.WNREUXM/garnet_eea_global-ota_full-OS3.0.302.0.WNREUXM-user-16.0.zip"
        );
        assert_eq!(
            urls[3],
            "https://hugeota.d.miui.com/OS3.0.302.0.WNREUXM/garnet_eea_global-ota_full-OS3.0.302.0.WNREUXM-user-16.0.zip"
        );
    }

    #[test]
    fn downloads_and_verifies_md5_from_http_server() {
        let body = b"Sensitivity download test payload";
        let expected_md5 = format!("{:x}", md5::Md5::digest(body));
        let listener = TcpListener::bind("127.0.0.1:0").expect("test listener should bind");
        let address = listener
            .local_addr()
            .expect("listener should expose its address");
        let response_body = body.to_vec();
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("test client should connect");
            let mut request = [0u8; 1024];
            let _ = stream.read(&mut request);
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                response_body.len()
            )
            .expect("test response headers should write");
            stream
                .write_all(&response_body)
                .expect("test response body should write");
        });

        let temp = tempfile::tempdir().expect("temp directory should exist");
        let client = Client::builder().build().expect("HTTP client should build");
        let path = download_with_md5(
            &client,
            &format!("http://{address}/recovery.zip"),
            temp.path(),
            &expected_md5,
        )
        .expect("download should match its MD5");

        assert_eq!(fs::read(path).expect("download should be readable"), body);
        server.join().expect("test server should finish");
    }

    #[test]
    fn removes_partial_file_when_checksum_verification_fails() {
        let body = b"corrupt test payload";
        let listener = TcpListener::bind("127.0.0.1:0").expect("test listener should bind");
        let address = listener
            .local_addr()
            .expect("listener should expose its address");
        let response_body = body.to_vec();
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("test client should connect");
            let mut request = [0u8; 1024];
            let _ = stream.read(&mut request);
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                response_body.len()
            )
            .expect("test response headers should write");
            stream
                .write_all(&response_body)
                .expect("test response body should write");
        });

        let temp = tempfile::tempdir().expect("temp directory should exist");
        let client = Client::builder().build().expect("HTTP client should build");
        let result = download_with_md5(
            &client,
            &format!("http://{address}/recovery.zip"),
            temp.path(),
            "00000000000000000000000000000000",
        );

        assert!(result.is_err(), "incorrect MD5 must fail the download");
        assert!(!temp.path().join("recovery.zip").exists());
        assert!(!temp.path().join("recovery.zip.part").exists());
        server.join().expect("test server should finish");
    }
}
