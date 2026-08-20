// Copyright (C) 2026 HasX
// Licensed under the GNU AGPL v3.0. See LICENSE file for details.
// Website: https://hasx.dev

use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::time::Duration;

use anyhow::{bail, Context, Result};
use eframe::egui;

use crate::download;
use crate::mi::profile::{apply_profile, RegionProfile};
use crate::mi::{DeviceInfo, MiClient};
use crate::sideload::sideload_zip_with_progress;
use crate::usb::{MiAssistantDevice, UsbTransport};
use crate::util::logging::{init_logger, LogVerbosity};
use crate::util::md5::md5_file;
use crate::validate;

// ─── Constants ───────────────────────────────────────────────────────────────

const CURRENT_DEVICE: &str = "Current device";
const DEMO_ROM_PATH: &str = "demo-recovery-rom.zip";
const DEMO_ROM_FILENAME: &str = "demo-hyperos-recovery-rom.zip";
const PROFILES: [(&str, &str); 9] = [
    (CURRENT_DEVICE, CURRENT_DEVICE),
    ("Global", "global"),
    ("EEA (Europe)", "eea"),
    ("India", "in"),
    ("Russia", "ru"),
    ("Indonesia", "id"),
    ("Turkey", "tr"),
    ("Taiwan", "tw"),
    ("China", "cn"),
];

// ─── Palette ─────────────────────────────────────────────────────────────────

struct Palette;
impl Palette {
    fn dark() -> bool {
        DARK_MODE.load(Ordering::Relaxed)
    }
    fn bg_base() -> egui::Color32 {
        if Self::dark() {
            egui::Color32::BLACK
        } else {
            egui::Color32::WHITE
        }
    }
    fn bg_panel() -> egui::Color32 {
        if Self::dark() {
            egui::Color32::from_gray(8)
        } else {
            egui::Color32::WHITE
        }
    }
    fn bg_card() -> egui::Color32 {
        if Self::dark() {
            egui::Color32::from_gray(22)
        } else {
            egui::Color32::from_gray(245)
        }
    }
    fn bg_field() -> egui::Color32 {
        if Self::dark() {
            egui::Color32::from_gray(13)
        } else {
            egui::Color32::from_gray(250)
        }
    }
    fn bg_drop() -> egui::Color32 {
        if Self::dark() {
            egui::Color32::from_rgb(48, 28, 12)
        } else {
            egui::Color32::from_rgb(255, 243, 235)
        }
    }
    fn border_subtle() -> egui::Color32 {
        if Self::dark() {
            egui::Color32::from_gray(58)
        } else {
            egui::Color32::from_gray(210)
        }
    }
    fn border_active() -> egui::Color32 {
        egui::Color32::from_rgb(255, 105, 0)
    }
    fn border_drop() -> egui::Color32 {
        egui::Color32::from_rgb(255, 105, 0)
    }
    fn text_primary() -> egui::Color32 {
        if Self::dark() {
            egui::Color32::WHITE
        } else {
            egui::Color32::BLACK
        }
    }
    fn text_secondary() -> egui::Color32 {
        if Self::dark() {
            egui::Color32::from_gray(190)
        } else {
            egui::Color32::from_gray(68)
        }
    }
    fn text_muted() -> egui::Color32 {
        if Self::dark() {
            egui::Color32::from_gray(145)
        } else {
            egui::Color32::from_gray(100)
        }
    }
    fn accent() -> egui::Color32 {
        egui::Color32::from_rgb(255, 105, 0)
    }
    fn accent_dim() -> egui::Color32 {
        if Self::dark() {
            egui::Color32::from_rgb(92, 42, 8)
        } else {
            egui::Color32::from_rgb(255, 224, 204)
        }
    }
    fn success() -> egui::Color32 {
        egui::Color32::from_rgb(25, 135, 84)
    }
    fn success_dim() -> egui::Color32 {
        if Self::dark() {
            egui::Color32::from_rgb(14, 55, 35)
        } else {
            egui::Color32::from_rgb(226, 246, 235)
        }
    }
    fn warning() -> egui::Color32 {
        egui::Color32::from_rgb(185, 105, 0)
    }
    fn warning_dim() -> egui::Color32 {
        if Self::dark() {
            egui::Color32::from_rgb(65, 40, 8)
        } else {
            egui::Color32::from_rgb(255, 242, 214)
        }
    }
    fn error() -> egui::Color32 {
        egui::Color32::from_rgb(205, 45, 38)
    }
    fn error_dim() -> egui::Color32 {
        if Self::dark() {
            egui::Color32::from_rgb(68, 16, 14)
        } else {
            egui::Color32::from_rgb(255, 235, 233)
        }
    }
    fn step_active() -> egui::Color32 {
        Self::accent()
    }
    fn step_done() -> egui::Color32 {
        Self::success()
    }
    fn step_locked() -> egui::Color32 {
        if Self::dark() {
            egui::Color32::from_gray(48)
        } else {
            egui::Color32::from_gray(205)
        }
    }
    fn step_done_bg() -> egui::Color32 {
        if Self::dark() {
            egui::Color32::from_rgb(17, 48, 32)
        } else {
            egui::Color32::from_rgb(232, 247, 238)
        }
    }
    fn step_locked_bg() -> egui::Color32 {
        if Self::dark() {
            egui::Color32::from_gray(15)
        } else {
            egui::Color32::from_gray(242)
        }
    }
}

static DARK_MODE: AtomicBool = AtomicBool::new(true);

// ─── Data types ───────────────────────────────────────────────────────────────

#[derive(Clone)]
struct FlashConfig {
    device_index: usize,
    rom_path: PathBuf,
    profile: String,
    codename: String,
    force_wipe: bool,
    wipe_confirmed: bool,
}

#[derive(Clone)]
struct DeviceRequestConfig {
    device_index: usize,
    profile: String,
    codename: String,
}

#[derive(Clone)]
struct DownloadedRom {
    path: PathBuf,
    filename: String,
}

impl FlashConfig {
    fn key(&self) -> String {
        format!(
            "{}|{}|{}|{}|{}",
            self.device_index,
            self.rom_path.display(),
            self.profile,
            self.codename,
            self.force_wipe
        )
    }
}

#[derive(Clone)]
struct ValidationReport {
    key: String,
    message: String,
    requires_wipe: bool,
    allowed_count: Option<usize>,
}

enum WorkerEvent {
    Device(Result<DeviceInfo, String>),
    UsbDevices(Result<Vec<MiAssistantDevice>, String>),
    Download(Result<DownloadedRom, String>),
    Validation(Result<ValidationReport, String>),
    Progress { sent: u64, total: u64 },
    Flash(Result<(), String>),
}

// ─── App entry point ─────────────────────────────────────────────────────────

pub fn run(demo_enabled: bool, theme_override: ThemeOverride) -> eframe::Result {
    init_logger(LogVerbosity::Normal);
    let mut viewport = egui::ViewportBuilder::default()
        .with_inner_size([900.0, 720.0])
        .with_min_inner_size([440.0, 420.0])
        .with_app_id("dev.hasx.sensitivity");
    if let Ok(icon) =
        eframe::icon_data::from_png_bytes(include_bytes!("../assets/sensitivity-icon.png"))
    {
        viewport = viewport.with_icon(icon);
    }
    let native_options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };
    eframe::run_native(
        "Sensitivity",
        native_options,
        Box::new(move |creation_context| {
            configure_theme(&creation_context.egui_ctx, theme_override);
            let mut app = SensitivityApp::new(demo_enabled);
            app.logo = Some(load_logo(&creation_context.egui_ctx));
            Ok(Box::new(app))
        }),
    )
}

