// Copyright (C) 2026 Chromatic
// Licensed under the GNU AGPL v3.0. See LICENSE file for details.
// Website: https://chromatic.hu

use anyhow::{anyhow, bail, Context, Result};
use indicatif::{ProgressBar, ProgressStyle};
use md5::Digest;
use reqwest::blocking::Client;
use std::collections::HashSet;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

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

pub fn choose_url(mirrors: &[String], filename: &str) -> Option<String> {
    for base in mirrors {
        // Prefer https mirrors
        if !base.starts_with("https://") {
            continue;
        }
        let url = format!(
            "{}/{}",
            base.trim_end_matches('/'),
            filename.trim_start_matches('/')
        );
        return Some(url);
    }
    // fallback to any mirror
    mirrors
        .first()
        .map(|b| format!("{}/{}", b.trim_end_matches('/'), filename))
}

const PUBLIC_CDNS: &[&str] = &[
    "bigota.d.miui.com",
    "hugeota.d.miui.com",
    "cdnorg.d.miui.com",
    "bn.d.miui.com",
    "ultimateota.d.miui.com",
];

struct MirrorCandidate {
    url: reqwest::Url,
    public_search: bool,
}

fn mirror_candidates(latest: &LatestInfo, mirrors: &[String]) -> Vec<MirrorCandidate> {
    let mut candidates = Vec::new();
    let mut seen = HashSet::new();
    let mut add = |url: reqwest::Url, public_search| {
        if matches!(url.scheme(), "http" | "https")
            && url.username().is_empty()
            && url.password().is_none()
            && seen.insert(url.to_string())
        {
            candidates.push(MirrorCandidate { url, public_search });
        }
    };
    let absolute = reqwest::Url::parse(&latest.filename).ok();
    if let Some(url) = absolute.as_ref() {
        add(url.clone(), false);
    }
    for base in mirrors.iter().take(64) {
        let url = if let Some(source) = absolute.as_ref() {
            reqwest::Url::parse(base).ok().map(|mut url| {
                url.set_path(source.path());
                url.set_query(source.query());
                url
            })
        } else {
            choose_url(std::slice::from_ref(base), &latest.filename)
                .and_then(|url| reqwest::Url::parse(&url).ok())
        };
        if let Some(url) = url {
            add(url, false);
        }
    }
    candidates.sort_by_key(|candidate| candidate.url.scheme() != "https");
    // Search only public Xiaomi CDNs for the exact same artifact. Signing
    // parameters are used only on the server-advertised URLs, never copied to
    // a different host. Every successful download still needs the server MD5.
    let mut paths: Vec<String> = candidates
        .iter()
        .filter(|candidate| {
            candidate
                .url
                .host_str()
                .is_some_and(|host| PUBLIC_CDNS.contains(&host))
        })
        .map(|candidate| candidate.url.path().to_owned())
        .collect();
    let filename = latest
        .filename
        .split(['?', '#'])
        .next()
        .unwrap_or("")
        .rsplit('/')
        .next()
        .unwrap_or("");
    let version = filename.split(['_', '-']).find(|part| {
        (part.starts_with("OS") || part.starts_with('V'))
            && part.contains('.')
            && part
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'.')
    });
    if let Some(version) = version {
        paths.push(format!("/{version}/{filename}"));
    }
    paths.sort();
    paths.dedup();
    for path in paths.iter().take(4) {
        for host in PUBLIC_CDNS {
            if let Ok(url) = reqwest::Url::parse(&format!("https://{host}{path}")) {
                if seen.insert(url.to_string()) {
                    candidates.push(MirrorCandidate {
                        url,
                        public_search: true,
                    });
                }
            }
        }
    }
    candidates
}

