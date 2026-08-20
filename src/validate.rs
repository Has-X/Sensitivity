// Copyright (C) 2026 HasX
// Licensed under the GNU AGPL v3.0. See LICENSE file for details.
// Website: https://hasx.dev

use anyhow::{anyhow, bail, Context, Result};
use base64::{engine::general_purpose, Engine as _};
use cbc::cipher::{block_padding::Pkcs7, BlockDecryptMut, BlockEncryptMut, KeyIvInit};
// (no-op)
use reqwest::blocking::Client;
use serde_json::{Map, Value};
use std::time::Duration;

use crate::mi::DeviceInfo;

type Aes128CbcEnc = cbc::Encryptor<aes::Aes128>;
type Aes128CbcDec = cbc::Decryptor<aes::Aes128>;

// Hardcoded key/iv placeholders; replace with values from original C if needed.
// AES-128-CBC key/iv as in original C (miasst.c)
const DEFAULT_KEY: [u8; 16] = [
    0x6D, 0x69, 0x75, 0x69, 0x6F, 0x74, 0x61, 0x76, 0x61, 0x6C, 0x69, 0x64, 0x65, 0x64, 0x31, 0x31,
];
const DEFAULT_IV: [u8; 16] = [
    0x30, 0x31, 0x30, 0x32, 0x30, 0x33, 0x30, 0x34, 0x30, 0x35, 0x30, 0x36, 0x30, 0x37, 0x30, 0x38,
];

fn get_key_iv() -> ([u8; 16], [u8; 16]) {
    fn parse_hex_16(s: &str) -> Option<[u8; 16]> {
        let s = s.trim();
        if s.len() != 32 {
            return None;
        }
        let mut out = [0u8; 16];
        for i in 0..16 {
            let byte = u8::from_str_radix(&s[i * 2..i * 2 + 2], 16).ok()?;
            out[i] = byte;
        }
        Some(out)
    }
    let key = std::env::var("SENSITIVITY_AES_KEY")
        .ok()
        .and_then(|v| parse_hex_16(&v));
    let iv = std::env::var("SENSITIVITY_AES_IV")
        .ok()
        .and_then(|v| parse_hex_16(&v));
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
    let md5 = md5_opt.unwrap_or_default();
    let zone_text = info.romzone.trim();
    let zone = serde_json::from_str::<Value>(zone_text)
        .unwrap_or_else(|_| Value::String(zone_text.to_owned()));
    // Recovery reports the language that Xiaomi associated with this device.
    // Do not replace it unconditionally with en-US: regional Recovery builds
    // can reject a request whose profile fields disagree with each other.
    let language = if info.language.trim().is_empty() {
        "en-US"
    } else {
        info.language.trim()
    };
    serde_json::to_string(&serde_json::json!({
        "d": info.device,
        "v": info.version,
        "c": info.codebase,
        "b": info.branch,
        "sn": info.sn,
        "l": language,
        "f": "1",
        "options": {"zone": zone},
        "pkg": md5,
    }))
    .context("Serializing Xiaomi validation request")
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
    buf.extend(std::iter::repeat_n(0u8, pad_len));
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
        .map_err(|e| {
            anyhow!(
                "AES-128-CBC decrypt failed: {} (cipher {} bytes)",
                e,
                cipher.len()
            )
        })?;
    Ok(dec.to_vec())
}

fn extract_json_braces(text: &str) -> Option<String> {
    let start = text.find('{')?;
    let end = text.rfind('}')?;
    if end <= start {
        return None;
    }
    Some(text[start..=end].to_string())
}

fn field_ignore_ascii_case<'a>(object: &'a Map<String, Value>, name: &str) -> Option<&'a Value> {
    object
        .iter()
        .find_map(|(key, value)| key.eq_ignore_ascii_case(name).then_some(value))
}

