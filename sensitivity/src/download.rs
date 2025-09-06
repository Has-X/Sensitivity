use anyhow::{anyhow, bail, Context, Result};
use indicatif::{ProgressBar, ProgressStyle};
use reqwest::blocking::Client;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use md5::Digest; // brings Md5::new() into scope

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
        .map(|arr| arr.iter().filter_map(|s| s.as_str().map(|t| t.to_string())).collect::<Vec<_>>())
        .unwrap_or_default();
    Ok((LatestInfo { filename, md5 }, mirrors))
}

pub fn choose_url(mirrors: &[String], filename: &str) -> Option<String> {
    for base in mirrors {
        // Prefer https mirrors
        if !base.starts_with("https://") { continue; }
        let url = format!("{}/{}", base.trim_end_matches('/'), filename.trim_start_matches('/'));
        return Some(url);
    }
    // fallback to any mirror
    mirrors.first().map(|b| format!("{}/{}", b.trim_end_matches('/'), filename))
}

pub fn download_with_md5(client: &Client, url: &str, dest_dir: &Path, expect_md5: &str) -> Result<PathBuf> {
    fs::create_dir_all(dest_dir).ok();
    // derive file name (strip query)
    let base = url.split('/').last().unwrap_or("download.zip");
    let base = base.split('?').next().unwrap_or(base);
    let dest = dest_dir.join(base);

    let resp = client.get(url).send().with_context(|| format!("GET {}", url))?;
    if !resp.status().is_success() {
        bail!("Download failed: HTTP {} from {}", resp.status(), url);
    }
    let len = resp.content_length();
    let pb = ProgressBar::new(len.unwrap_or(0));
    pb.set_style(ProgressStyle::default_bar()
        .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({percent}%)")
        .unwrap()
        .progress_chars("=>-"));

    let mut hasher = md5::Md5::new();
    let mut file = File::create(&dest).with_context(|| format!("create {}", dest.display()))?;
    let mut src = resp;
    let mut buf = [0u8; 128 * 1024];
    loop {
        let n = src.read(&mut buf)?;
        if n == 0 { break; }
        file.write_all(&buf[..n])?;
        hasher.update(&buf[..n]);
        if let Some(total) = len { pb.set_position(std::cmp::min(pb.position()+ (n as u64), total)); } else { pb.inc(n as u64); }
    }
    pb.finish_and_clear();
    let got = format!("{:x}", hasher.finalize());
    if got.to_lowercase() != expect_md5.to_lowercase() {
        bail!("MD5 mismatch after download: got {}, expected {}", got, expect_md5);
    }
    Ok(dest)
}