fn configure_theme(ctx: &egui::Context, theme_override: ThemeOverride) {
    let preference = match theme_override {
        ThemeOverride::System => egui::ThemePreference::System,
        ThemeOverride::Light => egui::ThemePreference::Light,
        ThemeOverride::Dark => egui::ThemePreference::Dark,
    };
    ctx.set_theme(preference);
    apply_theme(ctx, theme_override);

    ctx.global_style_mut(|style| {
        style.spacing.item_spacing = egui::vec2(10.0, 8.0);
        style.spacing.button_padding = egui::vec2(16.0, 9.0);
        style.spacing.window_margin = egui::Margin::same(0);
    });
}

fn apply_theme(ctx: &egui::Context, theme_override: ThemeOverride) {
    let native_theme = match theme_override {
        ThemeOverride::Dark => egui::SystemTheme::Dark,
        ThemeOverride::Light => egui::SystemTheme::Light,
        ThemeOverride::System => egui::SystemTheme::SystemDefault,
    };
    ctx.send_viewport_cmd(egui::ViewportCommand::SetTheme(native_theme));

    let dark = match theme_override {
        ThemeOverride::Dark => true,
        ThemeOverride::Light => false,
        ThemeOverride::System => match ctx.system_theme() {
            Some(egui::Theme::Dark) => true,
            Some(egui::Theme::Light) => false,
            None => DARK_MODE.load(Ordering::Relaxed),
        },
    };
    if DARK_MODE.swap(dark, Ordering::Relaxed) == dark {
        return;
    }

    let mut visuals = if dark {
        egui::Visuals::dark()
    } else {
        egui::Visuals::light()
    };
    visuals.panel_fill = Palette::bg_base();
    visuals.window_fill = Palette::bg_panel();
    visuals.faint_bg_color = Palette::bg_card();
    visuals.extreme_bg_color = Palette::bg_field();
    visuals.code_bg_color = Palette::bg_field();
    visuals.selection.bg_fill = Palette::accent_dim();
    visuals.selection.stroke = egui::Stroke::new(1.0, Palette::accent());
    visuals.hyperlink_color = Palette::accent();

    visuals.widgets.noninteractive.corner_radius = egui::CornerRadius::same(10);
    visuals.widgets.inactive.corner_radius = egui::CornerRadius::same(10);
    visuals.widgets.hovered.corner_radius = egui::CornerRadius::same(10);
    visuals.widgets.active.corner_radius = egui::CornerRadius::same(10);

    visuals.widgets.noninteractive.bg_fill = Palette::bg_card();
    visuals.widgets.inactive.bg_fill = Palette::bg_card();
    visuals.widgets.active.bg_fill = Palette::accent_dim();

    visuals.widgets.noninteractive.bg_stroke = egui::Stroke::new(1.0, Palette::border_subtle());
    visuals.widgets.inactive.bg_stroke = egui::Stroke::new(1.0, Palette::border_subtle());
    visuals.widgets.hovered.bg_stroke = egui::Stroke::new(1.0, Palette::border_active());
    visuals.widgets.active.bg_stroke = egui::Stroke::new(1.5, Palette::accent());

    visuals.override_text_color = Some(Palette::text_primary());
    ctx.set_visuals(visuals);
}

// ─── App state ───────────────────────────────────────────────────────────────

#[derive(PartialEq, Clone, Copy)]
enum RomSource {
    Local,
    Download,
}

#[derive(PartialEq, Clone, Copy)]
pub enum ThemeOverride {
    System,
    Light,
    Dark,
}

struct SensitivityApp {
    device_index: usize,
    rom_path: String,
    rom_source: RomSource,
    profile: String,
    codename: String,
    download_dir: String,
    force_wipe: bool,
    acknowledge_wipe: bool,
    device: Option<DeviceInfo>,
    device_collapsed: bool,
    available_devices: Vec<MiAssistantDevice>,
    device_scan_completed: bool,
    demo_enabled: bool,
    demo_mode: bool,
    theme_override: ThemeOverride,
    logo: Option<egui::TextureHandle>,
    validation: Option<ValidationReport>,
    downloaded_rom: Option<DownloadedRom>,
    status: String,
    error: Option<String>,
    worker: Option<Receiver<WorkerEvent>>,
    operation: Option<&'static str>,
    progress: Option<(u64, u64)>,
    flash_done: bool,
}

impl Default for SensitivityApp {
    fn default() -> Self {
        Self {
            device_index: 0,
            rom_path: String::new(),
            rom_source: RomSource::Download,
            profile: CURRENT_DEVICE.to_owned(),
            codename: String::new(),
            download_dir: default_download_dir().display().to_string(),
            force_wipe: false,
            acknowledge_wipe: false,
            device: None,
            device_collapsed: false,
            available_devices: Vec::new(),
            device_scan_completed: false,
            demo_enabled: false,
            demo_mode: false,
            theme_override: ThemeOverride::System,
            logo: None,
            validation: None,
            downloaded_rom: None,
            status: String::new(),
            error: None,
            worker: None,
            operation: None,
            progress: None,
            flash_done: false,
        }
    }
}

impl SensitivityApp {
    fn new(demo_enabled: bool) -> Self {
        Self {
            demo_enabled,
            ..Self::default()
        }
    }

    fn busy(&self) -> bool {
        self.worker.is_some()
    }

    fn has_rom(&self) -> bool {
        !self.rom_path.trim().is_empty()
    }

    fn config_key_valid(&self) -> bool {
        self.config()
            .ok()
            .and_then(|c| self.validation.as_ref().filter(|r| r.key == c.key()))
            .is_some()
    }

    fn request_config(&self) -> Result<DeviceRequestConfig> {
        let profile = self.profile.clone();
        if profile != CURRENT_DEVICE && profile.parse::<RegionProfile>().is_err() {
            bail!("Unsupported region profile: {profile}");
        }
        Ok(DeviceRequestConfig {
            device_index: self.device_index,
            profile,
            codename: self.codename.trim().to_owned(),
        })
    }

    fn config(&self) -> Result<FlashConfig> {
        let request = self.request_config()?;
        let rom_path = PathBuf::from(self.rom_path.trim());
        if self.rom_path.trim().is_empty() || (!self.demo_mode && !rom_path.is_file()) {
            bail!("Select a valid Recovery ROM zip file first.");
        }
        Ok(FlashConfig {
            device_index: request.device_index,
            rom_path,
            profile: request.profile,
            codename: request.codename,
            force_wipe: self.force_wipe,
            wipe_confirmed: self.acknowledge_wipe,
        })
    }

    fn validated_config(&self) -> Result<FlashConfig> {
        let config = self.config()?;
        match &self.validation {
            Some(report) if report.key == config.key() => Ok(config),
            _ => bail!(
                "Validate this ROM before flashing. Re-run validation if you changed the file or region."
            ),
        }
    }

    fn start_detect(&mut self) {
        if self.demo_mode {
            self.status = "Demo device is already connected.".to_owned();
            return;
        }
        let (tx, rx) = mpsc::channel();
        let index = self.device_index;
        self.error = None;
        self.operation = Some("Detecting device");
        self.worker = Some(rx);
        std::thread::spawn(move || {
            let result = read_device(index).map_err(format_error);
            let _ = tx.send(WorkerEvent::Device(result));
        });
    }

