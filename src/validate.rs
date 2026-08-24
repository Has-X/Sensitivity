// Copyright (C) 2026 Chromatic
// Licensed under the GNU AGPL v3.0. See LICENSE file for details.
// Website: https://chromatic.hu

use crate::i18n::tr;
use anyhow::{anyhow, bail, Context, Result};
use base64::{engine::general_purpose, Engine as _};
use cbc::cipher::{block_padding::Pkcs7, BlockModeDecrypt, BlockModeEncrypt, KeyIvInit};
use reqwest::blocking::Client;
use serde_json::{Map, Value};
use std::time::Duration;

use crate::mi::DeviceInfo;

type Aes128CbcEnc = cbc::Encryptor<aes::Aes128>;
type Aes128CbcDec = cbc::Decryptor<aes::Aes128>;

// AES-128-CBC key and IV used by Xiaomi's Mi Assistant validation protocol.
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
    pub full_json: Option<String>,
}

pub fn build_request_json(info: &DeviceInfo, md5_opt: Option<String>) -> Result<String> {
    let md5 = md5_opt.unwrap_or_default();
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

fn aes128_cbc_encrypt_b64(plain: &[u8]) -> Result<String> {
    let (key, iv) = get_key_iv();
    let mut buf = plain.to_vec();
    // reserve space for padding to next multiple of block size
    let bs = 16;
    let pad_len = bs - (buf.len() % bs);
    buf.extend(std::iter::repeat_n(0u8, pad_len));
    let enc_slice = Aes128CbcEnc::new(&key.into(), &iv.into())
        .encrypt_padded::<Pkcs7>(&mut buf, plain.len())?;
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
        .decrypt_padded::<Pkcs7>(&mut buf)
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

fn object_value_ci<'a>(object: &'a Map<String, Value>, key: &str) -> Option<&'a Value> {
    object
        .iter()
        .find_map(|(name, value)| name.eq_ignore_ascii_case(key).then_some(value))
}

fn find_key_recursive_ci<'a>(value: &'a Value, key: &str) -> Option<&'a Value> {
    match value {
        Value::Object(object) => object_value_ci(object, key).or_else(|| {
            object
                .values()
                .find_map(|child| find_key_recursive_ci(child, key))
        }),
        Value::Array(items) => items
            .iter()
            .find_map(|child| find_key_recursive_ci(child, key)),
        _ => None,
    }
}

