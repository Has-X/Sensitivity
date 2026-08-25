// Copyright (C) 2026 Chromatic
// Licensed under the GNU AGPL v3.0. See LICENSE file for details.
// Website: https://chromatic.hu

use std::io::{self, IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{bail, Context, Result};
use clap::{ArgAction, CommandFactory, Parser, Subcommand, ValueEnum};
use clap_complete::Shell;

use sensitivity::mi::profile::{apply_profile, RegionProfile};
use sensitivity::mi::{DeviceInfo, MiClient};
use sensitivity::sideload::{sideload_zip, sideload_zip_with_progress};
use sensitivity::usb::UsbTransport;
use sensitivity::{
    download,
    i18n::{tr, trf},
    util, validate,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum AdbPolicy {
    /// Leave the user's local ADB server alone.
    Keep,
    /// Ask a running ADB server to stop before opening the USB interface.
    Stop,
}

#[derive(Debug, Parser)]
#[command(
    name = "sensitivity",
    version,
    about = "Flash official Xiaomi Recovery ROMs over direct USB",
    long_about = "Sensitivity is a direct-USB Xiaomi Recovery flash and rescue tool. It does not require adb, an unlocked bootloader, or proprietary Xiaomi desktop software."
)]
struct Cli {
    /// Emit newline-delimited JSON events for a supervising application
    #[arg(long, global = true, hide = true)]
    machine: bool,

    /// File whose creation requests graceful cancellation
    #[arg(long, global = true, hide = true)]
    cancel_file: Option<PathBuf>,

    /// File whose creation approves a server-required data wipe
    #[arg(long, global = true, hide = true)]
    approval_file: Option<PathBuf>,

    /// Device index among matching Mi Assistant interfaces
    #[arg(long, default_value_t = 0, global = true)]
    device_index: usize,

    /// Chunk size for sideload (bytes)
    #[arg(long, default_value_t = 65536, global = true, hide = true)]
    chunk_size: usize,

    /// Validation server URL
    #[arg(
        long,
        default_value = "https://update.miui.com/updates/miotaV3.php",
        global = true,
        hide = true
    )]
    server_url: String,

    /// Allow HTTP (insecure). Prints a big warning.
    #[arg(long, action = ArgAction::SetTrue, global = true, hide = true)]
    http: bool,

    /// Debug raw USB packets (directions/sizes)
    #[arg(long, action = ArgAction::SetTrue, global = true, hide = true)]
    debug_usb: bool,

    /// How to coexist with a local ADB server
    #[arg(long, value_enum, default_value_t = AdbPolicy::Keep, global = true)]
    adb_policy: AdbPolicy,

    /// Override device fields sent to validation (advanced)
    #[arg(long, global = true, hide = true)]
    override_device: Option<String>,
    #[arg(long, global = true, hide = true)]
    override_version: Option<String>,
    #[arg(long, global = true, hide = true)]
    override_sn: Option<String>,
    #[arg(long, global = true, hide = true)]
    override_codebase: Option<String>,
    #[arg(long, global = true, hide = true)]
    override_branch: Option<String>,
    #[arg(long, global = true, hide = true)]
    override_romzone: Option<String>,

    /// Apply a region profile: global, eea, in, ru, id, tr, tw, cn
    #[arg(long, global = true)]
    profile: Option<RegionProfile>,
    /// Codename to use when building device name from profile (e.g., garnet)
    #[arg(long, global = true)]
    codename: Option<String>,

    /// Override MD5 used for server validation (bypasses file hashing)
    #[arg(long, global = true, hide = true)]
    md5: Option<String>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// Generate shell completion definitions
    Completions {
        /// Shell whose completion format should be generated
        #[arg(value_enum)]
        shell: Shell,
    },
    /// List matching recovery USB interfaces without opening them
    Devices {
        /// Emit stable machine-readable JSON
        #[arg(long)]
        json: bool,
    },
    /// Check USB access and identify common setup problems
    Doctor,
    /// Check that a recovery device can complete the protocol handshake
    Detect,
    /// Print device and ROM information
    #[command(visible_alias = "read-info")]
    Info {
        /// Emit stable machine-readable JSON
        #[arg(long)]
        json: bool,
    },
    /// Query the server and list allowed ROMs
    ListAllowedRoms {
        /// Write a redacted validation response to PATH for troubleshooting
        #[arg(long, value_name = "PATH")]
        dump_json: Option<PathBuf>,
    },
    /// Validate and sideload the given Recovery ROM zip
    Flash {
        path: PathBuf,
        /// Skip confirmation prompts
        #[arg(long)]
        yes: bool,
        /// Provide validation token manually (skip server validation)
        #[arg(long)]
        token: Option<String>,
        /// Allow/force data wipe (sets sideload-host :1). Useful if using --token without server response.
        #[arg(long, action = ArgAction::SetTrue)]
        wipe: bool,
        /// Write a redacted validation response to PATH for troubleshooting
        #[arg(long, value_name = "PATH")]
        dump_json: Option<PathBuf>,
        /// Validate with Xiaomi and stop before sideloading
        #[arg(long, conflicts_with = "token")]
        validate_only: bool,
    },
    /// Erase user data, then reboot
    FormatData {
        /// Confirm the destructive operation without an interactive prompt
        #[arg(long)]
        yes: bool,
    },
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
        /// Allow/force data wipe (sets sideload-host :1). Overrides server Erase=0 when true.
        #[arg(long, action = ArgAction::SetTrue)]
        wipe: bool,
    },
}