    fn start_device_scan(&mut self) {
        if self.busy() || self.demo_mode {
            return;
        }
        let (tx, rx) = mpsc::channel();
        self.error = None;
        self.operation = Some("Looking for Recovery phones");
        self.worker = Some(rx);
        std::thread::spawn(move || {
            let result = UsbTransport::list_mi_assistant_devices().map_err(format_error);
            let _ = tx.send(WorkerEvent::UsbDevices(result));
        });
    }

    fn stop_adb_server(&mut self) {
        if self.busy() || self.demo_mode {
            return;
        }
        self.error = None;
        match crate::util::adb_server::stop_for_usb(Duration::from_secs(2)) {
            Ok(true) => {
                self.status = "Sensitivity stopped the ADB server. Retry Find Recovery phones now."
                    .to_owned();
            }
            Ok(false) => {
                self.status =
                    "No ADB server was running. Retry Find Recovery phones now.".to_owned();
            }
            Err(error) => self.error = Some(format!("Could not stop the ADB server: {error:#}")),
        }
    }

    fn enter_demo_mode(&mut self) {
        if !self.demo_enabled {
            return;
        }
        self.demo_mode = true;
        self.device = Some(demo_device());
        self.device_collapsed = false;
        self.available_devices.clear();
        self.device_scan_completed = false;
        self.rom_path.clear();
        self.rom_source = RomSource::Download;
        self.validation = None;
        self.downloaded_rom = None;
        self.acknowledge_wipe = false;
        self.flash_done = false;
        self.error = None;
        self.status =
            "Demo mode is active. No phone, ROM, network request, or flash is used.".to_owned();
    }

    fn exit_demo_mode(&mut self) {
        self.reset();
    }

    fn reset(&mut self) {
        let logo = self.logo.clone();
        *self = Self::new(self.demo_enabled);
        self.logo = logo;
    }

    fn start_download(&mut self) {
        if self.demo_mode {
            self.rom_path = DEMO_ROM_PATH.to_owned();
            self.rom_source = RomSource::Download;
            self.validation = None;
            self.acknowledge_wipe = false;
            self.flash_done = false;
            self.downloaded_rom = Some(DownloadedRom {
                path: PathBuf::from(DEMO_ROM_PATH),
                filename: DEMO_ROM_FILENAME.to_owned(),
            });
            self.status = "Demo Recovery ROM prepared. No download was performed.".to_owned();
            return;
        }
        let config = match self.request_config() {
            Ok(c) => c,
            Err(e) => {
                self.error = Some(e.to_string());
                return;
            }
        };
        if self.download_dir.trim().is_empty() {
            self.error = Some("Choose a folder to save the ROM into.".to_owned());
            return;
        }
        let output_dir = PathBuf::from(self.download_dir.trim());
        let (tx, rx) = mpsc::channel();
        self.error = None;
        self.progress = Some((0, 0));
        self.operation = Some("Downloading ROM");
        self.worker = Some(rx);
        std::thread::spawn(move || {
            let ptx = tx.clone();
            let result = download_latest_rom(&config, &output_dir, move |received, total| {
                let _ = ptx.send(WorkerEvent::Progress {
                    sent: received,
                    total: total.unwrap_or(0),
                });
            })
            .map_err(format_error);
            let _ = tx.send(WorkerEvent::Download(result));
        });
    }

    fn start_validation(&mut self) {
        let config = match self.config() {
            Ok(c) => c,
            Err(e) => {
                self.error = Some(e.to_string());
                return;
            }
        };
        if self.demo_mode {
            self.error = None;
            self.acknowledge_wipe = false;
            self.validation = Some(ValidationReport {
                key: config.key(),
                message: "Demo ROM approved. This is a local simulation.".to_owned(),
                requires_wipe: false,
                allowed_count: Some(1),
            });
            self.status = "Demo validation complete. Ready to test the final screen.".to_owned();
            return;
        }
        let (tx, rx) = mpsc::channel();
        self.error = None;
        self.validation = None;
        self.acknowledge_wipe = false;
        self.operation = Some("Validating");
        self.worker = Some(rx);
        std::thread::spawn(move || {
            let result = validate_rom(&config).map_err(format_error);
            let _ = tx.send(WorkerEvent::Validation(result));
        });
    }

    fn start_flash(&mut self) {
        let config = match self.validated_config() {
            Ok(c) => c,
            Err(e) => {
                self.error = Some(e.to_string());
                return;
            }
        };
        let requires_wipe = self
            .validation
            .as_ref()
            .is_some_and(|r| r.requires_wipe || config.force_wipe);
        if requires_wipe && !self.acknowledge_wipe {
            self.error = Some("Confirm the data-wipe warning before flashing.".to_owned());
            return;
        }
        if self.demo_mode {
            self.flash_done = true;
            self.progress = Some((1, 1));
            self.status =
                "Demo flash complete. No phone was contacted and no data changed.".to_owned();
            return;
        }
        let (tx, rx) = mpsc::channel();
        self.error = None;
        self.progress = Some((0, 0));
        self.operation = Some("Flashing");
        self.worker = Some(rx);
        std::thread::spawn(move || {
            let ptx = tx.clone();
            let result = flash_rom(&config, move |sent, total| {
                let _ = ptx.send(WorkerEvent::Progress { sent, total });
            })
            .map_err(format_error);
            let _ = tx.send(WorkerEvent::Flash(result));
        });
    }

    fn poll_worker(&mut self, ctx: &egui::Context) {
        let mut events = Vec::new();
        let mut disconnected = false;
        if let Some(receiver) = &self.worker {
            loop {
                match receiver.try_recv() {
                    Ok(e) => events.push(e),
                    Err(mpsc::TryRecvError::Empty) => break,
                    Err(mpsc::TryRecvError::Disconnected) => {
                        disconnected = true;
                        break;
                    }
                }
            }
        }
        for event in events {
            match event {
                WorkerEvent::Device(result) => match result {
                    Ok(info) => {
                        self.device_collapsed = false;
                        self.device = Some(info);
                        self.status = "Device connected.".to_owned();
                    }
                    Err(e) => self.error = Some(e),
                },
                WorkerEvent::UsbDevices(result) => match result {
                    Ok(devices) => {
                        self.available_devices = devices;
                        self.device_scan_completed = true;
                        self.status = if self.available_devices.is_empty() {
                            "No Mi Assistant Recovery interfaces found.".to_owned()
                        } else {
                            format!(
                                "Sensitivity found {} Mi Assistant Recovery interface(s).",
                                self.available_devices.len()
                            )
                        };
                    }
                    Err(e) => self.error = Some(e),
                },
                WorkerEvent::Download(result) => match result {
                    Ok(dl) => {
                        self.rom_path = dl.path.display().to_string();
                        self.validation = None;
                        self.acknowledge_wipe = false;
                        self.downloaded_rom = Some(dl);
                        self.progress = None;
                        self.status = "ROM downloaded and verified.".to_owned();
                    }
                    Err(e) => {
                        self.error = Some(e);
                        self.progress = None;
                    }
                },
                WorkerEvent::Validation(result) => match result {
                    Ok(report) => {
                        self.status = if report.requires_wipe || self.force_wipe {
                            "ROM approved. Acknowledge the data wipe to continue.".to_owned()
                        } else {
                            "ROM approved. Ready to flash.".to_owned()
                        };
                        self.validation = Some(report);
                    }
                    Err(e) => self.error = Some(e),
                },
                WorkerEvent::Progress { sent, total } => {
                    self.progress = Some((sent, total));
                    if total > 0 {
                        self.status = format!(
                            "Sending {} / {}...",
                            format_bytes(sent),
                            format_bytes(total)
                        );
                    }
                }
                WorkerEvent::Flash(result) => match result {
                    Ok(()) => {
                        self.flash_done = true;
                        self.progress = Some((1, 1));
                        self.status = "Flash complete. Check your device screen.".to_owned();
                    }
                    Err(e) => {
                        self.error = Some(e);
                        self.progress = None;
                    }
                },
            }
        }
        if disconnected {
            self.worker = None;
            self.operation = None;
        }
        if self.busy() {
            ctx.request_repaint_after(Duration::from_millis(80));
        }
    }