fn non_empty_string(value: &Value) -> Option<String> {
    value
        .as_str()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn token_from_validate(value: &Value) -> Option<String> {
    if let Some(token) = non_empty_string(value) {
        return Some(token);
    }
    let object = value.as_object()?;
    object_value_ci(object, "token")
        .and_then(non_empty_string)
        .or_else(|| object_value_ci(object, "validate").and_then(token_from_validate))
}

fn erase_value(value: &Value) -> Option<i32> {
    value
        .as_i64()
        .and_then(|number| i32::try_from(number).ok())
        .or_else(|| value.as_str().and_then(|number| number.parse().ok()))
        .or_else(|| value.as_bool().map(i32::from))
}

fn parse_validation_response(json_text: &str) -> Result<ValidateResult> {
    let root: Value = serde_json::from_str(json_text).context("Parsing JSON in server response")?;
    let mut out = ValidateResult {
        full_json: Some(json_text.to_owned()),
        ..ValidateResult::default()
    };

    if let Some(pkg) = find_key_recursive_ci(&root, "PkgRom") {
        if let Some(object) = pkg.as_object() {
            if let Some(validate) = object_value_ci(object, "Validate") {
                if let Some(items) = validate.as_array() {
                    out.pkgrom_validate = Some(
                        items
                            .iter()
                            .filter_map(non_empty_string)
                            .collect::<Vec<_>>(),
                    );
                } else {
                    out.validate_token = token_from_validate(validate);
                }
            }
            if out.validate_token.is_none() {
                out.validate_token = object_value_ci(object, "Token").and_then(non_empty_string);
            }
            out.pkgrom_erase = object_value_ci(object, "Erase").and_then(erase_value);
        }
    }

    if let Some(code) = find_key_recursive_ci(&root, "Code") {
        out.code_message = code
            .as_object()
            .and_then(|object| object_value_ci(object, "message"))
            .and_then(non_empty_string);
    }

    Ok(out)
}

fn redact_json_value(value: &Value) -> Value {
    match value {
        Value::Object(object) => Value::Object(
            object
                .iter()
                .map(|(key, value)| (key.clone(), redact_json_value(value)))
                .collect(),
        ),
        Value::Array(items) => Value::Array(items.iter().map(redact_json_value).collect()),
        Value::String(_) => Value::String("<redacted>".to_owned()),
        Value::Number(_) => Value::Number(0.into()),
        Value::Bool(_) => Value::Bool(false),
        Value::Null => Value::Null,
    }
}

pub fn redacted_response_json(result: &ValidateResult) -> Result<String> {
    let raw = result
        .full_json
        .as_deref()
        .ok_or_else(|| anyhow!("No validation response JSON is available"))?;
    let value: Value = serde_json::from_str(raw).context("Parsing validation response for dump")?;
    serde_json::to_string_pretty(&redact_json_value(&value))
        .context("Serializing redacted validation response")
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
    parse_validation_response(&json_text)
}

pub fn print_allowed(res: &ValidateResult) {
    // Prefer explicit allowed list (PkgRom.Validate)
    if let Some(list) = &res.pkgrom_validate {
        if list.is_empty() {
            println!("{}", tr("status.no_allowed_roms"));
        } else {
            println!("{}", tr("status.allowed_roms"));
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
                    eprintln!("{}: Invalid data", tr("error.prefix"));
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
        println!("{}", tr("status.no_allowed_roms"));
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
    fn parses_standard_validation_token() {
        let result = parse_validation_response(
            r#"{"PkgRom":{"Validate":"secret-token","Erase":1},"Code":{"message":"success"}}"#,
        )
        .unwrap();
        assert_eq!(result.validate_token.as_deref(), Some("secret-token"));
        assert_eq!(result.pkgrom_erase, Some(1));
        assert_eq!(result.code_message.as_deref(), Some("success"));
    }

    #[test]
    fn parses_case_insensitive_nested_validation_token() {
        let result = parse_validation_response(
            r#"{"data":{"pkgrom":{"validate":{"token":"nested-token"},"erase":"1"}},"code":{"MESSAGE":"success"}}"#,
        )
        .unwrap();
        assert_eq!(result.validate_token.as_deref(), Some("nested-token"));
        assert_eq!(result.pkgrom_erase, Some(1));
        assert_eq!(result.code_message.as_deref(), Some("success"));
    }

    #[test]
    fn preserves_allowed_rom_array() {
        let result =
            parse_validation_response(r#"{"PkgRom":{"Validate":["one.zip","two.zip"],"Erase":0}}"#)
                .unwrap();
        assert_eq!(
            result.pkgrom_validate,
            Some(vec!["one.zip".to_owned(), "two.zip".to_owned()])
        );
        assert_eq!(result.validate_token, None);
    }

    #[test]
    fn diagnostic_json_redacts_all_scalar_values() {
        let result = parse_validation_response(
            r#"{"PkgRom":{"Validate":"secret-token","Erase":7,"Wipe":true},"Code":{"message":"success"}}"#,
        )
        .unwrap();
        let diagnostic = redacted_response_json(&result).unwrap();
        assert!(diagnostic.contains("PkgRom"));
        assert!(diagnostic.contains("Validate"));
        assert!(!diagnostic.contains("secret-token"));
        assert!(!diagnostic.contains("success"));
        assert!(!diagnostic.contains("string:"));
        assert!(!diagnostic.contains("\": 7"));
        assert!(!diagnostic.contains("true"));
        let redacted: Value = serde_json::from_str(&diagnostic).unwrap();
        assert!(redacted["PkgRom"]["Validate"].is_string());
        assert!(redacted["PkgRom"]["Erase"].is_number());
        assert!(redacted["PkgRom"]["Wipe"].is_boolean());
    }

    #[test]
    fn unfamiliar_json_remains_available_for_redacted_diagnostics() {
        let result = parse_validation_response(r#"{"Unexpected":{"Value":"private"}}"#).unwrap();
        assert_eq!(result.validate_token, None);
        let diagnostic = redacted_response_json(&result).unwrap();
        assert!(diagnostic.contains("Unexpected"));
        assert!(diagnostic.contains("Value"));
        assert!(!diagnostic.contains("private"));
    }
}