fn probe_mirror(client: &Client, url: &reqwest::Url) -> Result<()> {
    let mut response = client
        .get(url.clone())
        .header(reqwest::header::RANGE, "bytes=0-3")
        .timeout(std::time::Duration::from_secs(10))
        .send()
        .map_err(reqwest::Error::without_url)
        .context("Checking ROM mirror")?;
    let status = response.status();
    crate::diagnostics::event(
        "download.mirror_probe",
        serde_json::json!({"status": status.as_u16()}),
    );
    if !matches!(status.as_u16(), 200 | 206) {
        bail!("ROM mirror check failed: HTTP {status}");
    }
    let mut signature = [0; 4];
    response
        .read_exact(&mut signature)
        .context("Reading ROM mirror ZIP signature")?;
    if signature != *b"PK\x03\x04" {
        bail!("ROM mirror did not return a ZIP package");
    }
    Ok(())
}

/// Prefer advertised URLs, then probe public CDN copies of the same artifact.
pub fn download_latest(client: &Client, json: &str, dest_dir: &Path) -> Result<PathBuf> {
    let (latest, mirrors) = parse_latest_from_json(json)?;
    let candidates = mirror_candidates(&latest, &mirrors);
    let mut last_error = None;
    for (index, candidate) in candidates.iter().enumerate() {
        crate::diagnostics::event(
            "download.mirror",
            serde_json::json!({"attempt": index + 1, "public_search": candidate.public_search}),
        );
        if candidate.public_search {
            if let Err(error) = probe_mirror(client, &candidate.url) {
                last_error = Some(error);
                continue;
            }
        }
        match download_with_md5(client, candidate.url.as_str(), dest_dir, &latest.md5) {
            Ok(path) => return Ok(path),
            Err(error) => last_error = Some(error),
        }
    }
    Err(last_error.unwrap_or_else(|| anyhow!("No ROM mirror returned by the server"))).context(
        "No working ROM mirror found; try again later or select an official Recovery ROM manually",
    )
}

/// Catalogs can supply another URL for an approved artifact. They cannot pick
/// another version, region, package type, or checksum for this operation.
pub fn download_latest_with_catalog(
    client: &Client,
    json: &str,
    dest_dir: &Path,
    device: &str,
) -> Result<PathBuf> {
    match download_latest(client, json, dest_dir) {
        Ok(path) => Ok(path),
        Err(primary_error) => {
            let (latest, _) = parse_latest_from_json(json)?;
            let filename = latest
                .filename
                .split(['?', '#'])
                .next()
                .unwrap_or("")
                .rsplit('/')
                .next()
                .unwrap_or("");
            let report = match crate::catalog::fetch(client, device, None) {
                Ok(report) => report,
                Err(_) => return Err(primary_error),
            };
            let mut seen = HashSet::new();
            for url in crate::catalog::matching_urls(&report, filename, &latest.md5) {
                if !seen.insert(url.clone()) {
                    continue;
                }
                let Ok(parsed) = reqwest::Url::parse(&url) else {
                    continue;
                };
                if probe_mirror(client, &parsed).is_err() {
                    continue;
                }
                if let Ok(path) = download_with_md5(client, &url, dest_dir, &latest.md5) {
                    return Ok(path);
                }
            }
            Err(primary_error).context(
                "Firmware catalogs did not provide a working copy of the same approved package",
            )
        }
    }
}