    fn collect_dropped_file(&mut self, ctx: &egui::Context) {
        let dropped = ctx.input(|i| i.raw.dropped_files.clone());
        if let Some(path) = dropped
            .first()
            .map(|f| f.path().to_path_buf())
            .filter(|p| !p.as_os_str().is_empty())
        {
            self.rom_path = path.display().to_string();
            self.rom_source = RomSource::Local;
            self.validation = None;
            self.acknowledge_wipe = false;
            self.downloaded_rom = None;
            self.flash_done = false;
        }
    }
}

// ─── App rendering ───────────────────────────────────────────────────────────

impl eframe::App for SensitivityApp {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        apply_theme(ctx, self.theme_override);
        self.collect_dropped_file(ctx);
        self.poll_worker(ctx);
    }

    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        render_error_toast(self, ui.ctx());

        // ── Scrollable body ──────────────────────────────────────────────────
        egui::ScrollArea::vertical()
            .auto_shrink([true, false])
            .show(ui, |ui| {
                let available_width = ui.available_width();
                let side_padding = (available_width * 0.045).clamp(14.0, 44.0);
                let content_width = (available_width - side_padding * 2.0).min(960.0);
                let center_offset = ((available_width - content_width) * 0.5).max(0.0);
                ui.horizontal(|ui| {
                    ui.add_space(center_offset);
                    ui.allocate_ui_with_layout(
                        egui::vec2(content_width, 0.0),
                        egui::Layout::top_down(egui::Align::LEFT),
                        |ui| {
                            ui.add_space(18.0);
                            render_header(self, ui);
                            ui.add_space(16.0);
                            render_steps(self, ui, frame);
                            ui.add_space(16.0);
                            render_status_footer(self, ui);
                            ui.add_space(12.0);
                        },
                    );
                });
            });
    }
}

// ─── Header ──────────────────────────────────────────────────────────────────

fn render_header(app: &mut SensitivityApp, ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        paint_logo(ui, 42.0, app.logo.as_ref());
        ui.add_space(10.0);
        ui.vertical(|ui| {
            ui.add_space(2.0);
            ui.label(
                egui::RichText::new("Sensitivity")
                    .size(19.0)
                    .color(Palette::text_primary())
                    .strong(),
            );
            ui.label(
                egui::RichText::new("Xiaomi Recovery ROM installer")
                    .size(12.0)
                    .color(Palette::text_secondary()),
            );
        });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if app.demo_enabled && app.demo_mode {
                if ui.button("Exit demo").clicked() {
                    app.exit_demo_mode();
                }
                status_pill(ui, StatusKind::Success, "Demo mode");
            } else if app.demo_enabled && ui.button("Try demo mode").clicked() {
                app.enter_demo_mode();
            }
        });
    });
}

// ─── Steps ───────────────────────────────────────────────────────────────────