fn main() -> ExitCode {
    let machine = std::env::args_os().any(|argument| argument == "--machine");
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            if machine {
                emit_machine_event(serde_json::json!({
                    "event": "error",
                    "message": format!("{error:#}")
                }));
            }
            eprintln!("{}: {error:#}", tr("error.prefix"));
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<()> {
    let cli = Cli::parse();
    reset_control_file(cli.cancel_file.as_deref())?;
    reset_control_file(cli.approval_file.as_deref())?;
    if !cli.server_url.starts_with("https://") && !cli.http {
        bail!(
            "{}",
            trf("error.refuse_http", &[("{url}", &cli.server_url)])
        );
    }
    if cli.http && cli.server_url.starts_with("http://") {
        eprintln!("{}", trf("warning.http", &[("{url}", &cli.server_url)]));
    }

    let adb_was_running = util::adb_server::is_running(std::time::Duration::from_millis(200));
    if cli.adb_policy == AdbPolicy::Stop && adb_was_running {
        util::adb_server::kill_adb_server(std::time::Duration::from_secs(2))
            .context(tr("error.stop_adb"))?;
        eprintln!("{}", tr("warning.adb_stopped"));
    }

    // Open USB transport
    let make_client = || -> Result<MiClient> {
        if cli.adb_policy == AdbPolicy::Keep && adb_was_running {
            let recoveries = util::adb_server::discover_mi_recoveries(Duration::from_secs(3))
                .context("Discovering Mi Recovery devices through the running ADB server")?;
            if !recoveries.is_empty() {
                let recovery = recoveries.get(cli.device_index).ok_or_else(|| {
                    anyhow::anyhow!(
                        "Device index {} out of range ({} found through ADB server)",
                        cli.device_index,
                        recoveries.len()
                    )
                })?;
                return Ok(MiClient::from_adb_server(recovery.transport_id));
            }
        }
        let transport =
            UsbTransport::open(cli.device_index, cli.debug_usb).context(tr("error.open_usb"))?;
        MiClient::new(transport).context(tr("error.init_adb"))
    };
    // Handle config-only subcommands before touching USB
    match &cli.command {
        Commands::Completions { shell } => {
            let mut command = Cli::command();
            clap_complete::generate(*shell, &mut command, "sensitivity", &mut io::stdout());
            return Ok(());
        }
        Commands::Devices { json } => {
            let devices = discover_recovery_interfaces(cli.adb_policy, adb_was_running)
                .context(tr("error.discover_usb"))?;
            if *json {
                println!("{}", serde_json::to_string_pretty(&devices)?);
            } else if devices.is_empty() {
                println!("{}", tr("status.no_devices"));
                println!(
                    "Boot stock recovery, choose 'Connect with Mi Assistant', then reconnect USB."
                );
            } else {
                println!(
                    "{}",
                    trf(
                        "status.devices_found",
                        &[("{count}", &devices.len().to_string())]
                    )
                );
                for device in &devices {
                    print_usb_device(device);
                }
            }
            return Ok(());
        }
        Commands::Doctor => {
            println!(
                "{}",
                trf(
                    "status.sensitivity_version",
                    &[("{version}", env!("CARGO_PKG_VERSION"))]
                )
            );
            println!(
                "{}",
                trf(
                    "label.platform",
                    &[
                        ("{platform}", std::env::consts::OS),
                        ("{arch}", std::env::consts::ARCH)
                    ]
                )
            );
            println!(
                "{}",
                trf(
                    "label.adb_server",
                    &[(
                        "{state}",
                        if adb_was_running {
                            "running"
                        } else {
                            "not running"
                        }
                    )]
                )
            );
            let devices = discover_recovery_interfaces(cli.adb_policy, adb_was_running)
                .context(tr("error.discover_usb"))?;
            println!(
                "{}",
                trf(
                    "status.matching_interfaces",
                    &[("{count}", &devices.len().to_string())]
                )
            );
            for device in &devices {
                print_usb_device(device);
            }
            if devices.is_empty() {
                eprintln!("\n{}", tr("status.no_matching"));
                #[cfg(windows)]
                eprintln!("On Windows, the Mi Assistant interface must use the WinUSB driver.");
                #[cfg(target_os = "linux")]
                eprintln!(
                    "On Linux, reconnect the phone and check USB permissions if access is denied."
                );
                bail!("{}", tr("error.doctor_setup"));
            }
            match make_client() {
                Ok(_) => {
                    println!("{}", tr("status.recovery_ready"));
                    println!("{}", tr("status.ready_result"));
                    return Ok(());
                }
                Err(error) => {
                    println!("{}", tr("status.recovery_unavailable"));
                    eprintln!("\n{error:#}");
                    if adb_was_running
                        && cli.adb_policy == AdbPolicy::Keep
                        && adb_may_own_interface(&error)
                    {
                        eprintln!(
                            "\nTry again with `--adb-policy stop` if ADB owns the USB interface."
                        );
                    }
                    #[cfg(windows)]
                    eprintln!("On Windows, the Mi Assistant interface must use the WinUSB driver.");
                    #[cfg(target_os = "linux")]
                    eprintln!("On Linux, reconnect the phone and check USB permissions if access is denied.");
                    bail!("{}", tr("error.doctor_setup"));
                }
            }
        }
        _ => {}
    }

    let mut client = make_client().map_err(|error| {
        if adb_was_running
            && cli.adb_policy == AdbPolicy::Keep
            && adb_may_own_interface(&error)
        {
            error.context(
                "A local ADB server is running. If it owns this USB interface, retry with --adb-policy stop",
            )
        } else {
            error
        }
    })?;
    let identity = IdentityOptions::from(&cli);

    match cli.command {
        Commands::Completions { .. } => {
            unreachable!("completions returns before USB command dispatch")
        }
        Commands::Devices { .. } => unreachable!("devices returns before USB command dispatch"),
        Commands::Doctor => unreachable!("doctor returns before USB command dispatch"),
        Commands::Detect => {
            println!("{}", tr("status.recovery_ready"));
        }
        Commands::Info { json } => {
            let info = client.read_all_info().context(tr("error.fetch_device"))?;
            if json {
                println!("{}", serde_json::to_string_pretty(&info)?);
            } else {
                println!("{}", trf("label.device", &[("{value}", &info.device)]));
                println!("{}", trf("label.version", &[("{value}", &info.version)]));
                println!("{}", trf("label.serial", &[("{value}", &info.sn)]));
                println!("{}", trf("label.codebase", &[("{value}", &info.codebase)]));
                println!("{}", trf("label.branch", &[("{value}", &info.branch)]));
                println!("{}", trf("label.language", &[("{value}", &info.language)]));
                println!("{}", trf("label.region", &[("{value}", &info.region)]));
                println!("{}", trf("label.romzone", &[("{value}", &info.romzone)]));
            }
        }
        Commands::DownloadLatest { output_dir } => {
            let info = effective_device_info(
                &identity,
                client.read_all_info().context(tr("error.fetch_device"))?,
            )?;
            let req_json =
                validate::build_request_json(&info, None).context(tr("error.build_validation"))?;
            let resp = validate::validate(&cli.server_url, &req_json)
                .context(tr("error.validation_http"))?;
            let json = resp
                .full_json
                .clone()
                .ok_or_else(|| anyhow::anyhow!(tr("error.no_full_json")))?;
            let (latest, mirrors) =
                download::parse_latest_from_json(&json).context(tr("error.parse_latest"))?;
            let url = download::choose_url(&mirrors, &latest.filename)
                .ok_or_else(|| anyhow::anyhow!(tr("error.no_mirror")))?;
            let client_http = reqwest::blocking::Client::builder()
                .user_agent("MiTunes_UserAgent_v3.0")
                .build()?;
            let out_dir = output_dir.unwrap_or_else(|| std::env::current_dir().unwrap());
            let path = download::download_with_md5(&client_http, &url, &out_dir, &latest.md5)
                .context(tr("error.download_latest"))?;
            if cli.machine {
                emit_machine_event(serde_json::json!({
                    "event": "downloaded",
                    "path": path,
                    "md5_verified": true
                }));
            } else {
                println!(
                    "{}",
                    trf(
                        "status.downloaded",
                        &[("{path}", &path.display().to_string())]
                    )
                );
            }
        }
        Commands::FlashFromLatest {
            output_dir,
            yes,
            wipe,
        } => {
            emit_status(cli.machine, &tr("status.reading_recovery"));
            let info = effective_device_info(
                &identity,
                client.read_all_info().context(tr("error.fetch_device"))?,
            )?;
            // Step 1: Get LatestRom info
            let req_json =
                validate::build_request_json(&info, None).context(tr("error.build_validation"))?;
            let resp1 = validate::validate(&cli.server_url, &req_json)
                .context(tr("error.validation_http"))?;
            let json = resp1
                .full_json
                .clone()
                .ok_or_else(|| anyhow::anyhow!(tr("error.no_full_json")))?;
            let (latest, mirrors) =
                download::parse_latest_from_json(&json).context(tr("error.parse_latest"))?;
            let url = download::choose_url(&mirrors, &latest.filename)
                .ok_or_else(|| anyhow::anyhow!(tr("error.no_mirror")))?;
            // Step 2: Download
            emit_status(cli.machine, &tr("status.downloading"));
            let client_http = reqwest::blocking::Client::builder()
                .user_agent("MiTunes_UserAgent_v3.0")
                .build()?;
            let out_dir = output_dir.unwrap_or_else(|| std::env::current_dir().unwrap());
            let local_path = download::download_with_md5(&client_http, &url, &out_dir, &latest.md5)
                .context(tr("error.download_latest"))?;
            // Step 3: Validate for this MD5 and flash
            let req_json2 = validate::build_request_json(&info, Some(latest.md5.clone()))
                .context(tr("error.build_validation"))?;
            let resp2 = validate::validate(&cli.server_url, &req_json2)
                .context(tr("error.validation_http"))?;
            if let Some(msg) = resp2.code_message.as_deref() {
                println!("{}", trf("status.server_message", &[("{message}", msg)]));
            }
            let cancel = install_cancel_handler(cli.cancel_file.as_deref())?;
            if (resp2.pkgrom_erase == Some(1) || wipe) && !yes {
                confirm_data_wipe_supervised(cli.machine, cli.approval_file.as_deref(), &cancel)?;
            }
            let token = resp2
                .validate_token
                .as_deref()
                .ok_or_else(|| anyhow::anyhow!(tr("error.missing_token")))?
                .to_string();
            let allow_wipe = resp2.pkgrom_erase == Some(1) || wipe;
            emit_status(cli.machine, &tr("status.flashing"));
            run_sideload(
                &mut client,
                &local_path,
                cli.chunk_size,
                &token,
                allow_wipe,
                &cancel,
                cli.machine,
            )
            .context(tr("error.sideload"))?;
            emit_completed(cli.machine, &tr("status.flash_completed"));
        }
        Commands::ListAllowedRoms { dump_json } => {
            let info = effective_device_info(
                &identity,
                client.read_all_info().context(tr("error.fetch_device"))?,
            )?;
            let req_json =
                validate::build_request_json(&info, None).context(tr("error.build_validation"))?;
            let resp = validate::validate(&cli.server_url, &req_json)
                .context(tr("error.validation_http"))?;
            if let Some(path) = dump_json.as_deref() {
                write_redacted_validation_response(&resp, path)?;
            }
            validate::print_allowed(&resp)?;
        }
        Commands::Flash {
            path,
            yes,
            token,
            wipe,
            dump_json,
            validate_only,
        } => {
            if !path.exists() {
                bail!(
                    "{}",
                    trf(
                        "error.zip_not_found",
                        &[("{path}", &path.display().to_string())]
                    )
                );
            }
            emit_status(cli.machine, &tr("status.reading_recovery"));
            let info = effective_device_info(
                &identity,
                client.read_all_info().context(tr("error.fetch_device"))?,
            )?;
            emit_status(cli.machine, &tr("status.checking_package"));
            let computed_md5 = util::md5::md5_file(&path).context(tr("error.compute_md5"))?;
            // An explicit one-session override is retained for protocol debugging.
            let used_md5 = if let Some(m) = &cli.md5 {
                m.clone()
            } else {
                computed_md5.clone()
            };
            if used_md5.len() != 32 || !used_md5.chars().all(|c| c.is_ascii_hexdigit()) {
                bail!("{}", tr("error.md5_length"));
            }
            if used_md5.to_lowercase() != computed_md5 {
                eprintln!(
                    "{}",
                    trf(
                        "warning.md5_override",
                        &[("{override}", &used_md5), ("{computed}", &computed_md5)]
                    )
                );
            }
            let req_json = validate::build_request_json(&info, Some(used_md5.clone()))
                .context(tr("error.build_validation"))?;
            let mut resp = validate::ValidateResult::default();
            // Preserve whether CLI provided a token before shadowing it
            let cli_token_provided = token.is_some();
            let token_string = match token {
                Some(t) => t,
                None => {
                    let r = validate::validate(&cli.server_url, &req_json)
                        .context(tr("error.validation_http"))?;
                    if let Some(path) = dump_json.as_deref() {
                        write_redacted_validation_response(&r, path)?;
                    }
                    if let Some(msg) = r.code_message.as_deref() {
                        println!("{}", trf("status.server_message", &[("{message}", msg)]));
                    }
                    let t = match r.validate_token.as_deref() {
                        Some(t) if !t.is_empty() => t.to_string(),
                        _ => bail!("{}", tr("error.no_token")),
                    };
                    resp = r;
                    t
                }
            };
            if validate_only {
                emit_status(cli.machine, &tr("status.ready_result"));
                return Ok(());
            }
            if let Some(v) = &resp.pkgrom_validate {
                if v.is_empty() {
                    eprintln!("No allowed ROMs reported by server (Validate array empty). Proceeding may fail.");
                }
            }
            let cancel = install_cancel_handler(cli.cancel_file.as_deref())?;
            if (resp.pkgrom_erase == Some(1) || (cli_token_provided && wipe)) && !yes {
                confirm_data_wipe_supervised(cli.machine, cli.approval_file.as_deref(), &cancel)?;
            }
            let allow_wipe = if cli_token_provided {
                wipe
            } else {
                resp.pkgrom_erase == Some(1) || wipe
            };
            emit_status(cli.machine, &tr("status.flashing"));
            run_sideload(
                &mut client,
                &path,
                cli.chunk_size,
                &token_string,
                allow_wipe,
                &cancel,
                cli.machine,
            )
            .context(tr("error.sideload"))?;
            emit_completed(cli.machine, &tr("status.flash_completed"));
        }
        Commands::FormatData { yes } => {
            if !yes {
                confirm_data_wipe()?;
            }
            client
                .simple_command("format-data:")
                .context("format-data:")?;
            client.simple_command("reboot:").context("reboot:")?;
        }
        Commands::Reboot => {
            client.simple_command("reboot:").context("reboot:")?;
        }
    }

    Ok(())
}

fn reset_control_file(path: Option<&Path>) -> Result<()> {
    if let Some(path) = path {
        match std::fs::remove_file(path) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(error)
                    .with_context(|| format!("Resetting control file {}", path.display()));
            }
        }
    }
    Ok(())
}

