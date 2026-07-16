// Copyright (C) 2025 HasX
// Licensed under the GNU AGPL v3.0. See LICENSE file for details.
// Website: https://hasx.dev

use anyhow::{bail, Result};

use crate::mi::DeviceInfo;

#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum RegionProfile {
    #[value(alias = "mi")]
    Global,
    #[value(alias = "eu")]
    Eea,
    #[value(alias = "india")]
    In,
    #[value(alias = "russia")]
    Ru,
    #[value(alias = "indo", alias = "indonesia")]
    Id,
    #[value(alias = "turkey")]
    Tr,
    #[value(alias = "taiwan")]
    Tw,
    #[value(alias = "china")]
    Cn,
}

impl RegionProfile {
    fn device_name(&self, codename: &str) -> String {
        match self {
            RegionProfile::Global => format!("{}_global", codename),
            RegionProfile::Eea => format!("{}_eea_global", codename),
            RegionProfile::In => format!("{}_in_global", codename),
            RegionProfile::Ru => format!("{}_ru_global", codename),
            RegionProfile::Id => format!("{}_id_global", codename),
            RegionProfile::Tr => format!("{}_tr_global", codename),
            RegionProfile::Tw => format!("{}_tw_global", codename),
            RegionProfile::Cn => codename.to_string(),
        }
    }

    fn version_suffix(&self) -> &'static str {
        match self {
            RegionProfile::Global => "MIXM",
            RegionProfile::Eea => "EUXM",
            RegionProfile::In => "INXM",
            RegionProfile::Ru => "RUXM",
            RegionProfile::Id => "IDXM",
            RegionProfile::Tr => "TRXM",
            RegionProfile::Tw => "TWXM",
            RegionProfile::Cn => "CNXM",
        }
    }
}

fn derive_codename(device: &str) -> String {
    // e.g., garnet_in_global -> garnet; garnet_global -> garnet; garnet -> garnet
    device.split('_').next().unwrap_or(device).to_string()
}

fn replace_version_region_suffix(version: &str, new_suffix: &str) -> String {
    // Expect version like OS2.0.202.0.VNRINXM. Replace last 4 letters with new_suffix.
    if let Some(dot) = version.rfind('.') {
        let (head, tail) = version.split_at(dot + 1);
        // tail like VNRINXM
        if tail.len() >= 4 {
            let prefix = &tail[..tail.len().saturating_sub(4)];
            return format!("{}{}{}", head, prefix, new_suffix);
        }
    }
    version.to_string()
}

pub fn apply_profile(
    info: &DeviceInfo,
    profile: RegionProfile,
    codename_override: Option<&str>,
) -> Result<DeviceInfo> {
    let codename = codename_override
        .map(|s| s.to_string())
        .unwrap_or_else(|| derive_codename(&info.device));
    if codename.is_empty()
        || !codename.chars().all(|character| {
            character.is_ascii_alphanumeric() || character == '_' || character == '-'
        })
    {
        bail!("invalid device codename: {codename:?}");
    }
    let device = profile.device_name(&codename);
    let version = replace_version_region_suffix(&info.version, profile.version_suffix());
    let mut out = info.clone();
    out.device = device;
    out.version = version;
    out.branch = "F".to_string();
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn device_info() -> DeviceInfo {
        DeviceInfo {
            device: "garnet_in_global".into(),
            sn: "serial".into(),
            version: "OS2.0.202.0.VNRINXM".into(),
            codebase: "15.0".into(),
            branch: "original".into(),
            language: "en".into(),
            region: "IN".into(),
            romzone: "1".into(),
        }
    }

    #[test]
    fn global_profile_changes_only_expected_identity_fields() {
        let original = device_info();
        let changed = apply_profile(&original, RegionProfile::Global, None).unwrap();

        assert_eq!(changed.device, "garnet_global");
        assert_eq!(changed.version, "OS2.0.202.0.VNRMIXM");
        assert_eq!(changed.branch, "F");
        assert_eq!(changed.sn, original.sn);
        assert_eq!(changed.codebase, original.codebase);
        assert_eq!(changed.romzone, original.romzone);
    }

    #[test]
    fn explicit_codename_is_used_for_region_profile() {
        let changed = apply_profile(&device_info(), RegionProfile::Eea, Some("ruby")).unwrap();
        assert_eq!(changed.device, "ruby_eea_global");
        assert!(changed.version.ends_with("EUXM"));
    }

    #[test]
    fn unsafe_or_empty_codename_is_rejected() {
        assert!(apply_profile(&device_info(), RegionProfile::Global, Some("")).is_err());
        assert!(apply_profile(&device_info(), RegionProfile::Global, Some("bad value")).is_err());
    }
}