fn render_steps(app: &mut SensitivityApp, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
    let step1_done = app.device.is_some();
    let step2_done = app.has_rom();
    let step3_done = app.config_key_valid();
    let step4_done = app.flash_done;

    let busy = app.busy();

    ui.add_space(2.0);

    // ── Step 1: Connect ──────────────────────────────────────────────────────
    let state1 = step_state(step1_done, true);
    step_card(ui, 1, "Connect your device", state1, |ui| {
        render_device_picker(app, ui, busy);
        ui.add_space(8.0);

        if step1_done {
            // Collapsed summary + expand toggle
            if let Some(info) = &app.device.clone() {
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(format!("  {}   |   {}", info.device, info.region))
                            .color(Palette::success())
                            .size(13.0),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let expand_label = if app.device_collapsed {
                            "Show details"
                        } else {
                            "Hide details"
                        };
                        if ui.small_button(expand_label).clicked() {
                            app.device_collapsed = !app.device_collapsed;
                        }
                        if ui
                            .add_enabled(!busy, egui::Button::new("Re-detect").small())
                            .clicked()
                        {
                            app.start_detect();
                        }
                    });
                });

                if !app.device_collapsed {
                    ui.add_space(8.0);
                    egui::Grid::new("device_info")
                        .num_columns(4)
                        .spacing([20.0, 5.0])
                        .show(ui, |ui| {
                            info_kv(ui, "Model", &info.device);
                            info_kv(ui, "Region", &info.region);
                            ui.end_row();
                            info_kv(ui, "OS version", &info.version);
                            info_kv(ui, "Branch", &info.branch);
                            ui.end_row();
                            info_kv(ui, "Serial", &info.sn);
                            info_kv(ui, "ROM zone", &info.romzone);
                            ui.end_row();
                        });
                }
            }
        } else {
            // Instruction + detect button
            inline_hint(ui, "Put your Xiaomi device in Recovery mode, then select  Connect with Mi Assistant  on the device screen.");
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                let detecting = app.operation == Some("Detecting device");
                let btn = fat_button("Detect device", Palette::accent(), detecting || !busy);
                if ui.add_enabled(!busy, btn).clicked() {
                    app.start_detect();
                }
                if detecting {
                    ui.spinner();
                }
                ui.add_space(12.0);
                ui.label(
                    egui::RichText::new("USB device #")
                        .size(12.0)
                        .color(Palette::text_muted()),
                );
                ui.add(egui::DragValue::new(&mut app.device_index).range(0..=8));
            });
        }
    });

    ui.add_space(6.0);

    // ── Step 2: Select ROM ───────────────────────────────────────────────────
    let state2 = step_state(step2_done, step1_done);
    step_card(ui, 2, "Select a Recovery ROM", state2, |ui| {
        if !step1_done {
            ui.disable();
        }

        if !step1_done {
            ui.label(
                egui::RichText::new("Complete step 1 first.")
                    .size(13.0)
                    .color(Palette::text_muted())
                    .italics(),
            );
            return;
        }

        // Source tabs
        ui.horizontal(|ui| {
            tab_btn(
                ui,
                "Download latest",
                app.rom_source == RomSource::Download,
                || {
                    app.rom_source = RomSource::Download;
                },
            );
            tab_btn(
                ui,
                "Use local file",
                app.rom_source == RomSource::Local,
                || {
                    app.rom_source = RomSource::Local;
                },
            );
        });

        ui.add_space(10.0);

        match app.rom_source {
            RomSource::Download => {
                // Download strip
                ui.horizontal_wrapped(|ui| {
                    ui.label(
                        egui::RichText::new("Save to")
                            .size(12.5)
                            .color(Palette::text_secondary()),
                    );
                    ui.add(
                        egui::TextEdit::singleline(&mut app.download_dir)
                            .hint_text("Downloads/Sensitivity")
                            .desired_width(260.0)
                            .font(egui::TextStyle::Monospace),
                    );
                });
                ui.add_space(8.0);

                let is_downloading = app.operation == Some("Downloading ROM");
                ui.horizontal(|ui| {
                    let btn = fat_button("Download latest ROM", Palette::accent(), !busy);
                    if ui.add_enabled(!busy, btn).clicked() {
                        app.start_download();
                    }
                    if is_downloading {
                        ui.spinner();
                        if let Some((sent, total)) = app.progress {
                            if total > 0 {
                                let frac = sent as f32 / total as f32;
                                ui.add(
                                    egui::ProgressBar::new(frac)
                                        .desired_width(160.0)
                                        .animate(true),
                                );
                            } else {
                                ui.add(
                                    egui::ProgressBar::new(0.0)
                                        .desired_width(120.0)
                                        .animate(true),
                                );
                            }
                        }
                    }
                });

                if let Some(dl) = &app.downloaded_rom {
                    ui.add_space(8.0);
                    status_pill(
                        ui,
                        StatusKind::Success,
                        &format!("{}  |  MD5 verified", dl.filename),
                    );
                } else {
                    ui.add_space(6.0);
                    ui.label(
                        egui::RichText::new("Fetches the latest ROM Xiaomi reports for your device and verifies its MD5 before it becomes selectable.")
                            .size(11.5)
                            .color(Palette::text_muted()),
                    );
                }
            }
            RomSource::Local => {
                // Drop zone
                let has_file = !app.rom_path.trim().is_empty();
                drop_zone(ui, has_file, &app.rom_path.clone(), |new_path| {
                    app.rom_path = new_path;
                    app.validation = None;
                    app.acknowledge_wipe = false;
                    app.downloaded_rom = None;
                    app.flash_done = false;
                });

                // Or paste path
                ui.add_space(6.0);
                let prev = app.rom_path.clone();
                ui.add(
                    egui::TextEdit::singleline(&mut app.rom_path)
                        .hint_text("Or paste a full path here...")
                        .desired_width(f32::INFINITY)
                        .font(egui::TextStyle::Monospace),
                );
                if app.rom_path != prev {
                    app.validation = None;
                    app.acknowledge_wipe = false;
                    app.flash_done = false;
                }
            }
        }

        // Region & codename (shown for both sources when a file is selected)
        if step2_done {
            ui.add_space(10.0);
            ui.add(egui::Separator::default().spacing(1.0));
            ui.add_space(8.0);

            ui.horizontal_wrapped(|ui| {
                ui.label(
                    egui::RichText::new("Region profile")
                        .size(12.5)
                        .color(Palette::text_secondary()),
                );
                egui::ComboBox::from_id_salt("profile")
                    .selected_text(profile_label(&app.profile))
                    .width(160.0)
                    .show_ui(ui, |ui| {
                        for (label, value) in PROFILES {
                            if ui
                                .selectable_value(&mut app.profile, value.to_owned(), label)
                                .changed()
                            {
                                app.validation = None;
                                app.flash_done = false;
                            }
                        }
                    });

                ui.add_space(8.0);
                ui.label(
                    egui::RichText::new("Codename")
                        .size(12.5)
                        .color(Palette::text_secondary()),
                );
                let prev_cn = app.codename.clone();
                ui.add(
                    egui::TextEdit::singleline(&mut app.codename)
                        .hint_text("e.g. garnet  (optional)")
                        .desired_width(150.0)
                        .font(egui::TextStyle::Monospace),
                );
                if app.codename != prev_cn {
                    app.validation = None;
                }
            });
            ui.add_space(4.0);
            ui.label(
                egui::RichText::new("Match the profile to your ROM's region. Leave as \"Current device\" if unsure.")
                    .size(11.5)
                    .color(Palette::text_muted()),
            );

            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.checkbox(&mut app.force_wipe, "");
                ui.label(egui::RichText::new("Allow data wipe").size(13.0).color(
                    if app.force_wipe {
                        Palette::warning()
                    } else {
                        Palette::text_secondary()
                    },
                ));
                ui.label(
                    egui::RichText::new("  Required for cross-region flashing. Erases all data.")
                        .size(11.5)
                        .color(Palette::text_muted()),
                );
            });
        }
    });

    ui.add_space(6.0);

    // ── Step 3: Validate ─────────────────────────────────────────────────────
    let state3 = step_state(step3_done, step1_done && step2_done);
    step_card(ui, 3, "Validate with Xiaomi", state3, |ui| {
        let prereqs = step1_done && step2_done;
        if !prereqs {
            ui.disable();
        }

        if !prereqs {
            ui.label(
                egui::RichText::new("Complete steps 1 and 2 first.")
                    .size(13.0)
                    .color(Palette::text_muted())
                    .italics(),
            );
            return;
        }

        let validating = app.operation == Some("Validating");
        if step3_done {
            if let Some(report) = &app.validation.clone() {
                let wipe_needed = report.requires_wipe || app.force_wipe;
                let kind = if wipe_needed {
                    StatusKind::Warning
                } else {
                    StatusKind::Success
                };
                let icon = if wipe_needed { "Warning" } else { "Approved" };
                status_pill(ui, kind, &format!("{}  {}", icon, report.message));
                if let Some(count) = report.allowed_count {
                    ui.add_space(2.0);
                    ui.label(
                        egui::RichText::new(format!("{count} compatible ROM(s) listed by Xiaomi."))
                            .size(12.0)
                            .color(Palette::text_muted()),
                    );
                }
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(!busy, egui::Button::new("Re-validate").small())
                        .clicked()
                    {
                        app.start_validation();
                    }
                });
            }
        } else {
            inline_hint(ui, "Sensitivity sends an encrypted request to Xiaomi to confirm this ROM is authorized for your device. No data leaves your machine beyond device identity and the ROM's MD5.");
            ui.add_space(10.0);

            ui.horizontal(|ui| {
                let btn = fat_button("Validate ROM", Palette::accent(), !busy);
                if ui.add_enabled(!busy, btn).clicked() {
                    app.start_validation();
                }
                if validating {
                    ui.spinner();
                    ui.label(
                        egui::RichText::new("Contacting Xiaomi...")
                            .size(12.0)
                            .color(Palette::text_secondary()),
                    );
                }
            });
        }
    });

    ui.add_space(6.0);

    // ── Step 4: Flash ────────────────────────────────────────────────────────
    let state4 = step_state(step4_done, step3_done);
    step_card(ui, 4, "Flash to device", state4, |ui| {
        if !step3_done {
            ui.disable();
        }

        if !step3_done {
            ui.label(
                egui::RichText::new("Complete validation first.")
                    .size(13.0)
                    .color(Palette::text_muted())
                    .italics(),
            );
            return;
        }

        if step4_done {
            status_pill(
                ui,
                StatusKind::Success,
                "Flash complete. Check your device screen for the recovery result.",
            );
            ui.add_space(8.0);
            if ui.button("Start over").clicked() {
                app.reset();
            }
            return;
        }

        // Wipe acknowledgement
        if let Some(report) = &app.validation.clone() {
            let wipe_needed = report.requires_wipe || app.force_wipe;
            if wipe_needed {
                egui::Frame::new()
                    .fill(Palette::warning_dim())
                    .stroke(egui::Stroke::new(1.0, Palette::warning()))
                    .inner_margin(egui::Margin::symmetric(12, 10))
                    .corner_radius(6.0)
                    .show(ui, |ui| {
                        ui.horizontal_wrapped(|ui| {
                            ui.label(
                                egui::RichText::new("Data wipe required.")
                                    .color(Palette::warning())
                                    .size(13.5)
                                    .strong(),
                            );
                            ui.label(
                                egui::RichText::new("This ROM will erase all data on the device.")
                                    .size(13.0)
                                    .color(Palette::warning()),
                            );
                        });
                        ui.add_space(6.0);
                        ui.horizontal(|ui| {
                            ui.checkbox(&mut app.acknowledge_wipe, "");
                            ui.label(
                                egui::RichText::new(
                                    "I understand this will erase all data on the device.",
                                )
                                .size(13.0)
                                .color(Palette::text_primary()),
                            );
                        });
                    });
                ui.add_space(10.0);
            }
        }

        let wipe_needed = app
            .validation
            .as_ref()
            .is_some_and(|r| r.requires_wipe || app.force_wipe);
        let wipe_ok = !wipe_needed || app.acknowledge_wipe;
        let can_flash = !busy && wipe_ok;

        let flashing = app.operation == Some("Flashing");

        ui.horizontal(|ui| {
            let btn = fat_button(
                "Flash ROM",
                if can_flash {
                    Palette::accent()
                } else {
                    Palette::border_subtle()
                },
                can_flash,
            );
            if ui.add_enabled(can_flash, btn).clicked() {
                app.start_flash();
            }
            if flashing {
                ui.spinner();
                if let Some((sent, total)) = app.progress {
                    if total > 0 {
                        ui.add(
                            egui::ProgressBar::new(sent as f32 / total as f32)
                                .desired_width(200.0)
                                .animate(true),
                        );
                    } else {
                        ui.add(
                            egui::ProgressBar::new(0.0)
                                .desired_width(120.0)
                                .animate(true),
                        );
                    }
                }
            }
        });

        if !wipe_ok {
            ui.add_space(4.0);
            ui.label(
                egui::RichText::new("Acknowledge the wipe warning above before flashing.")
                    .size(12.0)
                    .color(Palette::warning()),
            );
        } else if !busy {
            ui.add_space(4.0);
            ui.label(
                egui::RichText::new(
                    "Keep the device connected and don't touch it during flashing.",
                )
                .size(12.0)
                .color(Palette::text_muted()),
            );
        }
    });
}

