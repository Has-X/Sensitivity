// Copyright (C) 2026 HasX
// Licensed under the GNU AGPL v3.0. See LICENSE file for details.
// Website: https://hasx.dev

use anyhow::{bail, Context, Result};
use rusb::{DeviceHandle, UsbContext};
use std::time::Duration;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MiAssistantDevice {
    pub index: usize,
    pub vendor_id: u16,
    pub product_id: u16,
    pub bus_number: u8,
    pub address: u8,
    /// Stable topology path below the USB root hub when the platform exposes it.
    pub port_numbers: Vec<u8>,
    pub interface_number: u8,
    pub ep_in: u8,
    pub ep_out: u8,
}

impl MiAssistantDevice {
    pub fn label(&self) -> String {
        let ports = if self.port_numbers.is_empty() {
            "unknown".to_owned()
        } else {
            self.port_numbers
                .iter()
                .map(u8::to_string)
                .collect::<Vec<_>>()
                .join("-")
        };
        format!(
            "#{index} · USB {vendor:04x}:{product:04x} · ports {ports} · bus {bus}, address {address} · interface {interface}",
            index = self.index,
            vendor = self.vendor_id,
            product = self.product_id,
            ports = ports,
            bus = self.bus_number,
            address = self.address,
            interface = self.interface_number,
        )
    }
}

pub struct UsbTransport {
    handle: DeviceHandle<rusb::Context>,
    ep_in: u8,
    ep_out: u8,
    timeout: Duration,
    pub debug_usb: bool,
}

type MiAssistantMatch = (rusb::Device<rusb::Context>, u8, u8, u8);

impl UsbTransport {
    pub fn list_mi_assistant_devices() -> Result<Vec<MiAssistantDevice>> {
        let ctx = usb_context()?;
        let devices = mi_assistant_devices(&ctx)?;
        devices
            .iter()
            .enumerate()
            .map(|(index, (device, interface_number, ep_in, ep_out))| {
                let descriptor = device.device_descriptor().with_context(|| {
                    format!(
                        "Reading USB descriptor for bus {}, address {}",
                        device.bus_number(),
                        device.address()
                    )
                })?;
                Ok(MiAssistantDevice {
                    index,
                    vendor_id: descriptor.vendor_id(),
                    product_id: descriptor.product_id(),
                    bus_number: device.bus_number(),
                    address: device.address(),
                    port_numbers: device.port_numbers().unwrap_or_default(),
                    interface_number: *interface_number,
                    ep_in: *ep_in,
                    ep_out: *ep_out,
                })
            })
            .collect()
    }

    pub fn open(device_index: usize, debug_usb: bool) -> Result<Self> {
        let ctx = usb_context()?;
        let mut matches = mi_assistant_devices(&ctx)?;

        if matches.is_empty() {
            bail!("No Mi Assistant ADB interface found (class 0xff, subclass 0x42, protocol 1)");
        }
        if device_index >= matches.len() {
            bail!(
                "Device index {} out of range ({} found)",
                device_index,
                matches.len()
            );
        }

        let (device, interface_number, ep_in, ep_out) = matches.remove(device_index);
        let handle = device.open().context("Opening USB device")?;

        #[cfg(any(target_os = "linux", target_os = "android"))]
        {
            handle.set_auto_detach_kernel_driver(true).ok();
        }
        handle
            .claim_interface(interface_number)
            .with_context(|| format!("Claiming interface {}", interface_number))?;
        Ok(UsbTransport {
            handle,
            ep_in,
            ep_out,
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

fn usb_context() -> Result<rusb::Context> {
    rusb::Context::new().map_err(|e| {
        let msg = format!("libusb initialization failed: {}", e);
        #[cfg(windows)]
        {
            eprintln!("{}\nOn Windows, ensure libusb-1.0.dll is installed and the device uses WinUSB (Zadig).", msg);
        }
        anyhow::anyhow!(msg)
    })
}

fn mi_assistant_devices(ctx: &rusb::Context) -> Result<Vec<MiAssistantMatch>> {
    let mut matches = Vec::new();
    for device in ctx.devices().context("Listing USB devices")?.iter() {
        // Use active configuration only (mirrors miasst.c)
        let config = match device.active_config_descriptor() {
            Ok(c) => c,
            Err(_) => continue,
        };
        for iface in config.interfaces() {
            for setting in iface.descriptors() {
                if setting.class_code() == 0xff
                    && setting.sub_class_code() == 0x42
                    && setting.protocol_code() == 0x01
                {
                    let mut ep_in = None;
                    let mut ep_out = None;
                    for ep in setting.endpoint_descriptors() {
                        let addr = ep.address();
                        if ep.transfer_type() == rusb::TransferType::Bulk {
                            if addr & 0x80 != 0 {
                                ep_in = Some(addr);
                            } else {
                                ep_out = Some(addr);
                            }
                        }
                    }
                    if let (Some(_in), Some(_out)) = (ep_in, ep_out) {
                        matches.push((device.clone(), setting.interface_number(), _in, _out));
                    }
                }
            }
        }
    }
    // libusb's enumeration order is not stable across an ADB restart. USB port
    // topology is stable while the phone remains connected, so use it as the
    // primary order and only then fall back to transient bus/address values.
    matches.sort_by(|(left, ..), (right, ..)| {
        left.port_numbers()
            .unwrap_or_default()
            .cmp(&right.port_numbers().unwrap_or_default())
            .then_with(|| left.bus_number().cmp(&right.bus_number()))
            .then_with(|| left.address().cmp(&right.address()))
    });

    Ok(matches)
}
