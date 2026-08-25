// Copyright (C) 2026 Chromatic
// Licensed under the GNU AGPL v3.0. See LICENSE file for details.
// Website: https://chromatic.hu

use anyhow::{Context, Result};

use crate::adb::{connect, AdbConnection};
use crate::usb::UsbTransport;
use crate::util::adb_server::{AdbServerSideloadStream, AdbServerTransport};
pub mod profile;

#[derive(Debug, Clone, serde::Serialize)]
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
    backend: MiBackend,
}

enum MiBackend {
    Direct(AdbConnection),
    Server(AdbServerTransport),
}

pub enum OpenedSideload<'a> {
    Direct {
        stream: crate::adb::AdbStream<'a>,
        pending: Option<crate::adb::AdbPacket>,
    },
    Server(AdbServerSideloadStream),
}

impl MiClient {
    pub fn new(usb: UsbTransport) -> Result<Self> {
        let adb = connect(usb).context("ADB CONNECT handshake failed")?;
        Ok(Self {
            backend: MiBackend::Direct(adb),
        })
    }

    pub fn from_adb_server(transport_id: u64) -> Self {
        Self {
            backend: MiBackend::Server(AdbServerTransport::new(transport_id)),
        }
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
        Ok(DeviceInfo {
            device,
            sn,
            version,
            codebase,
            branch,
            language,
            region,
            romzone,
        })
    }

    pub fn simple_query(&mut self, cmd: &str) -> Result<String> {
        let text = match &mut self.backend {
            MiBackend::Direct(adb) => adb.query_text(cmd),
            MiBackend::Server(adb) => adb.query_text(cmd),
        }
        .with_context(|| format!("query_text {cmd}"))?;
        Ok(text)
    }

    pub fn simple_command(&mut self, cmd: &str) -> Result<()> {
        match &mut self.backend {
            MiBackend::Direct(adb) => {
                let mut stream = adb.open_service(cmd)?;
                // Reboot and format-data can deliberately drop USB before CLSE. A
                // successfully opened service is the recovery's acknowledgement.
                let _ = stream.read_to_end();
            }
            MiBackend::Server(adb) => adb.command(cmd)?,
        }
        Ok(())
    }

    pub fn open_sideload(&mut self, name: &str) -> Result<OpenedSideload<'_>> {
        match &mut self.backend {
            MiBackend::Direct(adb) => {
                let (stream, pending) = adb.open_sideload(name)?;
                Ok(OpenedSideload::Direct { stream, pending })
            }
            MiBackend::Server(adb) => Ok(OpenedSideload::Server(adb.open_sideload(name)?)),
        }
    }
}