fn render_device_picker(app: &mut SensitivityApp, ui: &mut egui::Ui, busy: bool) {
    ui.horizontal_wrapped(|ui| {
        let scan_label = if app.device_scan_completed {
            "Refresh available phones"
        } else {
            "Find Recovery phones"
        };
        if ui
            .add_enabled(
                !busy && !app.demo_mode,
                egui::Button::new(scan_label).small(),
            )
            .clicked()
        {
            app.start_device_scan();
        }

        if ui
            .add_enabled(
                !busy && !app.demo_mode,
                egui::Button::new("Stop ADB server").small(),
            )
            .on_hover_text("Release the USB interface if adb.exe is holding it")
            .clicked()
        {
            app.stop_adb_server();
        }

        if app.demo_mode {
            ui.label(
                egui::RichText::new("Demo device is simulated locally")
                    .size(11.5)
                    .color(Palette::text_muted()),
            );
        } else if app.device_scan_completed {
            if app.available_devices.is_empty() {
                ui.label(
                    egui::RichText::new("No Mi Assistant Recovery phone found")
                        .size(11.5)
                        .color(Palette::text_muted()),
                );
            } else {
                let selected = app
                    .available_devices
                    .iter()
                    .find(|device| device.index == app.device_index)
                    .map(MiAssistantDevice::label)
                    .unwrap_or_else(|| "Choose a Recovery phone".to_owned());
                egui::ComboBox::from_id_salt("recovery_phone")
                    .selected_text(selected)
                    .width(330.0)
                    .show_ui(ui, |ui| {
                        for device in &app.available_devices {
                            ui.selectable_value(
                                &mut app.device_index,
                                device.index,
                                device.label(),
                            );
                        }
                    });
            }
        } else {
            ui.label(
                egui::RichText::new("Lists only phones in Mi Assistant Recovery mode")
                    .size(11.5)
                    .color(Palette::text_muted()),
            );
        }
    });
}

// ─── Status bar ──────────────────────────────────────────────────────────────

fn render_status_footer(app: &SensitivityApp, ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        if let Some(op) = app.operation {
            ui.spinner();
            ui.add_space(4.0);
            ui.label(egui::RichText::new(op).color(Palette::accent()).size(12.5));
            ui.add_space(8.0);
        }
        if !app.status.is_empty() {
            ui.label(
                egui::RichText::new(&app.status)
                    .size(12.5)
                    .color(Palette::text_secondary()),
            );
        }
    });
}

fn render_error_toast(app: &mut SensitivityApp, ctx: &egui::Context) {
    let Some(error) = app.error.clone() else {
        return;
    };

    let mut dismiss = false;
    egui::Area::new(egui::Id::new("error_toast"))
        .order(egui::Order::Foreground)
        .anchor(egui::Align2::RIGHT_TOP, egui::vec2(-18.0, 42.0))
        .show(ctx, |ui| {
            ui.set_max_width(440.0);
            egui::Frame::new()
                .fill(Palette::error_dim())
                .stroke(egui::Stroke::new(1.0, Palette::error()))
                .inner_margin(egui::Margin::symmetric(12, 10))
                .corner_radius(12.0)
                .show(ui, |ui| {
                    ui.horizontal_top(|ui| {
                        paint_logo(ui, 20.0, app.logo.as_ref());
                        ui.add_space(3.0);
                        ui.vertical(|ui| {
                            ui.label(
                                egui::RichText::new("Could not connect to the device")
                                    .color(Palette::error())
                                    .size(13.0)
                                    .strong(),
                            );
                            ui.label(
                                egui::RichText::new(error)
                                    .color(Palette::text_primary())
                                    .size(12.0),
                            );
                        });
                        let (close_rect, close_response) =
                            ui.allocate_exact_size(egui::vec2(20.0, 20.0), egui::Sense::click());
                        let close_fill = if close_response.hovered() {
                            Palette::error().linear_multiply(0.22)
                        } else {
                            Palette::error().linear_multiply(0.12)
                        };
                        ui.painter()
                            .circle_filled(close_rect.center(), 10.0, close_fill);
                        ui.painter().text(
                            close_rect.center(),
                            egui::Align2::CENTER_CENTER,
                            "x",
                            egui::FontId::proportional(14.0),
                            Palette::error(),
                        );
                        if close_response.on_hover_text("Dismiss").clicked() {
                            dismiss = true;
                        }
                    });
                });
        });

    if dismiss {
        app.error = None;
    }
}

// ─── Widget helpers ───────────────────────────────────────────────────────────

/// Step completion state
#[derive(Clone, Copy, PartialEq)]
enum StepState {
    Done,
    Active,
    Locked,
}

fn step_state(done: bool, unlocked: bool) -> StepState {
    if done {
        StepState::Done
    } else if unlocked {
        StepState::Active
    } else {
        StepState::Locked
    }
}

