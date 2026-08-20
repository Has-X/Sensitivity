// Copyright (C) 2026 HasX
// Licensed under the GNU AGPL v3.0. See LICENSE file for details.
// Website: https://hasx.dev

use std::path::PathBuf;

use anyhow::{bail, Context, Result};
use clap::{ArgAction, Parser, Subcommand};

use sensitivity::download;
use sensitivity::mi::profile::{apply_profile, RegionProfile};
use sensitivity::mi::MiClient;
use sensitivity::sideload::sideload_zip;
use sensitivity::usb::UsbTransport;
use sensitivity::util;
use sensitivity::util::config;
use sensitivity::util::logging::{init_logger, LogVerbosity};
use sensitivity::validate;

#[derive(Debug, Parser)]
#[command(
    name = "sensitivity",
    version,
    about = "Sensitivity — Xiaomi Recovery ROM validator and USB sideloader.\n\nPut the device in Recovery → Connect with Mi Assistant, then run a subcommand."
)]
struct Cli {
    /// Which device to use when multiple phones are connected (default: 0, the first one)
    #[arg(long, default_value_t = 0, global = true)]
    device_index: usize,

    /// USB transfer chunk size in bytes — reduce if transfers stall (default: 65536)
    #[arg(long, default_value_t = 65536, global = true)]
    chunk_size: usize,