fn write_redacted_validation_response(
    result: &validate::ValidateResult,
    path: &Path,
) -> Result<()> {
    let diagnostic = validate::redacted_response_json(result)?;
    std::fs::write(path, diagnostic)
        .with_context(|| format!("Writing redacted validation response to {}", path.display()))
}

fn emit_machine_event(event: serde_json::Value) {
    println!("{event}");
    io::stdout().flush().ok();
}

fn emit_status(machine: bool, message: &str) {
    if machine {
        emit_machine_event(serde_json::json!({
            "event": "status",
            "message": message
        }));
    }
}

fn emit_completed(machine: bool, message: &str) {
    if machine {
        emit_machine_event(serde_json::json!({
            "event": "completed",
            "message": message
        }));
    }
}

fn confirm_data_wipe_supervised(
    machine: bool,
    approval_file: Option<&Path>,
    cancel: &AtomicBool,
) -> Result<()> {
    if !machine {
        return confirm_data_wipe();
    }
    let approval_file = approval_file.ok_or_else(|| anyhow::anyhow!(tr("error.approval_file")))?;
    emit_machine_event(serde_json::json!({
        "event": "confirmation_required",
        "kind": "data_wipe",
        "message": "Xiaomi requires this flash to permanently erase user data."
    }));
    loop {
        if cancel.load(Ordering::Relaxed) {
            bail!("{}", tr("error.wipe_not_approved"));
        }
        if approval_file.exists() {
            let _ = std::fs::remove_file(approval_file);
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

#[allow(clippy::too_many_arguments)]
fn run_sideload(
    client: &mut MiClient,
    path: &Path,
    chunk_size: usize,
    token: &str,
    allow_wipe: bool,
    cancel: &AtomicBool,
    machine: bool,
) -> Result<()> {
    if machine {
        sideload_zip_with_progress(
            client,
            path,
            chunk_size,
            token,
            allow_wipe,
            cancel,
            |current, total| {
                emit_machine_event(serde_json::json!({
                    "event": "progress",
                    "current": current,
                    "total": total
                }));
            },
        )
    } else {
        sideload_zip(client, path, chunk_size, token, allow_wipe, cancel)
    }
}

fn confirm_data_wipe() -> Result<()> {
    if !io::stdin().is_terminal() {
        bail!("{}", tr("error.wipe_terminal"));
    }
    eprintln!("{}", tr("prompt.erase_warning"));
    eprint!("{}", tr("prompt.erase_type"));
    io::stderr().flush().ok();
    let mut answer = String::new();
    io::stdin().read_line(&mut answer)?;
    if answer.trim() != "ERASE" {
        bail!("{}", tr("error.wipe_cancelled"));
    }
    Ok(())
}

fn install_cancel_handler(cancel_file: Option<&Path>) -> Result<Arc<AtomicBool>> {
    let cancel = Arc::new(AtomicBool::new(false));
    let handler_flag = Arc::clone(&cancel);
    ctrlc::set_handler(move || {
        handler_flag.store(true, Ordering::Relaxed);
        eprintln!("\n{}", tr("status.cancel_requested"));
    })
    .context(tr("error.install_ctrl_c"))?;
    if let Some(path) = cancel_file {
        let path = path.to_path_buf();
        let file_flag = Arc::clone(&cancel);
        std::thread::spawn(move || loop {
            if path.exists() {
                file_flag.store(true, Ordering::Relaxed);
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        });
    }
    Ok(cancel)
}

fn print_usb_device(device: &sensitivity::usb::UsbDeviceInfo) {
    if device.transport == "adb-server" {
        println!(
            "  [{}] {} via the running ADB server",
            device.index,
            device.recovery_device.as_deref().unwrap_or("Mi Recovery")
        );
        return;
    }
    println!(
        "  [{}] {:04x}:{:04x} bus {} address {} interface {} endpoints 0x{:02x}/0x{:02x}",
        device.index,
        device.vendor_id,
        device.product_id,
        device.bus,
        device.address,
        device.interface,
        device.endpoint_in,
        device.endpoint_out
    );
}

fn discover_recovery_interfaces(
    adb_policy: AdbPolicy,
    adb_server_running: bool,
) -> Result<Vec<sensitivity::usb::UsbDeviceInfo>> {
    if adb_policy == AdbPolicy::Keep && adb_server_running {
        let recoveries = util::adb_server::discover_mi_recoveries(Duration::from_secs(3))?;
        if !recoveries.is_empty() {
            return Ok(recoveries
                .into_iter()
                .enumerate()
                .map(|(index, recovery)| sensitivity::usb::UsbDeviceInfo {
                    index,
                    transport: "adb-server".to_owned(),
                    transport_id: Some(recovery.transport_id),
                    bus: 0,
                    address: 0,
                    vendor_id: 0,
                    product_id: 0,
                    interface: 0,
                    protocol: 1,
                    endpoint_in: 0,
                    endpoint_out: 0,
                    recovery_device: Some(recovery.recovery_device),
                    model: recovery.model,
                })
                .collect());
        }
    }
    UsbTransport::discover()
}

fn adb_may_own_interface(error: &anyhow::Error) -> bool {
    let message = format!("{error:#}");
    message.contains("Claiming interface") || message.contains("Opening USB device")
}

struct IdentityOptions {
    profile: Option<RegionProfile>,
    codename: Option<String>,
    device: Option<String>,
    version: Option<String>,
    serial: Option<String>,
    codebase: Option<String>,
    branch: Option<String>,
    romzone: Option<String>,
}

impl From<&Cli> for IdentityOptions {
    fn from(cli: &Cli) -> Self {
        Self {
            profile: cli.profile,
            codename: cli.codename.clone(),
            device: cli.override_device.clone(),
            version: cli.override_version.clone(),
            serial: cli.override_sn.clone(),
            codebase: cli.override_codebase.clone(),
            branch: cli.override_branch.clone(),
            romzone: cli.override_romzone.clone(),
        }
    }
}

fn effective_device_info(options: &IdentityOptions, mut info: DeviceInfo) -> Result<DeviceInfo> {
    if let Some(profile) = options.profile {
        info = apply_profile(&info, profile, options.codename.as_deref())?;
        eprintln!(
            "{}",
            trf(
                "status.profile_applied",
                &[("{profile}", &format!("{profile:?}"))]
            )
        );
    }
    if let Some(value) = &options.device {
        info.device = value.clone();
    }
    if let Some(value) = &options.version {
        info.version = value.clone();
    }
    if let Some(value) = &options.serial {
        info.sn = value.clone();
    }
    if let Some(value) = &options.codebase {
        info.codebase = value.clone();
    }
    if let Some(value) = &options.branch {
        info.branch = value.clone();
    }
    if let Some(value) = &options.romzone {
        info.romzone = value.clone();
    }
    Ok(info)
}

#[cfg(test)]
mod cli_tests {
    use super::*;

    #[test]
    fn adb_is_preserved_by_default() {
        let cli = Cli::try_parse_from(["sensitivity", "detect"]).unwrap();
        assert_eq!(cli.adb_policy, AdbPolicy::Keep);
    }

    #[test]
    fn explicit_stop_policy_parses() {
        let cli = Cli::try_parse_from(["sensitivity", "--adb-policy", "stop", "doctor"]).unwrap();
        assert_eq!(cli.adb_policy, AdbPolicy::Stop);
    }

    #[test]
    fn read_info_compatibility_alias_parses() {
        let cli = Cli::try_parse_from(["sensitivity", "read-info", "--json"]).unwrap();
        assert!(matches!(cli.command, Commands::Info { json: true }));
    }

    #[test]
    fn completion_shell_parses_without_usb_options() {
        let cli = Cli::try_parse_from(["sensitivity", "completions", "bash"]).unwrap();
        assert!(matches!(
            cli.command,
            Commands::Completions { shell: Shell::Bash }
        ));
    }

    #[test]
    fn adb_hint_is_only_used_for_usb_ownership_errors() {
        assert!(adb_may_own_interface(&anyhow::anyhow!(
            "Claiming interface 1"
        )));
        assert!(!adb_may_own_interface(&anyhow::anyhow!(
            "No Mi Assistant ADB interface found"
        )));
    }

    #[test]
    fn machine_supervision_options_parse() {
        let cli = Cli::try_parse_from([
            "sensitivity",
            "--machine",
            "--cancel-file",
            "cancel.flag",
            "--approval-file",
            "approve.flag",
            "flash",
            "rom.zip",
        ])
        .unwrap();
        assert!(cli.machine);
        assert_eq!(cli.cancel_file, Some(PathBuf::from("cancel.flag")));
        assert_eq!(cli.approval_file, Some(PathBuf::from("approve.flag")));
    }

    #[test]
    fn flash_redacted_diagnostic_path_parses() {
        let cli = Cli::try_parse_from([
            "sensitivity",
            "flash",
            "rom.zip",
            "--dump-json",
            "validation-shape.json",
        ])
        .unwrap();
        assert!(matches!(
            cli.command,
            Commands::Flash {
                dump_json: Some(path),
                ..
            } if path.as_path() == Path::new("validation-shape.json")
        ));
    }

    #[test]
    fn list_allowed_roms_redacted_diagnostic_path_parses() {
        let cli = Cli::try_parse_from([
            "sensitivity",
            "list-allowed-roms",
            "--dump-json",
            "validation-shape.json",
        ])
        .unwrap();
        assert!(matches!(
            cli.command,
            Commands::ListAllowedRoms {
                dump_json: Some(path)
            } if path.as_path() == Path::new("validation-shape.json")
        ));
    }

    #[test]
    fn flash_validate_only_parses() {
        let cli =
            Cli::try_parse_from(["sensitivity", "flash", "rom.zip", "--validate-only"]).unwrap();
        assert!(matches!(
            cli.command,
            Commands::Flash {
                validate_only: true,
                ..
            }
        ));
        assert!(Cli::try_parse_from([
            "sensitivity",
            "flash",
            "rom.zip",
            "--validate-only",
            "--token",
            "private-token",
        ])
        .is_err());
    }

    #[test]
    fn supervised_wipe_requires_an_approval_path() {
        let cancel = AtomicBool::new(true);
        assert!(confirm_data_wipe_supervised(true, None, &cancel).is_err());
    }

    #[test]
    fn supervised_wipe_consumes_the_approval_file() {
        let directory = tempfile::tempdir().unwrap();
        let approval = directory.path().join("approve");
        std::fs::write(&approval, []).unwrap();
        let cancel = AtomicBool::new(false);

        confirm_data_wipe_supervised(true, Some(&approval), &cancel).unwrap();

        assert!(!approval.exists());
    }
}
