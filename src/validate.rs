use anyhow::{anyhow, bail, Context, Result};
use base64::{engine::general_purpose, Engine as _};
use cbc::cipher::{block_padding::Pkcs7, BlockDecryptMut, BlockEncryptMut, KeyIvInit};
// (no-op)
use reqwest::blocking::Client;
use serde::Deserialize;
use std::time::Duration;

use crate::mi::DeviceInfo;

type Aes128CbcEnc = cbc::Encryptor<aes::Aes128>;
type Aes128CbcDec = cbc::Decryptor<aes::Aes128>;

// Hardcoded key/iv placeholders; replace with values from original C if needed.
// AES-128-CBC key/iv as in original C (miasst.c)
const DEFAULT_KEY: [u8; 16] = [
    0x6D, 0x69, 0x75, 0x69, 0x6F, 0x74, 0x61, 0x76,
    0x61, 0x6C, 0x69, 0x64, 0x65, 0x64, 0x31, 0x31,
];
const DEFAULT_IV: [u8; 16] = [
    0x30, 0x31, 0x30, 0x32, 0x30, 0x33, 0x30, 0x34,
    0x30, 0x35, 0x30, 0x36, 0x30, 0x37, 0x30, 0x38,
];

fn get_key_iv() -> ([u8; 16], [u8; 16]) {
    fn parse_hex_16(s: &str) -> Option<[u8; 16]> {
        let s = s.trim();
        if s.len() != 32 { return None; }
        let mut out = [0u8; 16];
        for i in 0..16 {
            let byte = u8::from_str_radix(&s[i*2..i*2+2], 16).ok()?;
            out[i] = byte;
        }
        Some(out)
    }
    let key = std::env::var("SENSITIVITY_AES_KEY").ok().and_then(|v| parse_hex_16(&v));
    let iv = std::env::var("SENSITIVITY_AES_IV").ok().and_then(|v| parse_hex_16(&v));
    (key.unwrap_or(DEFAULT_KEY), iv.unwrap_or(DEFAULT_IV))
}

#[derive(Debug, Default, Clone)]
pub struct ValidateResult {
    pub pkgrom_validate: Option<Vec<String>>,
    pub pkgrom_erase: Option<i32>,
    pub code_message: Option<String>,
    pub validate_token: Option<String>,
    pub raw_plaintext_head: Option<String>,
    pub full_json: Option<String>,
}

pub fn build_request_json(info: &DeviceInfo, md5_opt: Option<String>) -> Result<String> {
    let md5 = md5_opt.unwrap_or_else(|| "".to_string());
    // Replicate C behavior exactly: inject romzone verbatim (may be non-numeric like F)
    let zone_field = info.romzone.trim().to_string();
    let esc = |s: &str| s.replace('"', "\\\"");
    let json = format!(
        "{{\"d\":\"{}\",\"v\":\"{}\",\"c\":\"{}\",\"b\":\"{}\",\"sn\":\"{}\",\"l\":\"en-US\",\"f\":\"1\",\"options\":{{\"zone\":{}}},\"pkg\":\"{}\"}}",
        esc(&info.device),
        esc(&info.version),
        esc(&info.codebase),
        esc(&info.branch),
        esc(&info.sn),
        zone_field,
        esc(&md5),
    );
    Ok(json)
}

// Expose encoder so CLI can print base64 `q` payload like forked C
pub fn encode_request_b64(json_body: &str) -> Result<String> {
    aes128_cbc_encrypt_b64(json_body.as_bytes())
}

fn aes128_cbc_encrypt_b64(plain: &[u8]) -> Result<String> {
    let (key, iv) = get_key_iv();
    let mut buf = plain.to_vec();
    // reserve space for padding to next multiple of block size
    let bs = 16;
    let pad_len = bs - (buf.len() % bs);
    buf.extend(std::iter::repeat(0u8).take(pad_len));
    let enc_slice = Aes128CbcEnc::new(&key.into(), &iv.into())
        .encrypt_padded_mut::<Pkcs7>(&mut buf, plain.len())?;
    let ciphertext = enc_slice.to_vec();
    Ok(general_purpose::STANDARD.encode(&ciphertext))
}

fn aes128_cbc_decrypt_b64(b64: &str) -> Result<Vec<u8>> {
    let (key, iv) = get_key_iv();
    let cipher = match general_purpose::STANDARD.decode(b64) {
        Ok(c) => c,
        Err(e) => bail!("Base64 decode failed: {}", e),
    };
    let mut buf = cipher.clone();
    let dec = Aes128CbcDec::new(&key.into(), &iv.into())
        .decrypt_padded_mut::<Pkcs7>(&mut buf)
        .map_err(|e| anyhow!("AES-128-CBC decrypt failed: {} (cipher {} bytes)", e, cipher.len()))?;
    Ok(dec.to_vec())
}