fn nonempty_string(value: Option<&Value>) -> Option<String> {
    value
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

fn token_from_object(object: &Map<String, Value>) -> Option<String> {
    for name in ["Validate", "Token", "ValidateToken"] {
        let value = field_ignore_ascii_case(object, name);
        if let Some(token) = nonempty_string(value) {
            return Some(token);
        }
        if let Some(nested) = value.and_then(Value::as_object) {
            for nested_name in ["Token", "ValidateToken"] {
                if let Some(token) = nonempty_string(field_ignore_ascii_case(nested, nested_name)) {
                    return Some(token);
                }
            }
        }
    }
    None
}

fn erase_from_value(value: Option<&Value>) -> Option<i32> {
    let value = value?;
    if let Some(number) = value.as_i64() {
        return i32::try_from(number).ok();
    }
    value.as_str()?.trim().parse().ok()
}

fn parse_validate_response(
    json_text: &str,
    _plaintext_len: usize,
    raw_plaintext_head: String,
) -> Result<ValidateResult> {
    let root =
        serde_json::from_str::<Value>(json_text).context("Parsing JSON in server response")?;
    let root = root
        .as_object()
        .ok_or_else(|| anyhow!("Validation response root is not a JSON object"))?;
    let mut out = ValidateResult {
        raw_plaintext_head: Some(raw_plaintext_head),
        full_json: Some(json_text.to_string()),
        ..ValidateResult::default()
    };

    if let Some(pkg) = field_ignore_ascii_case(root, "PkgRom").and_then(Value::as_object) {
        if let Some(validate) = field_ignore_ascii_case(pkg, "Validate") {
            if let Some(values) = validate.as_array() {
                out.pkgrom_validate = Some(
                    values
                        .iter()
                        .filter_map(Value::as_str)
                        .map(str::to_owned)
                        .collect(),
                );
            }
        }
        out.validate_token = token_from_object(pkg);
        out.pkgrom_erase = erase_from_value(field_ignore_ascii_case(pkg, "Erase"));
    }

    // Xiaomi has returned casing and nesting variants across regions. Restrict
    // the fallback to documented token field names at the response root.
    if out.validate_token.is_none() {
        out.validate_token = token_from_object(root);
    }
    if let Some(code) = field_ignore_ascii_case(root, "Code").and_then(Value::as_object) {
        out.code_message = nonempty_string(field_ignore_ascii_case(code, "message"));
    }
    if out.pkgrom_validate.is_none() && out.code_message.is_none() && out.validate_token.is_none() {
        bail!(
            "Xiaomi's response did not contain the expected validation data. \
            Try a different region profile, or use --dump-json to inspect the raw server response."
        );
    }
    Ok(out)
}

pub fn validate(server_url: &str, json_body: &str) -> Result<ValidateResult> {
    let enc = aes128_cbc_encrypt_b64(json_body.as_bytes())?;
    let form = [("q", enc.as_str()), ("t", ""), ("s", "1")];
    let client = Client::builder().timeout(Duration::from_secs(30)).build()?;
    let resp = client
        .post(server_url)
        .header("User-Agent", "MiTunes_UserAgent_v3.0")
        .form(&form)
        .send();
    let resp = match resp {
        Ok(r) => r,
        Err(e) => bail!("HTTP request failed: {}", e),
    };
    let status = resp.status();
    let text = resp.text().unwrap_or_default();
    if !status.is_success() {
        let head = text.bytes().take(200).collect::<Vec<_>>();
        let head_hex = hex::encode(&head);
        bail!(
            "Validation failed: HTTP {}. First {} bytes: {}",
            status.as_u16(),
            head.len(),
            head_hex
        );
    }
    if text.trim().is_empty() {
        bail!("Validation failed: empty response body");
    }
    let plain = aes128_cbc_decrypt_b64(&text).context("Decrypting server response")?;
    let preview = String::from_utf8_lossy(&plain);
    let json_text = extract_json_braces(&preview)
        .ok_or_else(|| anyhow!("No JSON object found in plaintext (len {})", plain.len()))?;
    parse_validate_response(&json_text, plain.len(), preview.chars().take(200).collect())
}

pub fn print_allowed_with_options(res: &ValidateResult, dump_json: bool) {
    if dump_json {
        if let Some(j) = &res.full_json {
            println!("{}", j);
            return;
        }
    }
    // Prefer explicit allowed list (PkgRom.Validate)
    if let Some(list) = &res.pkgrom_validate {
        if list.is_empty() {
            println!("No allowed ROMs reported by server.");
        } else {
            println!("Allowed ROMs:");
            for s in list {
                println!("- {}", s);
            }
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
                    if k == "Icon" {
                        continue;
                    }
                    if let Some(o) = v.as_object() {
                        let name = o.get("name").and_then(|x| x.as_str());
                        let md5 = o.get("md5").and_then(|x| x.as_str());
                        if let (Some(name), Some(md5)) = (name, md5) {
                            println!("{}: {}\nmd5: {}\n", k, name, md5);
                            printed = true;
                        }
                    }
                }
                if printed {
                    return;
                }
            }
        }
    }

    // Last resort: print server message if any
    if let Some(msg) = &res.code_message {
        println!("{}", msg);
    } else {
        println!("Server did not include allowed ROM list.");
    }
}

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

    #[test]
    fn request_json_escapes_textual_rom_zone() {
        let info = DeviceInfo {
            device: "device\"name".to_owned(),
            sn: "serial".to_owned(),
            version: "version".to_owned(),
            codebase: "garnet".to_owned(),
            branch: "stable".to_owned(),
            language: "en".to_owned(),
            region: "Global".to_owned(),
            romzone: "global".to_owned(),
        };
        let request = build_request_json(&info, None).expect("request should serialize");
        let value: Value = serde_json::from_str(&request).expect("request should be valid JSON");
        assert_eq!(value["d"], "device\"name");
        assert_eq!(value["l"], "en");
        assert_eq!(value["options"]["zone"], "global");
    }

    #[test]
    fn request_json_preserves_numeric_rom_zone() {
        let info = DeviceInfo {
            device: "device".to_owned(),
            sn: "serial".to_owned(),
            version: "version".to_owned(),
            codebase: "garnet".to_owned(),
            branch: "stable".to_owned(),
            language: "en".to_owned(),
            region: "Global".to_owned(),
            romzone: "1".to_owned(),
        };
        let request = build_request_json(&info, None).expect("request should serialize");
        let value: Value = serde_json::from_str(&request).expect("request should be valid JSON");
        assert_eq!(value["options"]["zone"], 1);
    }

    #[test]
    fn request_json_uses_safe_default_language_when_recovery_omits_it() {
        let info = DeviceInfo {
            device: "device".to_owned(),
            sn: "serial".to_owned(),
            version: "version".to_owned(),
            codebase: "16".to_owned(),
            branch: "F".to_owned(),
            language: "  ".to_owned(),
            region: "PL".to_owned(),
            romzone: "2".to_owned(),
        };
        let request = build_request_json(&info, None).expect("request should serialize");
        let value: Value = serde_json::from_str(&request).expect("request should be valid JSON");
        assert_eq!(value["l"], "en-US");
    }

    #[test]
    fn parses_standard_validate_token() {
        let response = parse_validate_response(
            r#"{"Code":{"message":"success"},"PkgRom":{"Validate":"token-123","Erase":1}}"#,
            70,
            "response".to_string(),
        )
        .unwrap();
        assert_eq!(response.validate_token.as_deref(), Some("token-123"));
        assert_eq!(response.pkgrom_erase, Some(1));
        assert_eq!(response.code_message.as_deref(), Some("success"));
    }

    #[test]
    fn parses_casing_and_nested_token_variants() {
        let response = parse_validate_response(
            r#"{"code":{"Message":"success"},"pkgrom":{"validate":{"token":"token-456"},"erase":"1"}}"#,
            88,
            "response".to_string(),
        ).unwrap();
        assert_eq!(response.validate_token.as_deref(), Some("token-456"));
        assert_eq!(response.pkgrom_erase, Some(1));
        assert_eq!(response.code_message.as_deref(), Some("success"));
    }
}
