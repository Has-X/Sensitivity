// Copyright (C) 2025 HasX
// Licensed under the GNU AGPL v3.0. See LICENSE file for details.
// Website: https://hasx.dev

use anyhow::{Context, Result};

use crate::adb::{connect, AdbConnection};
use crate::usb::UsbTransport;
pub mod profile;

#[derive(Debug, Clone)]
pub struct DeviceInfo {
    pub device: String,
    pub sn: String,
    pub version: String,
    pub codebase: String,
    pub branch: String,
    pub language: String,
    pub region: String,
    pub romzone: String,
}

pub struct MiClient {
    adb: AdbConnection,
}

impl MiClient {
    pub fn new(usb: UsbTransport) -> Result<Self> {
        let adb = connect(usb).context("ADB CONNECT handshake failed")?;
        Ok(Self { adb })
    }

    pub fn read_all_info(&mut self) -> Result<DeviceInfo> {
        let device = self.simple_query("getdevice:")?;
        let sn = self.simple_query("getsn:")?;
        let version = self.simple_query("getversion:")?;
        let codebase = self.simple_query("getcodebase:")?;
        let branch = self.simple_query("getbranch:")?;
        let language = self.simple_query("getlanguage:")?;
        let region = self.simple_query("getregion:")?;
        let romzone = self.simple_query("getromzone:")?;
        Ok(DeviceInfo { device, sn, version, codebase, branch, language, region, romzone })
    }

    pub fn simple_query(&mut self, cmd: &str) -> Result<String> {
        let text = self.adb.query_text(cmd).with_context(|| format!("query_text {}", cmd))?;
        Ok(text)
    }

    pub fn simple_command(&mut self, cmd: &str) -> Result<()> {
        let mut s = self.adb.open_service(cmd)?;
        let _ = s.read_to_end();
        Ok(())
    }

    pub fn open_service(&mut self, name: &str) -> Result<crate::adb::AdbStream<'_>> {
        self.adb.open_service(name)
    }

    pub fn open_sideload(&mut self, name: &str) -> Result<(crate::adb::AdbStream<'_>, Option<crate::adb::AdbPacket>)> {
        self.adb.open_sideload(name)
    }
}