fn extract_json_braces(text: &str) -> Option<String> {
    let start = text.find('{')?;
    let end = text.rfind('}')?;
    if end <= start { return None; }
    Some(text[start..=end].to_string())
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum ValidateField {
    Str(String),
    Arr(Vec<String>),
}

#[derive(Debug, Deserialize)]
struct ResponsePkgRom {
    #[serde(default)]
    Validate: Option<ValidateField>,
    #[serde(default)]
    Erase: Option<i32>,
    #[serde(default)]
    Token: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ResponseCode { #[serde(default)] message: String }

#[derive(Debug, Deserialize)]
struct ResponseRoot { #[serde(default)] PkgRom: Option<ResponsePkgRom>, #[serde(default)] Code: Option<ResponseCode> }

pub fn validate(server_url: &str, json_body: &str) -> Result<ValidateResult> {
    let enc = aes128_cbc_encrypt_b64(json_body.as_bytes())?;
    let form = [
        ("q", enc.as_str()),
        ("t", ""),
        ("s", "1"),
    ];
    let client = Client::builder()
        .timeout(Duration::from_secs(30))
        .build()?;
    let resp = client
        .post(server_url)
        .header("User-Agent", "MiTunes_UserAgent_v3.0")
        .form(&form)
        .send();
    let resp = match resp { Ok(r) => r, Err(e) => bail!("HTTP request failed: {}", e) };
    let status = resp.status();
    let text = resp.text().unwrap_or_default();
    if !status.is_success() {
        let head = text.bytes().take(200).collect::<Vec<_>>();
        let head_hex = hex::encode(&head);
        bail!("Validation failed: HTTP {}. First {} bytes: {}", status.as_u16(), head.len(), head_hex);
    }
    if text.trim().is_empty() {
        bail!("Validation failed: empty response body");
    }
    let plain = aes128_cbc_decrypt_b64(&text).context("Decrypting server response")?;
    let preview = String::from_utf8_lossy(&plain);
    let json_text = extract_json_braces(&preview).ok_or_else(|| anyhow!("No JSON object found in plaintext (len {})", plain.len()))?;
    let root: ResponseRoot = serde_json::from_str(&json_text).context("Parsing JSON in server response")?;
    let mut out = ValidateResult::default();
    if let Some(pkg) = root.PkgRom {
        if let Some(v) = pkg.Validate {
            match v {
                ValidateField::Arr(list) => out.pkgrom_validate = Some(list),
                ValidateField::Str(s) => out.validate_token = Some(s),
            }
        }
        if out.validate_token.is_none() {
            if let Some(tok) = pkg.Token { out.validate_token = Some(tok); }
        }
        out.pkgrom_erase = pkg.Erase;
    }
    if let Some(code) = root.Code { if !code.message.is_empty() { out.code_message = Some(code.message); } }
    out.raw_plaintext_head = Some(preview.chars().take(200).collect());
    out.full_json = Some(json_text.clone());
    if out.pkgrom_validate.is_none() && out.code_message.is_none() {
        bail!(
            "Validation response missing expected keys (PkgRom.Validate or Code.message). Plaintext length {}. Head: {}",
            plain.len(),
            out.raw_plaintext_head.clone().unwrap_or_default()
        );
    }
    Ok(out)
}

pub fn print_allowed_with_options(res: &ValidateResult, dump_json: bool) {
    if dump_json {
        if let Some(j) = &res.full_json { println!("{}", j); return; }
    }
    // Prefer explicit allowed list (PkgRom.Validate)
    if let Some(list) = &res.pkgrom_validate {
        if list.is_empty() {
            println!("No allowed ROMs reported by server.");
        } else {
            println!("Allowed ROMs:");
            for s in list { println!("- {}", s); }
        }
        return;
    }

    // Fallback: parse top-level JSON and print entries with name/md5 (as miasst.c does for list-allowed-roms)
    if let Some(json_str) = &res.full_json {
        if let Ok(val) = serde_json::from_str::<serde_json::Value>(json_str) {
            if let Some(obj) = val.as_object() {
                // Detect invalid data like C code
                if obj.contains_key("Signup") || obj.contains_key("VersionBoot") {
                    eprintln!("Error: Invalid data");
                    return;
                }
                let mut printed = false;
                for (k, v) in obj {
                    if k == "Icon" { continue; }
                    if let Some(o) = v.as_object() {
                        let name = o.get("name").and_then(|x| x.as_str());
                        let md5 = o.get("md5").and_then(|x| x.as_str());
                        if let (Some(name), Some(md5)) = (name, md5) {
                            println!("{}: {}\nmd5: {}\n", k, name, md5);
                            printed = true;
                        }
                    }
                }
                if printed { return; }
            }
        }
    }

    // Last resort: print server message if any
    if let Some(msg) = &res.code_message { println!("{}", msg); }
    else { println!("Server did not include allowed ROM list."); }
}

pub fn print_allowed(res: &ValidateResult) { print_allowed_with_options(res, false) }
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_aes_b64_roundtrip() {
        let msg = b"hello world aes-128-cbc";
        let e = aes128_cbc_encrypt_b64(msg).unwrap();
        let d = aes128_cbc_decrypt_b64(&e).unwrap();
        assert_eq!(d, msg);
    }

    #[test]
    fn test_extract_json() {
        let s = "garbage { \"a\": 1 } trailing";
        let j = extract_json_braces(s).unwrap();
        assert_eq!(j, "{ \"a\": 1 }");
    }
}