/// Draws a step card with a left accent bar.
fn step_card(
    ui: &mut egui::Ui,
    number: u8,
    title: &str,
    state: StepState,
    content: impl FnOnce(&mut egui::Ui),
) {
    let bar_color = match state {
        StepState::Done => Palette::step_done(),
        StepState::Active => Palette::step_active(),
        StepState::Locked => Palette::step_locked(),
    };
    let bg = match state {
        StepState::Done => Palette::step_done_bg(),
        StepState::Active => Palette::bg_panel(),
        StepState::Locked => Palette::step_locked_bg(),
    };
    let content_width = (ui.available_width() - 36.0).max(0.0);

    let card = egui::Frame::new()
        .fill(bg)
        .stroke(egui::Stroke::new(1.0, Palette::border_subtle()))
        .inner_margin(egui::Margin::symmetric(18, 16))
        .corner_radius(14.0)
        .show(ui, |ui| {
            ui.set_min_width(content_width);

            // Header row
            ui.horizontal(|ui| {
                // Badge
                let badge_text = match state {
                    StepState::Done => "Done".to_owned(),
                    _ => number.to_string(),
                };
                let badge_color = bar_color;
                let (badge_rect, _) =
                    ui.allocate_exact_size(egui::vec2(20.0, 20.0), egui::Sense::hover());
                ui.painter()
                    .rect_filled(badge_rect, 10.0, badge_color.linear_multiply(0.15));
                ui.painter().text(
                    badge_rect.center(),
                    egui::Align2::CENTER_CENTER,
                    &badge_text,
                    egui::FontId::proportional(11.0),
                    badge_color,
                );
                ui.add_space(6.0);
                ui.label(
                    egui::RichText::new(title)
                        .size(14.5)
                        .color(match state {
                            StepState::Locked => Palette::text_muted(),
                            _ => Palette::text_primary(),
                        })
                        .strong(),
                );
            });

            // Content
            ui.add_space(8.0);
            content(ui);
        });

    let accent_rect = egui::Rect::from_min_max(
        egui::pos2(
            card.response.rect.left() + 2.0,
            card.response.rect.top() + 10.0,
        ),
        egui::pos2(
            card.response.rect.left() + 5.0,
            card.response.rect.bottom() - 10.0,
        ),
    );
    ui.painter().rect_filled(accent_rect, 2.0, bar_color);
}

/// A visually distinct drag-drop zone
fn drop_zone(ui: &mut egui::Ui, has_file: bool, current_path: &str, on_set: impl FnOnce(String)) {
    let is_hovering =
        ui.input(|i| !i.raw.dropped_files.is_empty() || !i.raw.hovered_files.is_empty());

    let (border_color, bg_color) = if is_hovering {
        (Palette::accent(), Palette::accent_dim())
    } else if has_file {
        (Palette::success(), Palette::success_dim())
    } else {
        (Palette::border_drop(), Palette::bg_drop())
    };

    egui::Frame::new()
        .fill(bg_color)
        .stroke(egui::Stroke::new(1.5, border_color))
        .inner_margin(egui::Margin::symmetric(16, 18))
        .corner_radius(14.0)
        .show(ui, |ui| {
            if has_file {
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new("Verified")
                            .color(Palette::success())
                            .size(16.0),
                    );
                    ui.add_space(6.0);
                    let name = PathBuf::from(current_path)
                        .file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_else(|| current_path.to_owned());
                    ui.label(
                        egui::RichText::new(name)
                            .size(13.0)
                            .color(Palette::text_primary())
                            .monospace(),
                    );
                });
            } else {
                ui.vertical_centered(|ui| {
                    ui.label(
                        egui::RichText::new("Drop a .zip file here")
                            .size(14.0)
                            .color(Palette::text_secondary()),
                    );
                    ui.add_space(2.0);
                    ui.label(
                        egui::RichText::new("recovery-rom.zip")
                            .size(11.5)
                            .color(Palette::text_muted())
                            .monospace(),
                    );
                });
            }
        });

    // Handle the drop event here (the actual data is consumed in collect_dropped_file globally)
    let _ = (on_set, is_hovering);
}

/// Fat primary action button
fn fat_button(label: &str, fill: egui::Color32, _active: bool) -> egui::Button<'_> {
    egui::Button::new(
        egui::RichText::new(label)
            .size(13.5)
            .color(Palette::text_primary()),
    )
    .fill(fill)
    .stroke(egui::Stroke::NONE)
    .min_size(egui::vec2(138.0, 36.0))
}

/// Inline info-style hint
fn inline_hint(ui: &mut egui::Ui, text: &str) {
    ui.horizontal_wrapped(|ui| {
        ui.label(
            egui::RichText::new("Info")
                .size(13.0)
                .color(Palette::text_muted()),
        );
        ui.add_space(4.0);
        ui.label(
            egui::RichText::new(text)
                .size(12.5)
                .color(Palette::text_muted()),
        );
    });
}

/// Status pill (success / warning / error)
#[derive(Clone, Copy)]
#[allow(dead_code)]
enum StatusKind {
    Success,
    Warning,
    Error,
}

fn status_pill(ui: &mut egui::Ui, kind: StatusKind, text: &str) {
    let (fg, bg) = match kind {
        StatusKind::Success => (Palette::success(), Palette::success_dim()),
        StatusKind::Warning => (Palette::warning(), Palette::warning_dim()),
        StatusKind::Error => (Palette::error(), Palette::error_dim()),
    };
    egui::Frame::new()
        .fill(bg)
        .stroke(egui::Stroke::new(1.0, fg))
        .inner_margin(egui::Margin::symmetric(10, 6))
        .corner_radius(10.0)
        .show(ui, |ui| {
            ui.label(egui::RichText::new(text).size(13.0).color(fg));
        });
}

/// Tab-style toggle button
fn tab_btn(ui: &mut egui::Ui, label: &str, active: bool, on_click: impl FnOnce()) {
    let (fill, stroke, text_color) = if active {
        (
            Palette::accent_dim(),
            egui::Stroke::new(1.0, Palette::accent()),
            Palette::accent(),
        )
    } else {
        (
            Palette::bg_card(),
            egui::Stroke::new(1.0, Palette::border_subtle()),
            Palette::text_secondary(),
        )
    };
    let btn = egui::Button::new(egui::RichText::new(label).size(13.0).color(text_color))
        .fill(fill)
        .stroke(stroke)
        .min_size(egui::vec2(160.0, 36.0));
    if ui.add(btn).clicked() && !active {
        on_click();
    }
}

/// 2-col key-value pair in a grid
fn info_kv(ui: &mut egui::Ui, label: &str, value: &str) {
    ui.label(
        egui::RichText::new(label)
            .size(12.0)
            .color(Palette::text_secondary()),
    );
    ui.label(
        egui::RichText::new(value)
            .size(12.5)
            .color(Palette::text_primary())
            .monospace(),
    );
}

/// Profile label lookup
fn profile_label(value: &str) -> &str {
    PROFILES
        .iter()
        .find_map(|(l, p)| (*p == value).then_some(*l))
        .unwrap_or(CURRENT_DEVICE)
}

// ─── Logo ─────────────────────────────────────────────────────────────────────

fn load_logo(ctx: &egui::Context) -> egui::TextureHandle {
    let icon = eframe::icon_data::from_png_bytes(include_bytes!("../assets/sensitivity-icon.png"))
        .expect("embedded Sensitivity icon must be a valid PNG");
    let image = egui::ColorImage::from_rgba_unmultiplied(
        [icon.width as usize, icon.height as usize],
        &icon.rgba,
    );
    ctx.load_texture("sensitivity-icon", image, egui::TextureOptions::LINEAR)
}

