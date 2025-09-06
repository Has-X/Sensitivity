use std::path::PathBuf;

use anyhow::{bail, Context, Result};
use clap::{ArgAction, Parser, Subcommand};

mod adb;
mod sideload;
mod validate;
mod usb;
mod mi;
mod util;
mod download;

use crate::mi::MiClient;
use crate::mi::profile::{apply_profile, RegionProfile};
use crate::sideload::sideload_zip;
use crate::usb::UsbTransport;
use crate::util::logging::{init_logger, LogVerbosity};
use crate::util::config;

#[derive(Debug, Parser)]
#[command(name = "miassistant", version, about = "SENSITIVITY: Mi Assistant CLI (HasX)")]
struct Cli {
    /// Device index among matching Mi Assistant interfaces
    #[arg(long, default_value_t = 0, global = true)]
    device_index: usize,

    /// Chunk size for sideload (bytes)
    #[arg(long, default_value_t = 65536, global = true)]
    chunk_size: usize,

    /// Validation server URL
    #[arg(long, default_value = "https://update.miui.com/updates/miotaV3.php", global = true)]
    server_url: String,

    /// Allow HTTP (insecure). Prints a big warning.
    #[arg(long, action = ArgAction::SetTrue, global = true)]
    http: bool,

    /// Debug raw USB packets (directions/sizes)
    #[arg(long, action = ArgAction::SetTrue, global = true)]
    debug_usb: bool,

    /// Kill local adb server on 127.0.0.1:5037 before connecting
    #[arg(long, action = ArgAction::SetTrue, global = true)]
    kill_adb_server: bool,

    /// Do not auto-kill adb server on handshake failure
    #[arg(long, action = ArgAction::SetTrue, global = true)]
    no_auto_kill: bool,

    /// Verbose logging
    #[arg(long, short = 'v', action = ArgAction::Count, global = true)]
    verbose: u8,

    /// Dump decrypted JSON from validation/list-allowed-roms
    #[arg(long, action = ArgAction::SetTrue, global = true)]
    dump_json: bool,

    /// Kill adb server after command completes
    #[arg(long, action = ArgAction::SetTrue, global = true)]
    kill_adb_after: bool,

    /// Allow local adb server to run (Windows only). By default we block it for stability.
    #[arg(long, action = ArgAction::SetTrue, global = true)]
    allow_adb: bool,

    /// Override device fields sent to validation (advanced)
    #[arg(long, global = true)]
    override_device: Option<String>,
    #[arg(long, global = true)]
    override_version: Option<String>,
    #[arg(long, global = true)]
    override_sn: Option<String>,
    #[arg(long, global = true)]
    override_codebase: Option<String>,
    #[arg(long, global = true)]
    override_branch: Option<String>,
    #[arg(long, global = true)]
    override_romzone: Option<String>,

    /// Apply a region profile: global, eea, in, ru, id, tr, tw, cn
    #[arg(long, global = true)]
    profile: Option<String>,
    /// Codename to use when building device name from profile (e.g., garnet)
    #[arg(long, global = true)]
    codename: Option<String>,