    /// Xiaomi validation server URL (change only if you are self-hosting)
    #[arg(
        long,
        default_value = "https://update.miui.com/updates/miotaV3.php",
        global = true
    )]
    server_url: String,

    /// Accept an HTTP (non-HTTPS) server URL — insecure, avoid in production
    #[arg(long, action = ArgAction::SetTrue, global = true)]
    http: bool,

    /// Log raw USB packet directions and sizes (developer tool)
    #[arg(long, action = ArgAction::SetTrue, global = true)]
    debug_usb: bool,

    /// Stop any running ADB server before connecting
    #[arg(long, action = ArgAction::SetTrue, global = true)]
    kill_adb_server: bool,

    /// Do not retry automatically if the first connection attempt fails
    #[arg(long, action = ArgAction::SetTrue, global = true)]
    no_auto_kill: bool,

    /// Show more detail in the output (-v = verbose, -vv = debug)
    #[arg(long, short = 'v', action = ArgAction::Count, global = true)]
    verbose: u8,

    /// Print the raw decrypted JSON from Xiaomi's server response
    #[arg(long, action = ArgAction::SetTrue, global = true)]
    dump_json: bool,

    /// Stop the ADB server when the command finishes
    #[arg(long, action = ArgAction::SetTrue, global = true)]
    kill_adb_after: bool,

    /// Let a local ADB server keep running alongside Sensitivity (Windows only — may cause conflicts)
    #[arg(long, action = ArgAction::SetTrue, global = true)]
    allow_adb: bool,

    /// Override the device identifier sent to Xiaomi (advanced — use with care)
    #[arg(long, global = true)]
    override_device: Option<String>,
    /// Override the OS version sent to Xiaomi (advanced)
    #[arg(long, global = true)]
    override_version: Option<String>,
    /// Override the serial number sent to Xiaomi (advanced)
    #[arg(long, global = true)]
    override_sn: Option<String>,
    /// Override the codebase sent to Xiaomi (advanced)
    #[arg(long, global = true)]
    override_codebase: Option<String>,
    /// Override the branch sent to Xiaomi (advanced)
    #[arg(long, global = true)]
    override_branch: Option<String>,
    /// Override the ROM zone sent to Xiaomi (advanced)
    #[arg(long, global = true)]
    override_romzone: Option<String>,

    /// Region to validate against: global, eea, in, ru, id, tr, tw, cn
    #[arg(long, global = true)]
    profile: Option<String>,
    /// Device codename to use with --profile when it cannot be derived automatically (e.g. garnet)
    #[arg(long, global = true)]
    codename: Option<String>,

    /// Use this MD5 for validation instead of computing one from the file (advanced)
    #[arg(long, global = true)]
    md5: Option<String>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// List phones exposing a Mi Assistant Recovery USB interface without opening them
    Devices,
    /// Exercise the safe offline CLI flow without USB, network traffic, ROM files, or flashing
    Demo,
    /// Read and print device information from the connected phone
    ReadInfo,
    /// Ask Xiaomi which ROMs are allowed for this device
    ListAllowedRoms,
    /// Validate a local Recovery ROM zip and sideload it to the device
    Flash {
        path: PathBuf,
        /// Skip the data-wipe confirmation prompt
        #[arg(long)]
        yes: bool,
        /// Use a pre-obtained validation token instead of asking Xiaomi
        #[arg(long)]
        token: Option<String>,
        /// Allow data wipe during flashing (required for some cross-region ROMs)
        #[arg(long, action = ArgAction::SetTrue)]
        wipe: bool,
    },
    /// Wipe user data and reboot (format-data)
    FormatData,
    /// Reboot the device
    Reboot,
    /// Download the latest Recovery ROM Xiaomi recommends for this device
    DownloadLatest {
        /// Folder to save the ROM into (default: current directory)
        #[arg(long)]
        output_dir: Option<PathBuf>,
    },
    /// Download the latest ROM from Xiaomi and flash it in one step
    FlashFromLatest {
        /// Folder to save the ROM into before flashing (default: current directory)
        #[arg(long)]
        output_dir: Option<PathBuf>,
        /// Skip the data-wipe confirmation prompt
        #[arg(long)]
        yes: bool,
        /// Allow data wipe during flashing
        #[arg(long, action = ArgAction::SetTrue)]
        wipe: bool,
    },
    /// Save an MD5 override so future validations skip file hashing
    SetHash { md5: String },
    /// Remove the saved MD5 override
    ClearHash,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    // Load persisted state (MD5 override)
    let mut state = config::load_state();
    init_logger(match cli.verbose {
        0 => LogVerbosity::Normal,
        1 => LogVerbosity::Verbose,
        _ => LogVerbosity::Debug,
    });

    if !cli.server_url.starts_with("https://") && !cli.http {
        bail!(
            "Refusing to connect to a non-HTTPS server. Use --http to override (insecure). URL: {}",
            cli.server_url
        );
    }
    if cli.http && cli.server_url.starts_with("http://") {
        eprintln!(
            "WARNING: Using an insecure HTTP endpoint for validation: {}",
            cli.server_url
        );
    }

    match &cli.command {
        Commands::Devices => {
            let devices = UsbTransport::list_mi_assistant_devices()
                .context("Listing Mi Assistant Recovery USB interfaces")?;
            if devices.is_empty() {
                println!("No Mi Assistant Recovery phones found.");
                println!("Put the phone in Recovery, choose Connect with Mi Assistant, then run this command again.");
            } else {
                for device in devices {
                    println!("{}", device.label());
                }
            }
            return Ok(());
        }
        Commands::Demo => {
            println!("Sensitivity CLI demo mode");
            println!("  Device: 2312DRA50G (simulated)");
            println!("  ROM: demo-hyperos-recovery-rom.zip (simulated)");
            println!("  Validation: approved (simulated)");
            println!("  Flash: skipped safely. No USB, network, file, or device operation was performed.");
            return Ok(());
        }
        _ => {}
    }

    // On Windows, default to exclusive mode: kill adb server and block port 5037 unless --allow-adb is provided.
    #[cfg(windows)]
    let mut _adb_block_guard: Option<std::net::TcpListener> = None;
    #[cfg(windows)]
    {
        if !cli.allow_adb {
            // Proactively kill by protocol
            let _ = util::adb_server::kill_adb_server(std::time::Duration::from_millis(500));
            // Fallback: hard kill process
            util::adb_server::kill_adb_process();
            // Try to block the port to prevent respawn
            _adb_block_guard = util::adb_server::block_port_5037();
            if _adb_block_guard.is_some() {
                eprintln!("ADB server stopped — exclusive USB mode active.");
            } else if util::adb_server::is_running(std::time::Duration::from_millis(200)) {
                eprintln!(
                    "Warning: an ADB server is still running on port 5037 and may interfere with USB access."
                );
            }
        } else if !cli.kill_adb_server
            && util::adb_server::is_running(std::time::Duration::from_millis(200))
        {
            eprintln!("Note: an ADB server is running on 127.0.0.1:5037. It may hold the USB device. Use --kill-adb-server to stop it, or omit --allow-adb.");
        }
        if cli.kill_adb_server {
            if let Err(e) = util::adb_server::kill_adb_server(std::time::Duration::from_secs(2)) {
                eprintln!("Warning: could not stop the ADB server: {}", e);
            } else {
                eprintln!("ADB server stopped.");
            }
        }
    }

    // Open USB transport
    let make_client = || -> Result<MiClient> {
        let transport = UsbTransport::open(cli.device_index, cli.debug_usb)
            .context("Opening USB Mi Assistant interface via libusb")?;
        MiClient::new(transport).context("Initializing ADB client")
    };
    // Handle config-only subcommands before touching USB
    match &cli.command {
        Commands::SetHash { md5 } => {
            if md5.len() != 32 || !md5.chars().all(|c| c.is_ascii_hexdigit()) {
                bail!("MD5 must be exactly 32 hex characters.");
            }
            state.override_md5 = Some(md5.to_lowercase());
            config::save_state(&state).context("saving state")?;
            println!("MD5 override saved.");
            return Ok(());
        }
        Commands::ClearHash => {
            state.override_md5 = None;
            config::save_state(&state).context("saving state")?;
            println!("MD5 override cleared.");
            return Ok(());
        }
        _ => {}
    }

    let mut client = match make_client() {
        Ok(c) => c,
        Err(e) => {
            if !cli.no_auto_kill {
                eprintln!("Could not connect to the device. Stopping ADB and retrying…");
                let _ = util::adb_server::kill_adb_server(std::time::Duration::from_millis(500));
                #[cfg(windows)]
                {
                    util::adb_server::kill_adb_process();
                }
                std::thread::sleep(std::time::Duration::from_millis(300));
                #[cfg(windows)]
                {
                    if _adb_block_guard.is_none() {
                        _adb_block_guard = util::adb_server::block_port_5037();
                    }
                }
                make_client().context(e)?
            } else {
                return Err(e);
            }
        }
    };

    match cli.command {
        Commands::ReadInfo => {
            let info = client.read_all_info().context("Fetching device info")?;
            println!("{}", info.device);
            println!("{}", info.version);
            println!("{}", info.sn);
            println!("{}", info.codebase);
            println!("{}", info.branch);
            println!("{}", info.language);
            println!("{}", info.region);
            println!("{}", info.romzone);
        }
        Commands::DownloadLatest { output_dir } => {
            let mut info = client.read_all_info().context("Fetching device info")?;
            if let Some(p) = &cli.profile {
                if let Ok(rp) = p.parse::<RegionProfile>() {
                    info = apply_profile(&info, rp, cli.codename.as_deref(), true)?;
                    eprintln!("Applied profile: {}", p);
                }
            }
            if let Some(v) = &cli.override_device {
                info.device = v.clone();
            }
            if let Some(v) = &cli.override_version {
                info.version = v.clone();
            }
            if let Some(v) = &cli.override_sn {
                info.sn = v.clone();
            }
            if let Some(v) = &cli.override_codebase {
                info.codebase = v.clone();
            }
            if let Some(v) = &cli.override_branch {
                info.branch = v.clone();
            }
            if let Some(v) = &cli.override_romzone {
                info.romzone = v.clone();
            }
            let req_json =
                validate::build_request_json(&info, None).context("Building validation request")?;
            if cli.dump_json {
                if let Ok(q) = validate::encode_request_b64(&req_json) {
                    eprintln!("Request JSON: {}", req_json);
                    eprintln!("q (base64): {}", q);
                }
            }
            let resp = validate::validate(&cli.server_url, &req_json)
                .context("Validation HTTP call failed")?;
            let json = resp
                .full_json
                .clone()
                .ok_or_else(|| anyhow::anyhow!("No full JSON in response"))?;
            let (latest, mirrors) =
                download::parse_latest_from_json(&json).context("Parsing LatestRom from JSON")?;
            let client_http = download::official_download_client()?;
            let out_dir = output_dir.unwrap_or_else(|| std::env::current_dir().unwrap());
            let path = download::download_from_https_mirrors_with_md5(
                &client_http,
                &mirrors,
                &latest,
                &out_dir,
                |_, _| {},
            )
            .context("Downloading LatestRom")?;
            println!("Downloaded to {} (md5 ok)", path.display());
        }
        Commands::FlashFromLatest {
            output_dir,
            yes,
            wipe,
        } => {
            let mut info = client.read_all_info().context("Fetching device info")?;
            if let Some(p) = &cli.profile {
                if let Ok(rp) = p.parse::<RegionProfile>() {
                    info = apply_profile(&info, rp, cli.codename.as_deref(), true)?;
                    eprintln!("Applied profile: {}", p);
                }
            }
            if let Some(v) = &cli.override_device {
                info.device = v.clone();
            }
            if let Some(v) = &cli.override_version {
                info.version = v.clone();
            }
            if let Some(v) = &cli.override_sn {
                info.sn = v.clone();
            }
            if let Some(v) = &cli.override_codebase {
                info.codebase = v.clone();
            }
            if let Some(v) = &cli.override_branch {
                info.branch = v.clone();
            }
            if let Some(v) = &cli.override_romzone {
                info.romzone = v.clone();
            }
            // Step 1: Get LatestRom info
            let req_json =
                validate::build_request_json(&info, None).context("Building validation request")?;
            let resp1 = validate::validate(&cli.server_url, &req_json)
                .context("Validation HTTP call failed")?;
            let json = resp1
                .full_json
                .clone()
                .ok_or_else(|| anyhow::anyhow!("No full JSON in response"))?;
            let (latest, mirrors) =
                download::parse_latest_from_json(&json).context("Parsing LatestRom from JSON")?;
            // Step 2: Download
            let client_http = download::official_download_client()?;
            let out_dir = output_dir.unwrap_or_else(|| std::env::current_dir().unwrap());
            let local_path = download::download_from_https_mirrors_with_md5(
                &client_http,
                &mirrors,
                &latest,
                &out_dir,
                |_, _| {},
            )
            .context("Downloading LatestRom")?;
            // Step 3: Validate for this MD5 and flash
            let req_json2 = validate::build_request_json(&info, Some(latest.md5.clone()))
                .context("Building validation request")?;
            if cli.dump_json {
                if let Ok(q) = validate::encode_request_b64(&req_json2) {
                    eprintln!("Request JSON: {}", req_json2);
                    eprintln!("q (base64): {}", q);
                }
            }
            let resp2 = validate::validate(&cli.server_url, &req_json2)
                .context("Validation HTTP call failed")?;
            if let Some(msg) = resp2.code_message.as_deref() {
                println!("Server message: {}", msg);
            }
            if (resp2.pkgrom_erase == Some(1) || wipe) && !yes {
                println!("⚠  This ROM will erase all data on the device. Press Enter to continue, or Ctrl+C to cancel.");
                let mut s = String::new();
                let _ = std::io::stdin().read_line(&mut s);
            }
            let token = resp2
                .validate_token
                .as_deref()
                .ok_or_else(|| anyhow::anyhow!("Missing Validate token in response"))?
                .to_string();
            if cli.verbose > 0 {
                eprintln!("Using validate token (len {}): {:.8}…", token.len(), token);
            }
            let allow_wipe = resp2.pkgrom_erase == Some(1) || wipe;
            sideload_zip(&mut client, &local_path, cli.chunk_size, &token, allow_wipe)
                .context("Sideload failed")?;
        }
        Commands::SetHash { .. } => {
            // Already handled before USB init
            return Ok(());
        }
        Commands::ClearHash => {
            // Already handled before USB init
            return Ok(());
        }
        Commands::Devices | Commands::Demo => unreachable!("handled before USB setup"),
        Commands::ListAllowedRoms => {
            let mut info = client.read_all_info().context("Fetching device info")?;
            if let Some(p) = &cli.profile {
                if let Ok(rp) = p.parse::<RegionProfile>() {
                    info = apply_profile(&info, rp, cli.codename.as_deref(), true)?;
                    eprintln!("Applied profile: {}", p);
                }
            }
            if let Some(v) = &cli.override_device {
                info.device = v.clone();
            }
            if let Some(v) = &cli.override_version {
                info.version = v.clone();
            }
            if let Some(v) = &cli.override_sn {
                info.sn = v.clone();
            }
            if let Some(v) = &cli.override_codebase {
                info.codebase = v.clone();
            }
            if let Some(v) = &cli.override_branch {
                info.branch = v.clone();
            }
            if let Some(v) = &cli.override_romzone {
                info.romzone = v.clone();
            }
            let req_json =
                validate::build_request_json(&info, None).context("Building validation request")?;
            if cli.dump_json {
                if let Ok(q) = validate::encode_request_b64(&req_json) {
                    eprintln!("Request JSON: {}", req_json);
                    eprintln!("q (base64): {}", q);
                }
            }
            let resp = validate::validate(&cli.server_url, &req_json)
                .context("Validation HTTP call failed")?;
            validate::print_allowed_with_options(&resp, cli.dump_json);
        }
        Commands::Flash {
            path,
            yes,
            token,
            wipe,
        } => {
            if !path.exists() {
                bail!("Zip not found: {}", path.display());
            }
            let mut info = client.read_all_info().context("Fetching device info")?;
            if let Some(p) = &cli.profile {
                if let Ok(rp) = p.parse::<RegionProfile>() {
                    info = apply_profile(&info, rp, cli.codename.as_deref(), true)?;
                    eprintln!("Applied profile: {}", p);
                }
            }
            if let Some(v) = &cli.override_device {
                info.device = v.clone();
            }
            if let Some(v) = &cli.override_version {
                info.version = v.clone();
            }
            if let Some(v) = &cli.override_sn {
                info.sn = v.clone();
            }
            if let Some(v) = &cli.override_codebase {
                info.codebase = v.clone();
            }
            if let Some(v) = &cli.override_branch {
                info.branch = v.clone();
            }
            if let Some(v) = &cli.override_romzone {
                info.romzone = v.clone();
            }
            let computed_md5 = util::md5::md5_file(&path).context("Computing MD5 of zip")?;
            // Determine MD5 to use (CLI > persisted > computed)
            let used_md5 = if let Some(m) = &cli.md5 {
                m.clone()
            } else if let Some(m) = &state.override_md5 {
                m.clone()
            } else {
                computed_md5.clone()
            };
            if used_md5.len() != 32 || !used_md5.chars().all(|c| c.is_ascii_hexdigit()) {
                bail!("Provided MD5 must be 32 hex characters");
            }
            if used_md5.to_lowercase() != computed_md5 {
                eprintln!(
                    "Note: using override MD5 {} instead of the computed {} — make sure this is intentional.",
                    used_md5, computed_md5
                );
            } else if cli.verbose > 0 {
                eprintln!("MD5: {}", used_md5);
            }
            let req_json = validate::build_request_json(&info, Some(used_md5.clone()))
                .context("Building validation request")?;
            if cli.dump_json {
                if let Ok(q) = validate::encode_request_b64(&req_json) {
                    eprintln!("Request JSON: {}", req_json);
                    eprintln!("q (base64): {}", q);
                }
            }
            let mut resp = validate::ValidateResult::default();
            // Preserve whether CLI provided a token before shadowing it
            let cli_token_provided = token.is_some();
            let token_string = match token {
                Some(t) => t,
                None => {
                    let r = validate::validate(&cli.server_url, &req_json)
                        .context("Validation HTTP call failed")?;
                    if let Some(msg) = r.code_message.as_deref() {
                        println!("Server message: {}", msg);
                    }
                    if cli.dump_json {
                        if let Some(j) = &r.full_json {
                            eprintln!("Decrypted JSON: {}", j);
                        }
                    }
                    let t = match r.validate_token.as_deref() {
                        Some(t) if !t.is_empty() => t.to_string(),
                        _ => bail!("Validation did not return a token. Cannot start sideload. Use --dump-json to inspect server response."),
                    };
                    resp = r;
                    t
                }
            };
            if cli.verbose > 0 {
                eprintln!(
                    "Using validate token (len {}): {:.8}…",
                    token_string.len(),
                    token_string
                );
            }
            if let Some(v) = &resp.pkgrom_validate {
                if v.is_empty() {
                    eprintln!("Warning: Xiaomi returned an empty allowed-ROM list. Flashing may be rejected by the device.");
                }
            }
            if (resp.pkgrom_erase == Some(1) || (cli_token_provided && wipe)) && !yes {
                println!("⚠  This ROM will erase all data on the device. Press Enter to continue, or Ctrl+C to cancel.");
                let mut s = String::new();
                let _ = std::io::stdin().read_line(&mut s);
            }
            let allow_wipe = if cli_token_provided {
                wipe
            } else {
                resp.pkgrom_erase == Some(1) || wipe
            };
            sideload_zip(
                &mut client,
                &path,
                cli.chunk_size,
                &token_string,
                allow_wipe,
            )
            .context("Sideload failed")?;
        }
        Commands::FormatData => {
            client
                .simple_command("format-data:")
                .context("format-data:")?;
            client.simple_command("reboot:").context("reboot:")?;
        }
        Commands::Reboot => {
            client.simple_command("reboot:").context("reboot:")?;
        }
    }

    if cli.kill_adb_after {
        let _ = util::adb_server::kill_adb_server(std::time::Duration::from_millis(500));
        #[cfg(windows)]
        {
            util::adb_server::kill_adb_process();
        }
    }

    Ok(())
}