fn paint_logo(ui: &mut egui::Ui, size: f32, texture: Option<&egui::TextureHandle>) {
    if let Some(texture) = texture {
        ui.add(
            egui::Image::from_texture(texture)
                .fit_to_exact_size(egui::vec2(size, size))
                .corner_radius(egui::CornerRadius::same((size * 0.18) as u8)),
        );
    } else {
        let (rect, _) = ui.allocate_exact_size(egui::vec2(size, size), egui::Sense::hover());
        ui.painter()
            .rect_filled(rect, size * 0.18, Palette::accent());
    }
}

// ─── Worker functions ─────────────────────────────────────────────────────────

fn demo_device() -> DeviceInfo {
    DeviceInfo {
        device: "2312DRA50G".to_owned(),
        sn: "DEMO-DEVICE-ONLY".to_owned(),
        version: "OS3.0.0.0.UNCCNXM".to_owned(),
        codebase: "demo".to_owned(),
        branch: "stable".to_owned(),
        language: "en".to_owned(),
        region: "Global".to_owned(),
        romzone: "global".to_owned(),
    }
}

fn read_device(device_index: usize) -> Result<DeviceInfo> {
    let transport = UsbTransport::open(device_index, false)
        .context("Opening the Mi Assistant USB interface. If Windows reports Access denied, click Stop ADB server, then retry. Is the device in Recovery -> Connect with Mi Assistant?")?;
    let mut client = MiClient::new(transport).context("Connecting to the device over ADB")?;
    client.read_all_info().context("Reading device information")
}

fn device_info_for_request(config: &DeviceRequestConfig) -> Result<DeviceInfo> {
    let info = read_device(config.device_index)?;
    if config.profile == CURRENT_DEVICE {
        return Ok(info);
    }
    let profile = config
        .profile
        .parse::<RegionProfile>()
        .map_err(|_| anyhow::anyhow!("Unknown region profile: {}", config.profile))?;
    let codename = (!config.codename.is_empty()).then_some(config.codename.as_str());
    apply_profile(&info, profile, codename, true).context("Applying the region profile")
}

fn device_info_for_config(config: &FlashConfig) -> Result<DeviceInfo> {
    device_info_for_request(&DeviceRequestConfig {
        device_index: config.device_index,
        profile: config.profile.clone(),
        codename: config.codename.clone(),
    })
}

fn download_latest_rom<F>(
    config: &DeviceRequestConfig,
    output_dir: &std::path::Path,
    on_progress: F,
) -> Result<DownloadedRom>
where
    F: FnMut(u64, Option<u64>),
{
    fs::create_dir_all(output_dir)
        .with_context(|| format!("Creating download folder {}", output_dir.display()))?;
    let info = device_info_for_request(config)?;
    let request =
        validate::build_request_json(&info, None).context("Building latest-ROM request")?;
    let response = validate::validate("https://update.miui.com/updates/miotaV3.php", &request)
        .context("Requesting the latest official Recovery ROM from Xiaomi")?;
    let json = response
        .full_json
        .ok_or_else(|| anyhow::anyhow!("Xiaomi did not return a ROM download link."))?;
    let (latest, mirrors) =
        download::parse_latest_from_json(&json).context("Parsing Xiaomi's ROM response")?;
    let client = download::official_download_client()?;
    let path = download::download_from_https_mirrors_with_md5(
        &client,
        &mirrors,
        &latest,
        output_dir,
        on_progress,
    )
    .context("Downloading and verifying the ROM")?;
    Ok(DownloadedRom {
        path,
        filename: latest.filename,
    })
}

fn default_download_dir() -> PathBuf {
    dirs_next::data_local_dir()
        .or_else(dirs_next::download_dir)
        .unwrap_or_else(std::env::temp_dir)
        .join("Sensitivity")
        .join("roms")
}

fn validate_rom(config: &FlashConfig) -> Result<ValidationReport> {
    let info = device_info_for_config(config)?;
    let md5 = md5_file(&config.rom_path).context("Computing ROM checksum")?;
    let request =
        validate::build_request_json(&info, Some(md5)).context("Building validation request")?;
    let response = validate::validate("https://update.miui.com/updates/miotaV3.php", &request)
        .context("Checking ROM with Xiaomi")?;
    let token = response
        .validate_token
        .as_deref()
        .filter(|t| !t.is_empty())
        .ok_or_else(|| {
            anyhow::anyhow!(
                "Xiaomi did not approve this ROM. No token returned. \
             Try a different region profile or download the latest official ROM."
            )
        })?;
    let _ = token;
    Ok(ValidationReport {
        key: config.key(),
        message: response
            .code_message
            .unwrap_or_else(|| "Approved by Xiaomi.".to_owned()),
        requires_wipe: response.pkgrom_erase == Some(1),
        allowed_count: response.pkgrom_validate.as_ref().map(Vec::len),
    })
}

fn flash_rom<F>(config: &FlashConfig, on_progress: F) -> Result<()>
where
    F: FnMut(u64, u64),
{
    let info = device_info_for_config(config)?;
    let md5 = md5_file(&config.rom_path).context("Computing ROM checksum")?;
    let request =
        validate::build_request_json(&info, Some(md5)).context("Building validation request")?;
    let response = validate::validate("https://update.miui.com/updates/miotaV3.php", &request)
        .context("Re-validating ROM before flash")?;
    let token = response
        .validate_token
        .as_deref()
        .filter(|t| !t.is_empty())
        .ok_or_else(|| anyhow::anyhow!("Xiaomi did not approve this ROM. Cannot flash."))?;
    let allow_wipe = response.pkgrom_erase == Some(1) || config.force_wipe;
    if allow_wipe && !config.wipe_confirmed {
        bail!("This ROM requires a data wipe. Acknowledge the warning before flashing.");
    }
    let transport = UsbTransport::open(config.device_index, false)
        .context("Re-opening the Mi Assistant USB interface")?;
    let mut client = MiClient::new(transport).context("Re-connecting to the device")?;
    sideload_zip_with_progress(
        &mut client,
        &config.rom_path,
        65_536,
        token,
        allow_wipe,
        on_progress,
    )
    .context("Sideload failed")
}

fn format_error(e: anyhow::Error) -> String {
    format!("{e:#}")
}

fn format_bytes(value: u64) -> String {
    const UNITS: [&str; 4] = ["B", "KiB", "MiB", "GiB"];
    let mut amount = value as f64;
    let mut unit = 0;
    while amount >= 1024.0 && unit < UNITS.len() - 1 {
        amount /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{value} {}", UNITS[unit])
    } else {
        format!("{amount:.1} {}", UNITS[unit])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn demo_flow_is_offline_and_reaches_completion() {
        let mut app = SensitivityApp::new(true);
        app.enter_demo_mode();
        assert!(app.device.is_some());
        assert!(!app.has_rom());

        app.start_download();
        assert_eq!(app.rom_path, DEMO_ROM_PATH);
        assert!(app.downloaded_rom.is_some());

        app.start_validation();
        assert!(app.config_key_valid());

        app.start_flash();
        assert!(app.flash_done);
        assert!(app.status.contains("No phone was contacted"));
    }

    #[test]
    fn demo_mode_cannot_be_enabled_without_the_startup_flag() {
        let mut app = SensitivityApp::default();
        app.enter_demo_mode();
        assert!(!app.demo_mode);
        assert!(app.device.is_none());
    }
}