pub fn download_with_md5(
    client: &Client,
    url: &str,
    dest_dir: &Path,
    expect_md5: &str,
) -> Result<PathBuf> {
    if expect_md5.len() != 32
        || !expect_md5
            .chars()
            .all(|character| character.is_ascii_hexdigit())
    {
        bail!("Expected MD5 must contain exactly 32 hexadecimal characters");
    }
    fs::create_dir_all(dest_dir)
        .with_context(|| format!("create download directory {}", dest_dir.display()))?;
    let parsed = reqwest::Url::parse(url).context("Invalid ROM download URL")?;
    let base = parsed
        .path_segments()
        .and_then(|mut segments| segments.next_back())
        .unwrap_or("");
    let base = if base.is_empty() {
        "download.zip"
    } else {
        base
    };
    if matches!(base, "." | "..") || base.contains(['\\', ':', '*', '?', '"', '<', '>', '|']) {
        bail!("Invalid ROM download filename");
    }
    let dest = dest_dir.join(base);

    let resp = client
        .get(url)
        .send()
        .map_err(reqwest::Error::without_url)
        .context("Requesting ROM download")?;
    crate::diagnostics::event(
        "download.http",
        serde_json::json!({"status": resp.status().as_u16(), "bytes": resp.content_length()}),
    );
    if !resp.status().is_success() {
        bail!("Download failed: HTTP {}", resp.status());
    }
    let len = resp.content_length();
    let pb = ProgressBar::new(len.unwrap_or(0));
    pb.set_style(ProgressStyle::default_bar()
        .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({percent}%)")
        .unwrap()
        .progress_chars("=>-"));

    let mut staged = tempfile::Builder::new()
        .prefix(".sensitivity-")
        .suffix(".part")
        .tempfile_in(dest_dir)
        .context("create temporary ROM download")?;
    let transfer = (|| -> Result<String> {
        let mut hasher = md5::Md5::new();
        let file = staged.as_file_mut();
        let mut src = resp;
        let mut buf = [0u8; 128 * 1024];
        loop {
            let n = src.read(&mut buf)?;
            if n == 0 {
                break;
            }
            file.write_all(&buf[..n])?;
            hasher.update(&buf[..n]);
            if let Some(total) = len {
                pb.set_position(std::cmp::min(pb.position() + (n as u64), total));
            } else {
                pb.inc(n as u64);
            }
        }
        file.flush()?;
        Ok(hex::encode(hasher.finalize()))
    })();
    pb.finish_and_clear();
    let got = match transfer {
        Ok(md5) => md5,
        Err(error) => return Err(error).context("Downloading ROM package"),
    };
    if got.to_lowercase() != expect_md5.to_lowercase() {
        crate::diagnostics::event("download.checksum", serde_json::json!({"verified": false}));
        bail!(
            "MD5 mismatch after download: got {}, expected {}",
            got,
            expect_md5
        );
    }
    crate::diagnostics::event("download.checksum", serde_json::json!({"verified": true}));
    staged
        .persist(&dest)
        .map_err(|error| error.error)
        .with_context(|| format!("finalize verified download {}", dest.display()))?;
    Ok(dest)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;
    use std::thread;

    #[test]
    #[ignore = "Live network check; set SENSITIVITY_MIRROR_TEST_PATH to a public ROM path"]
    fn live_public_cdn_health() {
        let path = std::env::var("SENSITIVITY_MIRROR_TEST_PATH")
            .expect("Provide a public versioned ROM path");
        assert!(path.starts_with('/') && !path.starts_with("//") && !path.contains(['?', '#']));
        let client = Client::builder()
            .user_agent("MiTunes_UserAgent_v3.0")
            .build()
            .unwrap();
        let mut working = 0;
        for host in PUBLIC_CDNS {
            let url = reqwest::Url::parse(&format!("https://{host}{path}")).unwrap();
            match probe_mirror(&client, &url) {
                Ok(()) => {
                    working += 1;
                    eprintln!("{host}: ZIP range available");
                }
                Err(error) => eprintln!("{host}: {error:#}"),
            }
        }
        assert!(
            working >= 2,
            "Expected multiple functional public mirrors, found {working}"
        );
    }

    fn serve_once(body: &'static [u8]) -> String {
        serve_status(body, "200 OK")
    }

    fn serve_status(body: &'static [u8], status: &'static str) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = [0u8; 1024];
            let _ = stream.read(&mut request);
            write!(
                stream,
                "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            )
            .unwrap();
            stream.write_all(body).unwrap();
        });
        format!("http://{address}/recovery.zip")
    }

    #[test]
    fn forbidden_mirror_falls_back_without_revealing_signed_url() {
        let directory = tempfile::tempdir().unwrap();
        let forbidden = serve_status(b"denied", "403 Forbidden");
        let working = serve_once(b"hello");
        let response = serde_json::json!({
            "LatestRom": {"filename": "recovery.zip?t=123&s=private-signature", "md5": "5d41402abc4b2a76b9719d911017c592"},
            "MirrorList": [forbidden.trim_end_matches("/recovery.zip"), working.trim_end_matches("/recovery.zip")]
        });
        let path =
            download_latest(&Client::new(), &response.to_string(), directory.path()).unwrap();
        assert_eq!(fs::read(path).unwrap(), b"hello");
        let url = format!(
            "{}?s=private-signature",
            serve_status(b"denied", "403 Forbidden")
        );
        let error = download_with_md5(
            &Client::new(),
            &url,
            directory.path(),
            "5d41402abc4b2a76b9719d911017c592",
        )
        .unwrap_err();
        let message = format!("{error:#}");
        assert!(message.contains("403"));
        assert!(!message.contains("private-signature"));
        assert!(!message.contains("http://"));
    }

    #[test]
    fn searches_public_cdn_paths_without_reusing_private_signatures() {
        let latest = LatestInfo { filename: "garnet_tw_global-ota_full-OS3.0.303.0.WNRTWXM-user-16.0-example.zip?t=123&s=private".into(), md5: "unused".into() };
        let candidates = mirror_candidates(&latest, &["https://ultimateota.d.miui.com".into()]);
        assert!(candidates[0].url.query().unwrap().contains("s=private"));
        for host in PUBLIC_CDNS {
            assert!(candidates.iter().any(|c| c.public_search
                && c.url.host_str() == Some(host)
                && c.url.path().starts_with("/OS3.0.303.0.WNRTWXM/")));
        }
        assert!(candidates
            .iter()
            .filter(|c| c.public_search)
            .all(|c| c.url.query().is_none()));
        assert_eq!(
            candidates
                .iter()
                .map(|c| c.url.as_str())
                .collect::<HashSet<_>>()
                .len(),
            candidates.len()
        );
    }

    #[test]
    fn mirror_probe_rejects_error_pages_even_when_http_status_is_success() {
        let client = Client::new();
        assert!(probe_mirror(
            &client,
            &reqwest::Url::parse(&serve_once(b"<html>not a ROM")).unwrap()
        )
        .is_err());
        assert!(probe_mirror(
            &client,
            &reqwest::Url::parse(&serve_status(b"PK\x03\x04package", "206 Partial Content"))
                .unwrap()
        )
        .is_ok());
        assert!(probe_mirror(
            &client,
            &reqwest::Url::parse(&serve_status(b"denied", "403 Forbidden")).unwrap()
        )
        .is_err());
    }

    #[test]
    fn fallback_reaches_a_working_mirror_beyond_the_old_eight_mirror_limit() {
        let directory = tempfile::tempdir().unwrap();
        let mut mirrors: Vec<String> = (0..8)
            .map(|_| {
                serve_status(b"denied", "403 Forbidden")
                    .trim_end_matches("/recovery.zip")
                    .to_owned()
            })
            .collect();
        mirrors.push(
            serve_once(b"hello")
                .trim_end_matches("/recovery.zip")
                .to_owned(),
        );
        let response = serde_json::json!({"LatestRom": {"filename": "recovery.zip", "md5": "5d41402abc4b2a76b9719d911017c592"}, "MirrorList": mirrors});
        let path =
            download_latest(&Client::new(), &response.to_string(), directory.path()).unwrap();
        assert_eq!(fs::read(path).unwrap(), b"hello");
    }

    #[test]
    fn corrupt_mirror_cannot_bypass_checksum_validation() {
        let directory = tempfile::tempdir().unwrap();
        let bad = serve_once(b"corrupt");
        let good = serve_once(b"hello");
        let response = serde_json::json!({"LatestRom": {"filename": "recovery.zip", "md5": "5d41402abc4b2a76b9719d911017c592"},
            "MirrorList": [bad.trim_end_matches("/recovery.zip"), good.trim_end_matches("/recovery.zip")]});
        let path =
            download_latest(&Client::new(), &response.to_string(), directory.path()).unwrap();
        assert_eq!(fs::read(path).unwrap(), b"hello");
        assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 1);
    }

    #[test]
    fn parses_latest_rom_and_prefers_https_mirror() {
        let json = r#"{
            "LatestRom":{"filename":"rom.zip?t=1","md5":"abc"},
            "MirrorList":["http://mirror-one", "https://mirror-two"]
        }"#;
        let (latest, mirrors) = parse_latest_from_json(json).unwrap();

        assert_eq!(latest.filename, "rom.zip?t=1");
        assert_eq!(latest.md5, "abc");
        assert_eq!(
            choose_url(&mirrors, &latest.filename).unwrap(),
            "https://mirror-two/rom.zip?t=1"
        );
    }

    #[test]
    fn verified_download_is_atomically_finalized() {
        let directory = tempfile::tempdir().unwrap();
        let url = serve_once(b"hello");
        let path = download_with_md5(
            &Client::new(),
            &url,
            directory.path(),
            "5d41402abc4b2a76b9719d911017c592",
        )
        .unwrap();

        assert_eq!(fs::read(path).unwrap(), b"hello");
        assert!(!directory.path().join(".recovery.zip.part").exists());
    }

    #[test]
    fn checksum_failure_removes_partial_download() {
        let directory = tempfile::tempdir().unwrap();
        let url = serve_once(b"corrupt");
        let result = download_with_md5(
            &Client::new(),
            &url,
            directory.path(),
            "5d41402abc4b2a76b9719d911017c592",
        );

        assert!(result.unwrap_err().to_string().contains("MD5 mismatch"));
        assert!(!directory.path().join("recovery.zip").exists());
        assert!(!directory.path().join(".recovery.zip.part").exists());
    }

    #[test]
    fn verified_download_replaces_existing_rom_without_touching_other_staging_files() {
        let directory = tempfile::tempdir().unwrap();
        let dest = directory.path().join("recovery.zip");
        let other_partial = directory.path().join(".recovery.zip.part");
        fs::write(&dest, b"old ROM").unwrap();
        fs::write(&other_partial, b"another transfer").unwrap();

        download_with_md5(
            &Client::new(),
            &serve_once(b"hello"),
            directory.path(),
            "5d41402abc4b2a76b9719d911017c592",
        )
        .unwrap();

        assert_eq!(fs::read(dest).unwrap(), b"hello");
        assert_eq!(fs::read(other_partial).unwrap(), b"another transfer");
        assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 2);
    }

    #[test]
    fn failed_verification_preserves_existing_rom_and_cleans_staging() {
        let directory = tempfile::tempdir().unwrap();
        let dest = directory.path().join("recovery.zip");
        fs::write(&dest, b"existing ROM").unwrap();

        let result = download_with_md5(
            &Client::new(),
            &serve_once(b"corrupt"),
            directory.path(),
            "5d41402abc4b2a76b9719d911017c592",
        );

        assert!(result.unwrap_err().to_string().contains("MD5 mismatch"));
        assert_eq!(fs::read(dest).unwrap(), b"existing ROM");
        assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 1);
    }

    #[test]
    fn failed_finalization_preserves_destination_and_cleans_staging() {
        let directory = tempfile::tempdir().unwrap();
        let dest = directory.path().join("recovery.zip");
        fs::create_dir(&dest).unwrap();
        fs::write(dest.join("keep"), b"existing data").unwrap();

        let error = download_with_md5(
            &Client::new(),
            &serve_once(b"hello"),
            directory.path(),
            "5d41402abc4b2a76b9719d911017c592",
        )
        .unwrap_err();

        assert!(error.to_string().contains("finalize verified download"));
        assert_eq!(fs::read(dest.join("keep")).unwrap(), b"existing data");
        assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 1);
    }
}
