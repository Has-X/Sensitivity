// Copyright (C) 2026 HasX
// Licensed under the GNU AGPL v3.0. See LICENSE file for details.
// Website: https://hasx.dev

use anyhow::{anyhow, bail, Context, Result};
use indicatif::{ProgressBar, ProgressStyle};
use md5::Digest;
use reqwest::blocking::Client;
use std::fs::{self, File};
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
    // derive file name (strip query)
    let base = url.split('/').next_back().unwrap_or("download.zip");
    let base = base.split('?').next().unwrap_or(base);
    let base = if base.is_empty() {
        "download.zip"
    } else {
        base
    };
    let dest = dest_dir.join(base);
    let partial = dest_dir.join(format!(".{base}.part"));

    let resp = client
        .get(url)
        .send()
        .with_context(|| format!("GET {}", url))?;
    if !resp.status().is_success() {
        bail!("Download failed: HTTP {} from {}", resp.status(), url);
    }
    let len = resp.content_length();
    let pb = ProgressBar::new(len.unwrap_or(0));
    pb.set_style(ProgressStyle::default_bar()
        .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({percent}%)")
        .unwrap()
        .progress_chars("=>-"));

    let transfer = (|| -> Result<String> {
        let mut hasher = md5::Md5::new();
        let mut file =
            File::create(&partial).with_context(|| format!("create {}", partial.display()))?;
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
        Err(error) => {
            let _ = fs::remove_file(&partial);
            return Err(error).context("Downloading ROM package");
        }
    };
    if got.to_lowercase() != expect_md5.to_lowercase() {
        let _ = fs::remove_file(&partial);
        bail!(
            "MD5 mismatch after download: got {}, expected {}",
            got,
            expect_md5
        );
    }
    if dest.exists() {
        fs::remove_file(&dest)
            .with_context(|| format!("replace existing download {}", dest.display()))?;
    }
    fs::rename(&partial, &dest)
        .with_context(|| format!("finalize verified download {}", dest.display()))?;
    Ok(dest)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;
    use std::thread;

    fn serve_once(body: &'static [u8]) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = [0u8; 1024];
            let _ = stream.read(&mut request);
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            )
            .unwrap();
            stream.write_all(body).unwrap();
        });
        format!("http://{address}/recovery.zip")
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
}
