// Copyright (C) 2025 HasX
// Licensed under the GNU AGPL v3.0. See LICENSE file for details.
// Website: https://hasx.dev

use anyhow::Result;

use crate::mi::DeviceInfo;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegionProfile {
    Global,
    Eea,
    In,
    Ru,
    Id,
    Tr,
    Tw,
    Cn,
}

impl RegionProfile {
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "global" | "mi" => Some(Self::Global),
            "eea" | "eu" => Some(Self::Eea),
            "in" | "india" => Some(Self::In),
            "ru" | "russia" => Some(Self::Ru),
            "id" | "indo" | "indonesia" => Some(Self::Id),
            "tr" | "turkey" => Some(Self::Tr),
            "tw" | "taiwan" => Some(Self::Tw),
            "cn" | "china" => Some(Self::Cn),
            _ => None,
        }
    }

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

pub fn apply_profile(info: &DeviceInfo, profile: RegionProfile, codename_override: Option<&str>, keep_codebase: bool) -> Result<DeviceInfo> {
    let codename = codename_override.map(|s| s.to_string()).unwrap_or_else(|| derive_codename(&info.device));
    let device = profile.device_name(&codename);
    let version = replace_version_region_suffix(&info.version, profile.version_suffix());
    let mut out = info.clone();
    out.device = device;
    out.version = version;
    out.branch = "F".to_string();
    if !keep_codebase {
        out.codebase = info.codebase.clone(); // keep by default; explicit override controls codebase
    }
    Ok(out)
}

