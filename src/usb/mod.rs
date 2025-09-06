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

impl UsbTransport {
    pub fn open(device_index: usize, debug_usb: bool) -> Result<Self> {
        let ctx = rusb::Context::new().map_err(|e| {
            let msg = format!("libusb initialization failed: {}", e);
            #[cfg(windows)]
            {
                eprintln!("{}\nOn Windows, ensure libusb-1.0.dll is installed and the device uses WinUSB (Zadig).", msg);
            }
            anyhow::anyhow!(msg)
        })?;

        let mut matches: Vec<(rusb::Device<rusb::Context>, u8, u8, u8)> = Vec::new();
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
                                if addr & 0x80 != 0 { ep_in = Some(addr); } else { ep_out = Some(addr); }
                            }
                        }
                        if let (Some(_in), Some(_out)) = (ep_in, ep_out) {
                            matches.push((device.clone(), setting.interface_number(), _in, _out));
                        }
                    }
                }
            }
        }

        if matches.is_empty() {
            bail!("No Mi Assistant ADB interface found (class 0xff, subclass 0x42, protocol 1)");
        }
        if device_index >= matches.len() {
            bail!("Device index {} out of range ({} found)", device_index, matches.len());
        }

        let (device, interface_number, ep_in, ep_out) = matches.remove(device_index);
        let mut handle = device.open().context("Opening USB device")?;

        #[cfg(any(target_os = "linux", target_os = "android"))]
        {
            handle.set_auto_detach_kernel_driver(true).ok();
        }
        handle
            .claim_interface(interface_number)
            .with_context(|| format!("Claiming interface {}", interface_number))?;
        Ok(UsbTransport { handle, ep_in, ep_out, timeout: Duration::from_millis(5000), debug_usb })
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