    /// Override MD5 used for server validation (bypasses file hashing)
    #[arg(long, global = true)]
    md5: Option<String>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// Print device and ROM info fields
    ReadInfo,
    /// Query the server and list allowed ROMs
    ListAllowedRoms,
    /// Validate and sideload the given Recovery ROM zip
    Flash {
        path: PathBuf,
        /// Skip confirmation prompts
        #[arg(long)]
        yes: bool,
        /// Provide validation token manually (skip server validation)
        #[arg(long)]
        token: Option<String>,
    },
    /// Issue format-data and reboot
    FormatData,
    /// Reboot the device
    Reboot,
    /// Download LatestRom reported by server
    DownloadLatest {
        /// Directory to save the ROM into (default: current dir)
        #[arg(long)]
        output_dir: Option<PathBuf>,
    },
    /// Download LatestRom and flash it (validate+flash)
    FlashFromLatest {
        /// Directory to save/download the ROM (default: current dir)
        #[arg(long)]
        output_dir: Option<PathBuf>,
        /// Skip confirmation prompts
        #[arg(long)]
        yes: bool,
    },
    /// Persistently set the MD5 used for validation (bypass hashing)
    SetHash { md5: String },
    /// Clear the persisted MD5 override
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
        bail!("Refusing to use non-HTTPS server without --http. Provided: {}", cli.server_url);
    }
    if cli.http && cli.server_url.starts_with("http://") {
        eprintln!("WARNING: Using HTTP for validation endpoint: {}", cli.server_url);
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
                eprintln!("Exclusive mode: adb port 5037 blocked for this session");
            } else if util::adb_server::is_running(std::time::Duration::from_millis(200)) {
                eprintln!("Warning: adb server still running on port 5037; stability may be affected");
            }
        } else if !cli.kill_adb_server && util::adb_server::is_running(std::time::Duration::from_millis(200)) {
            eprintln!("Note: adb server appears to be running on 127.0.0.1:5037. It may hold the USB. Pass --kill-adb-server to stop it or omit --allow-adb");
        }
        if cli.kill_adb_server {
            if let Err(e) = util::adb_server::kill_adb_server(std::time::Duration::from_secs(2)) {
                eprintln!("Warning: failed to kill adb server: {}", e);
            } else {
                eprintln!("adb server killed (port 5037)");
            }
        }
    }

    // Open USB transport
    let mut make_client = || -> Result<MiClient> {
        let transport = UsbTransport::open(cli.device_index, cli.debug_usb)
            .context("Opening USB Mi Assistant interface via libusb")?;
        MiClient::new(transport).context("Initializing ADB client")
    };
    // Handle config-only subcommands before touching USB
    match &cli.command {
        Commands::SetHash { md5 } => {
            if md5.len() != 32 || !md5.chars().all(|c| c.is_ascii_hexdigit()) {
                bail!("--md5 must be 32 hex chars");
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
                eprintln!("Handshake failed. Attempting to kill adb server and retry once…");
                let _ = util::adb_server::kill_adb_server(std::time::Duration::from_millis(500));
                #[cfg(windows)]
                { util::adb_server::kill_adb_process(); }
                std::thread::sleep(std::time::Duration::from_millis(300));
                #[cfg(windows)]
                { if _adb_block_guard.is_none() { _adb_block_guard = util::adb_server::block_port_5037(); } }
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
            if let Some(p) = &cli.profile { if let Some(rp) = RegionProfile::from_str(p) { info = apply_profile(&info, rp, cli.codename.as_deref(), true)?; eprintln!("Applied profile: {}", p); } }
            if let Some(v) = &cli.override_device { info.device = v.clone(); }
            if let Some(v) = &cli.override_version { info.version = v.clone(); }
            if let Some(v) = &cli.override_sn { info.sn = v.clone(); }
            if let Some(v) = &cli.override_codebase { info.codebase = v.clone(); }
            if let Some(v) = &cli.override_branch { info.branch = v.clone(); }
            if let Some(v) = &cli.override_romzone { info.romzone = v.clone(); }
            let req_json = validate::build_request_json(&info, None).context("Building validation request")?;
            if cli.dump_json { if let Ok(q) = validate::encode_request_b64(&req_json) { eprintln!("Request JSON: {}", req_json); eprintln!("q (base64): {}", q); } }
            let resp = validate::validate(&cli.server_url, &req_json).context("Validation HTTP call failed")?;
            let json = resp.full_json.clone().ok_or_else(|| anyhow::anyhow!("No full JSON in response"))?;
            let (latest, mirrors) = download::parse_latest_from_json(&json).context("Parsing LatestRom from JSON")?;
            let url = download::choose_url(&mirrors, &latest.filename).ok_or_else(|| anyhow::anyhow!("No mirror URL available"))?;
            let client_http = reqwest::blocking::Client::builder().user_agent("MiTunes_UserAgent_v3.0").build()?;
            let out_dir = output_dir.unwrap_or_else(|| std::env::current_dir().unwrap());
            let path = download::download_with_md5(&client_http, &url, &out_dir, &latest.md5).context("Downloading LatestRom")?;
            println!("Downloaded to {} (md5 ok)", path.display());
        }
        Commands::FlashFromLatest { output_dir, yes } => {
            let mut info = client.read_all_info().context("Fetching device info")?;
            if let Some(p) = &cli.profile { if let Some(rp) = RegionProfile::from_str(p) { info = apply_profile(&info, rp, cli.codename.as_deref(), true)?; eprintln!("Applied profile: {}", p); } }
            if let Some(v) = &cli.override_device { info.device = v.clone(); }
            if let Some(v) = &cli.override_version { info.version = v.clone(); }
            if let Some(v) = &cli.override_sn { info.sn = v.clone(); }
            if let Some(v) = &cli.override_codebase { info.codebase = v.clone(); }
            if let Some(v) = &cli.override_branch { info.branch = v.clone(); }
            if let Some(v) = &cli.override_romzone { info.romzone = v.clone(); }
            // Step 1: Get LatestRom info
            let req_json = validate::build_request_json(&info, None).context("Building validation request")?;
            let resp1 = validate::validate(&cli.server_url, &req_json).context("Validation HTTP call failed")?;
            let json = resp1.full_json.clone().ok_or_else(|| anyhow::anyhow!("No full JSON in response"))?;
            let (latest, mirrors) = download::parse_latest_from_json(&json).context("Parsing LatestRom from JSON")?;
            let url = download::choose_url(&mirrors, &latest.filename).ok_or_else(|| anyhow::anyhow!("No mirror URL available"))?;
            // Step 2: Download
            let client_http = reqwest::blocking::Client::builder().user_agent("MiTunes_UserAgent_v3.0").build()?;
            let out_dir = output_dir.unwrap_or_else(|| std::env::current_dir().unwrap());
            let local_path = download::download_with_md5(&client_http, &url, &out_dir, &latest.md5).context("Downloading LatestRom")?;
            // Step 3: Validate for this MD5 and flash
            let req_json2 = validate::build_request_json(&info, Some(latest.md5.clone())).context("Building validation request")?;
            if cli.dump_json { if let Ok(q) = validate::encode_request_b64(&req_json2) { eprintln!("Request JSON: {}", req_json2); eprintln!("q (base64): {}", q); } }
            let resp2 = validate::validate(&cli.server_url, &req_json2).context("Validation HTTP call failed")?;
            if let Some(msg) = resp2.code_message.as_deref() { println!("Server message: {}", msg); }
            if resp2.pkgrom_erase == Some(1) && !yes {
                println!("NOTICE: Data will be erased during flashing. Press Enter to continue…");
                let mut s = String::new();
                let _ = std::io::stdin().read_line(&mut s);
            }
            let token = resp2.validate_token.as_deref().ok_or_else(|| anyhow::anyhow!("Missing Validate token in response"))?.to_string();
            if cli.verbose > 0 { eprintln!("Using validate token (len {}): {:.8}…", token.len(), token); }
            sideload_zip(&mut client, &local_path, cli.chunk_size, &token).context("Sideload failed")?;
        }
        Commands::SetHash { .. } => {
            // Already handled before USB init
            return Ok(());
        }
        Commands::ClearHash => {
            // Already handled before USB init
            return Ok(());
        }
        Commands::ListAllowedRoms => {
            let mut info = client.read_all_info().context("Fetching device info")?;
            if let Some(p) = &cli.profile { if let Some(rp) = RegionProfile::from_str(p) { info = apply_profile(&info, rp, cli.codename.as_deref(), true)?; eprintln!("Applied profile: {}", p); } }
            if let Some(v) = &cli.override_device { info.device = v.clone(); }
            if let Some(v) = &cli.override_version { info.version = v.clone(); }
            if let Some(v) = &cli.override_sn { info.sn = v.clone(); }
            if let Some(v) = &cli.override_codebase { info.codebase = v.clone(); }
            if let Some(v) = &cli.override_branch { info.branch = v.clone(); }
            if let Some(v) = &cli.override_romzone { info.romzone = v.clone(); }
            let req_json = validate::build_request_json(&info, None).context("Building validation request")?;
            if cli.dump_json {
                if let Ok(q) = validate::encode_request_b64(&req_json) {
                    eprintln!("Request JSON: {}", req_json);
                    eprintln!("q (base64): {}", q);
                }
            }
            let resp = validate::validate(&cli.server_url, &req_json).context("Validation HTTP call failed")?;
            validate::print_allowed_with_options(&resp, cli.dump_json);
        }
        Commands::Flash { path, yes, token } => {
            if !path.exists() {
                bail!("Zip not found: {}", path.display());
            }
            let mut info = client.read_all_info().context("Fetching device info")?;
            if let Some(p) = &cli.profile { if let Some(rp) = RegionProfile::from_str(p) { info = apply_profile(&info, rp, cli.codename.as_deref(), true)?; eprintln!("Applied profile: {}", p); } }
            if let Some(v) = &cli.override_device { info.device = v.clone(); }
            if let Some(v) = &cli.override_version { info.version = v.clone(); }
            if let Some(v) = &cli.override_sn { info.sn = v.clone(); }
            if let Some(v) = &cli.override_codebase { info.codebase = v.clone(); }
            if let Some(v) = &cli.override_branch { info.branch = v.clone(); }
            if let Some(v) = &cli.override_romzone { info.romzone = v.clone(); }
            let computed_md5 = util::md5::md5_file(&path).context("Computing MD5 of zip")?;
            // Determine MD5 to use (CLI > persisted > computed)
            let used_md5 = if let Some(m) = &cli.md5 { m.clone() } else if let Some(m) = &state.override_md5 { m.clone() } else { computed_md5.clone() };
            if used_md5.len() != 32 || !used_md5.chars().all(|c| c.is_ascii_hexdigit()) {
                bail!("Provided MD5 must be 32 hex characters");
            }
            if used_md5.to_lowercase() != computed_md5 {
                eprintln!("WARNING: Using overridden MD5 {} (computed {})", used_md5, computed_md5);
            } else {
                if cli.verbose > 0 { eprintln!("Using MD5 {}", used_md5); }
            }
            let req_json = validate::build_request_json(&info, Some(used_md5.clone())).context("Building validation request")?;
            if cli.dump_json {
                if let Ok(q) = validate::encode_request_b64(&req_json) {
                    eprintln!("Request JSON: {}", req_json);
                    eprintln!("q (base64): {}", q);
                }
            }
            let mut resp = validate::ValidateResult::default();
            let token = match token {
                Some(t) => t,
                None => {
                    let r = validate::validate(&cli.server_url, &req_json).context("Validation HTTP call failed")?;
                    if let Some(msg) = r.code_message.as_deref() { println!("Server message: {}", msg); }
                    if cli.dump_json { if let Some(j) = &r.full_json { eprintln!("Decrypted JSON: {}", j); } }
                    let t = match r.validate_token.as_deref() {
                        Some(t) if !t.is_empty() => t.to_string(),
                        _ => bail!("Validation did not return a token. Cannot start sideload. Use --dump-json to inspect server response."),
                    };
                    resp = r;
                    t
                }
            };
            if cli.verbose > 0 { eprintln!("Using validate token (len {}): {:.8}…", token.len(), token); }
            if let Some(v) = &resp.pkgrom_validate {
                if v.is_empty() {
                    eprintln!("No allowed ROMs reported by server (Validate array empty). Proceeding may fail.");
                }
            }
            if resp.pkgrom_erase == Some(1) && !yes {
                println!("NOTICE: Data will be erased during flashing. Press Enter to continue…");
                let mut s = String::new();
                let _ = std::io::stdin().read_line(&mut s);
            }
            sideload_zip(&mut client, &path, cli.chunk_size, &token).context("Sideload failed")?;
        }
        Commands::FormatData => {
            client.simple_command("format-data:").context("format-data:")?;
            client.simple_command("reboot:").context("reboot:")?;
        }
        Commands::Reboot => {
            client.simple_command("reboot:").context("reboot:")?;
        }
    }

    if cli.kill_adb_after {
        let _ = util::adb_server::kill_adb_server(std::time::Duration::from_millis(500));
        #[cfg(windows)]
        { util::adb_server::kill_adb_process(); }
    }

    Ok(())
}
