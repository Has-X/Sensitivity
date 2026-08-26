// Copyright (C) 2026 Chromatic
// Licensed under the GNU AGPL v3.0. See LICENSE file for details.
// Website: https://chromatic.hu

use anyhow::{bail, Context, Result};
use rusb::{DeviceHandle, UsbContext};
use std::time::Duration;

pub struct UsbTransport {
    handle: DeviceHandle<rusb::Context>,
    ep_in: u8,
    ep_out: u8,
    timeout: Duration,
    pub debug_usb: bool,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct UsbDeviceInfo {
    pub index: usize,
    pub transport: String,
    pub transport_id: Option<u64>,
    pub bus: u8,
    pub address: u8,
    pub vendor_id: u16,
    pub product_id: u16,
    pub interface: u8,
    pub protocol: u8,
    pub endpoint_in: u8,
    pub endpoint_out: u8,
    pub recovery_device: Option<String>,
    pub model: Option<String>,
}

struct UsbCandidate {
    device: rusb::Device<rusb::Context>,
    info: UsbDeviceInfo,
}

fn usb_context() -> Result<rusb::Context> {
    rusb::Context::new().map_err(|error| {
        anyhow::anyhow!(
            "libusb initialization failed: {error}. Check that USB support is installed"
        )
    })
}

fn discover_candidates(context: &rusb::Context) -> Result<Vec<UsbCandidate>> {
    let mut exact_matches = Vec::new();
    let mut compatible_matches = Vec::new();
    for device in context.devices().context("Listing USB devices")?.iter() {
        // Use active configuration only to preserve the working miasst.c behavior.
        let config = match device.active_config_descriptor() {
            Ok(configuration) => configuration,
            Err(_) => continue,
        };
        let descriptor = device.device_descriptor().ok();
        for interface in config.interfaces() {
            for setting in interface.descriptors() {
                if setting.class_code() != 0xff || setting.sub_class_code() != 0x42 {
                    continue;
                }
                let mut endpoint_in = None;
                let mut endpoint_out = None;
                for endpoint in setting.endpoint_descriptors() {
                    if endpoint.transfer_type() != rusb::TransferType::Bulk {
                        continue;
                    }
                    if endpoint.address() & 0x80 != 0 {
                        endpoint_in = Some(endpoint.address());
                    } else {
                        endpoint_out = Some(endpoint.address());
                    }
                }
                if let (Some(endpoint_in), Some(endpoint_out)) = (endpoint_in, endpoint_out) {
                    let candidate = UsbCandidate {
                        device: device.clone(),
                        info: UsbDeviceInfo {
                            index: 0,
                            transport: "usb".to_owned(),
                            transport_id: None,
                            bus: device.bus_number(),
                            address: device.address(),
                            vendor_id: descriptor.as_ref().map_or(0, |value| value.vendor_id()),
                            product_id: descriptor.as_ref().map_or(0, |value| value.product_id()),
                            interface: setting.interface_number(),
                            protocol: setting.protocol_code(),
                            endpoint_in,
                            endpoint_out,
                            recovery_device: None,
                            model: None,
                        },
                    };
                    if setting.protocol_code() == 0x01 {
                        exact_matches.push(candidate);
                    } else {
                        compatible_matches.push(candidate);
                    }
                }
            }
        }
    }
    // Stock recoveries normally expose protocol 1. Some releases report a
    // different protocol byte despite speaking the same transport, so only
    // fall back to those interfaces when no exact match exists.
    let mut matches = if exact_matches.is_empty() {
        compatible_matches
    } else {
        exact_matches
    };
    for (index, candidate) in matches.iter_mut().enumerate() {
        candidate.info.index = index;
    }
    Ok(matches)
}

impl UsbTransport {
    pub fn discover() -> Result<Vec<UsbDeviceInfo>> {
        let context = usb_context()?;
        Ok(discover_candidates(&context)?
            .into_iter()
            .map(|candidate| candidate.info)
            .collect())
    }

    pub fn open(device_index: usize, debug_usb: bool) -> Result<Self> {
        let context = usb_context()?;
        let mut matches = discover_candidates(&context)?;

        if matches.is_empty() {
            bail!("No Mi Assistant ADB interface found (class 0xff, subclass 0x42)");
        }
        if device_index >= matches.len() {
            bail!(
                "Device index {} out of range ({} found)",
                device_index,
                matches.len()
            );
        }

        let candidate = matches.remove(device_index);
        let handle = candidate.device.open().context("Opening USB device")?;

        #[cfg(any(target_os = "linux", target_os = "android"))]
        {
            handle.set_auto_detach_kernel_driver(true).ok();
        }
        handle
            .claim_interface(candidate.info.interface)
            .with_context(|| format!("Claiming interface {}", candidate.info.interface))?;
        Ok(UsbTransport {
            handle,
            ep_in: candidate.info.endpoint_in,
            ep_out: candidate.info.endpoint_out,
            timeout: Duration::from_millis(5000),
            debug_usb,
        })
    }

    pub fn set_timeout(&mut self, dur: Duration) {
        self.timeout = dur;
    }

    pub fn write_all(&mut self, data: &[u8]) -> Result<()> {
        let mut written = 0;
        while written < data.len() {
            let n = self
                .handle
                .write_bulk(self.ep_out, &data[written..], self.timeout)
                .context("USB bulk write failed")?;
            if n == 0 {
                bail!("USB bulk write returned 0 bytes (stall or timeout)");
            }
            if self.debug_usb {
                eprintln!("usb out: {} bytes", n);
            }
            written += n;
        }
        Ok(())
    }

    pub fn read_exact(&mut self, buf: &mut [u8]) -> Result<()> {
        let mut read = 0;
        while read < buf.len() {
            let n = self
                .handle
                .read_bulk(self.ep_in, &mut buf[read..], self.timeout)
                .context("USB bulk read failed")?;
            if n == 0 {
                bail!("USB bulk read returned 0 bytes (stall or timeout)");
            }
            if self.debug_usb {
                eprintln!("usb in: {} bytes", n);
            }
            read += n;
        }
        Ok(())
    }
}
