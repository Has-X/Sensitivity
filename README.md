Sensitivity: Mi Assistant CLI
=============================

[![License: AGPL v3](https://img.shields.io/badge/License-AGPL_v3-blue.svg)](./LICENSE)

⚠️ **Notice:** Sensitivity is licensed under the GNU Affero General Public License v3.0 (AGPL-3.0).  
This means it **cannot** be integrated into closed-source or paid “pro tools” (e.g. Hydra, TFM, DFT, etc.).  
Any redistribution or modification must remain open source under the AGPL.  
See [License & Usage Notice](#license--usage-notice) for details.

---

Sensitivity is a modern, Rust-based reimplementation of Xiaomi’s Mi Assistant flashing flow. It speaks the ADB protocol directly over USB (no adb.exe required), validates Recovery ROMs against Xiaomi’s `miotaV3` endpoint, and sideloads with robust progress, error reporting, and cross-region handling.

Highlights
- Pure Rust, production-ready, no external ADB needed
- Direct USB via `rusb` (Mi Assistant interface 0xff/0x42/1)
- Minimal ADB protocol (CONNECT/OPEN/WRTE/OKAY/CLSE)
- Xiaomi commands: `getdevice`, `getsn`, `getversion`, `getcodebase`, `getbranch`, `getlanguage`, `getregion`, `getromzone`, `format-data`, `reboot`
- Validation client for `miotaV3` (AES-128-CBC + Base64 framing)
- Sideload with correct wipe flag negotiation and proper end-of-transfer handling

Safety First
- Stock recovery constraints apply: avoid downgrades on locked bootloaders
- Cross-region flashes may require a data wipe; the tool will honor server guidance or a `--wipe` override
- FRP remains enforced by the device; flashing does not bypass Google account lock

Build From Source
- Prerequisites: Rust (stable, 1.70+)
- Linux:
  - `cargo build --release` (libusb is built vendored automatically)
- Windows:
  - Install a WinUSB driver for the Mi Assistant interface using Zadig (class 0xff / subclass 0x42 / protocol 1)
  - `cargo build --release` (vendored libusb is compiled automatically)

CI Builds
- GitHub Actions builds are provided for Linux and Windows (release profile) and upload artifacts.

Install (Artifacts)
- Download the latest artifact from GitHub Actions (Linux: `miassistant-linux-x86_64`, Windows: `miassistant-windows-x86_64.exe`) and place it in your PATH.

Quick Start
1) Put device into stock recovery and select “Connect with Mi Assistant”.
2) Connect via USB.
3) Read info:
   `miassistant read-info`
4) Validate and flash a ROM with automatic token fetching:
   `miassistant flash "/path/to/rom.zip" --profile global --codename garnet --yes`

Important Flags
- `--profile <region>` and `--codename <device>`: build the device identity for cross-region (e.g., `--profile global --codename garnet`)
- `--wipe`: allow/force a data wipe when flashing (sets the final `:1` in `sideload-host`)
- `--token <string>`: provide validation token manually (advanced; pair with `--wipe` if needed)
- `--chunk-size <bytes>`: default 65536 (64 KiB)
- `--verbose` / `-v`: more logs (`-vv` for debug)
- `--dump-json`: print decrypted validation JSON for inspection

Examples
- List allowed ROMs for current device (after applying a profile):
  `miassistant list-allowed-roms --profile global --codename garnet --dump-json`
- Flash with server token and wipe if required:
  `miassistant flash "/path/to/rom.zip" --yes`
- Flash with manual token and forced wipe:
  `miassistant flash "/path/to/rom.zip" --token <token> --wipe --yes`
- Download LatestRom from server and flash:
  `miassistant flash-from-latest --profile global --codename garnet --yes`

Environment (Advanced)
- `SENSITIVITY_AES_KEY` / `SENSITIVITY_AES_IV`: 32-hex strings to override AES-128-CBC key/iv used for `miotaV3` framing. Defaults mimic the original client.

Troubleshooting
- Handshake failed / Not detected:
  - Ensure recovery is in “Connect with Mi Assistant” (not ADB sideload)
  - On Windows, stop `adb` server or run with exclusive mode; install WinUSB driver
  - Reconnect USB cable, try another port
- “Installation aborted”:
  - Mismatched token vs ROM or missing wipe flag; re-validate and let the tool fetch the token, or use `--wipe` with manual `--token`
  - Downgrade attempts on a locked bootloader will be refused by recovery
- EEA/Global cross-flash:
  - Use `--profile global --codename <device>` and let the tool validate; be prepared for a wipe

Project Name
- This project is Sensitivity. Any legacy references to “(HasX)” in older materials should be treated as developer attribution only; the tool itself is branded as Sensitivity.

---

## License & Usage Notice

This project is licensed under the **GNU Affero General Public License v3.0 (AGPL-3.0)**.  

- ✅ Free to use, study, modify, and share.  
- ✅ Redistribution allowed only if the source code remains under the AGPL.  
- ❌ **Not permitted**: integration into closed-source or paid “pro tools” (e.g. Hydra, TFM, DFT, etc.).  
- ❌ **Not permitted**: repackaging Sensitivity as proprietary software.  

Sensitivity is intended for **learning, research, and legitimate device recovery only**.  
No Xiaomi proprietary components are included.
