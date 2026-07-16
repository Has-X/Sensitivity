// Copyright (C) 2025 HasX
// Licensed under the GNU AGPL v3.0. See LICENSE file for details.

#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::thread;

use eframe::egui;
use sensitivity::mi::{DeviceInfo, MiClient};
use sensitivity::usb::{UsbDeviceInfo, UsbTransport};
use sensitivity::{sideload, util, validate};

const SERVER_URL: &str = "https://update.miui.com/updates/miotaV3.php";

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1040.0, 720.0])
            .with_min_inner_size([820.0, 560.0]),
        ..Default::default()
    };
    eframe::run_native(
        "Sensitivity",
        options,
        Box::new(|creation_context| Ok(Box::new(SensitivityApp::new(creation_context)))),
    )
}

#[derive(Debug)]
enum Message {
    Status(String),
    Error(String),
    DeviceInfo(DeviceInfo),
    Roms(String),
    Validated {
        path: PathBuf,
        token: String,
        erase: bool,
        md5: String,
    },
    Progress {
        sent: u64,
        total: u64,
    },
    Finished(String),
}

#[derive(Debug, Clone)]
struct ValidatedRom {
    path: PathBuf,
    token: String,
    erase: bool,
    md5: String,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
enum Language {
    #[default]
    En,
    Es,
}

#[derive(Default, serde::Serialize, serde::Deserialize)]
struct PersistedState {
    rom_path: Option<PathBuf>,
    stop_adb: bool,
    language: Language,
}

struct SensitivityApp {
    devices: Vec<UsbDeviceInfo>,
    selected_device: usize,
    device_info: Option<DeviceInfo>,
    rom_path: Option<PathBuf>,
    validated: Option<ValidatedRom>,
    rom_listing: String,
    logs: Vec<String>,
    status: String,
    progress: Option<(u64, u64)>,
    busy: bool,
    stop_adb: bool,
    language: Language,
    receiver: Option<Receiver<Message>>,
    cancel: Option<Arc<AtomicBool>>,
    confirm_flash: bool,
    confirm_format: bool,
}

impl SensitivityApp {
    fn new(context: &eframe::CreationContext<'_>) -> Self {
        let persisted = context
            .storage
            .and_then(|storage| storage.get_string("sensitivity.state"))
            .and_then(|json| serde_json::from_str::<PersistedState>(&json).ok())
            .unwrap_or_default();
        let mut app = Self {
            devices: Vec::new(),
            selected_device: 0,
            device_info: None,
            rom_path: persisted.rom_path,
            validated: None,
            rom_listing: String::new(),
            logs: Vec::new(),
            status: "Connect a phone in Mi Assistant recovery mode.".into(),
            progress: None,
            busy: false,
            stop_adb: persisted.stop_adb,
            language: persisted.language,
            receiver: None,
            cancel: None,
            confirm_flash: false,
            confirm_format: false,
        };
        app.refresh_devices();
        app
    }

    fn log(&mut self, message: impl Into<String>) {
        self.logs.push(message.into());
        if self.logs.len() > 400 {
            self.logs.drain(..self.logs.len() - 400);
        }
    }

    fn t(&self, english: &'static str) -> &'static str {
        if self.language == Language::En {
            return english;
        }
        match english {
            "Xiaomi Recovery flash and rescue" => "Flasheo y rescate de Xiaomi Recovery",
            "Recovery device" => "Dispositivo en recovery",
            "Refresh USB devices" => "Actualizar dispositivos USB",
            "No matching interface found." => "No se encontró una interfaz compatible.",
            "Interface" => "Interfaz",
            "Stop local ADB before opening USB" => "Detener ADB local antes de abrir USB",
            "Read device info" => "Leer información del dispositivo",
            "List allowed ROMs" => "Listar ROMs permitidas",
            "Official Recovery ROM" => "ROM Recovery oficial",
            "Choose ROM ZIP" => "Elegir ZIP de ROM",
            "Validate ROM" => "Validar ROM",
            "Validation requires a data wipe" => "La validación requiere borrar los datos",
            "Validated; no wipe requested" => "Validada; no se solicita borrado",
            "Flash validated ROM" => "Flashear ROM validada",
            "Cancel flash" => "Cancelar flasheo",
            "Recovery actions" => "Acciones de recovery",
            "Reboot" => "Reiniciar",
            "Erase data" => "Borrar datos",
            "Device information" => "Información del dispositivo",
            "Read device info to begin." => "Lee la información del dispositivo para comenzar.",
            "Allowed ROM response" => "Respuesta de ROMs permitidas",
            "No response loaded." => "No se ha cargado una respuesta.",
            "Activity" => "Actividad",
            "Confirm flash" => "Confirmar flasheo",
            "Xiaomi requires this flash to permanently erase user data." => {
                "Xiaomi requiere que este flasheo borre permanentemente los datos."
            }
            "Flash the validated official ROM now?" => "¿Flashear ahora la ROM oficial validada?",
            "Keep the phone connected until recovery reports completion." => {
                "Mantén el teléfono conectado hasta que recovery indique que terminó."
            }
            "Cancel" => "Cancelar",
            "Erase data and flash" => "Borrar datos y flashear",
            "Flash" => "Flashear",
            "Confirm data erase" => "Confirmar borrado de datos",
            "This permanently erases all user data, then reboots the phone." => {
                "Esto borra permanentemente todos los datos y reinicia el teléfono."
            }
            "Erase all data" => "Borrar todos los datos",
            _ => english,
        }
    }

    fn refresh_devices(&mut self) {
        match UsbTransport::discover() {
            Ok(devices) => {
                self.devices = devices;
                if self.selected_device >= self.devices.len() {
                    self.selected_device = 0;
                }
                self.status = match self.devices.len() {
                    0 => "No Mi Assistant recovery interface found.".into(),
                    1 => "One recovery interface found. Read device info to continue.".into(),
                    count => format!("{count} recovery interfaces found. Select one to continue."),
                };
            }
            Err(error) => {
                self.devices.clear();
                self.status = "USB discovery failed.".into();
                self.log(format!("USB discovery failed: {error:#}"));
            }
        }
    }

    fn start_task<F>(&mut self, status: impl Into<String>, task: F)
    where
        F: FnOnce(Sender<Message>) + Send + 'static,
    {
        if self.busy {
            return;
        }
        let (sender, receiver) = mpsc::channel();
        self.receiver = Some(receiver);
        self.busy = true;
        self.status = status.into();
        self.progress = None;
        thread::spawn(move || task(sender));
    }

    fn open_client(index: usize, stop_adb: bool) -> anyhow::Result<MiClient> {
        if stop_adb && util::adb_server::is_running(std::time::Duration::from_millis(200)) {
            util::adb_server::kill_adb_server(std::time::Duration::from_secs(2))?;
        }
        let transport = UsbTransport::open(index, false)?;
        MiClient::new(transport)
    }

    fn read_device_info(&mut self) {
        let index = self.selected_device;
        let stop_adb = self.stop_adb;
        self.start_task("Reading device information...", move |sender| {
            let result =
                Self::open_client(index, stop_adb).and_then(|mut client| client.read_all_info());
            match result {
                Ok(info) => {
                    let _ = sender.send(Message::DeviceInfo(info));
                    let _ = sender.send(Message::Status("Device is ready.".into()));
                }
                Err(error) => {
                    let _ = sender.send(Message::Error(format!("{error:#}")));
                }
            }
        });
    }

    fn list_roms(&mut self) {
        let Some(info) = self.device_info.clone() else {
            self.log("Read device info before querying ROMs.");
            return;
        };
        self.start_task("Querying Xiaomi validation service...", move |sender| {
            let result = (|| -> anyhow::Result<String> {
                let request = validate::build_request_json(&info, None)?;
                let response = validate::validate(SERVER_URL, &request)?;
                Ok(response.full_json.unwrap_or_else(|| {
                    response
                        .code_message
                        .unwrap_or_else(|| "No ROM listing returned.".into())
                }))
            })();
            match result {
                Ok(listing) => {
                    let pretty = serde_json::from_str::<serde_json::Value>(&listing)
                        .and_then(|value| serde_json::to_string_pretty(&value))
                        .unwrap_or(listing);
                    let _ = sender.send(Message::Roms(pretty));
                    let _ = sender.send(Message::Status("ROM query complete.".into()));
                }
                Err(error) => {
                    let _ = sender.send(Message::Error(format!("ROM query failed: {error:#}")));
                }
            }
        });
    }

    fn choose_rom(&mut self) {
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("Recovery ROM", &["zip"])
            .pick_file()
        {
            self.rom_path = Some(path);
            self.validated = None;
            self.status = "ROM selected. Validate it before flashing.".into();
        }
    }

    fn validate_rom(&mut self) {
        let Some(info) = self.device_info.clone() else {
            self.log("Read device info before validating a ROM.");
            return;
        };
        let Some(path) = self.rom_path.clone() else {
            self.log("Choose an official Recovery ROM ZIP first.");
            return;
        };
        self.start_task("Hashing and validating ROM...", move |sender| {
            let result = (|| -> anyhow::Result<ValidatedRom> {
                let md5 = sensitivity::util::md5::md5_file(&path)?;
                let request = validate::build_request_json(&info, Some(md5.clone()))?;
                let response = validate::validate(SERVER_URL, &request)?;
                let token = response
                    .validate_token
                    .filter(|token| !token.is_empty())
                    .ok_or_else(|| anyhow::anyhow!("Xiaomi did not return a validation token"))?;
                Ok(ValidatedRom {
                    path,
                    token,
                    erase: response.pkgrom_erase == Some(1),
                    md5,
                })
            })();
            match result {
                Ok(validated) => {
                    let _ = sender.send(Message::Validated {
                        path: validated.path,
                        token: validated.token,
                        erase: validated.erase,
                        md5: validated.md5,
                    });
                    let _ =
                        sender.send(Message::Status("ROM validated and ready to flash.".into()));
                }
                Err(error) => {
                    let _ = sender.send(Message::Error(format!("Validation failed: {error:#}")));
                }
            }
        });
    }

    fn request_flash(&mut self) {
        if self.validated.is_none() {
            self.log("Validate the selected ROM before flashing.");
            return;
        }
        self.confirm_flash = true;
    }

    fn start_flash(&mut self) {
        let Some(validated) = self.validated.clone() else {
            return;
        };
        let index = self.selected_device;
        let stop_adb = self.stop_adb;
        let cancel = Arc::new(AtomicBool::new(false));
        self.cancel = Some(Arc::clone(&cancel));
        self.confirm_flash = false;
        self.start_task(
            "Flashing ROM. Do not disconnect the phone...",
            move |sender| {
                let result = (|| -> anyhow::Result<()> {
                    let mut client = Self::open_client(index, stop_adb)?;
                    sideload::sideload_zip_with_progress(
                        &mut client,
                        &validated.path,
                        64 * 1024,
                        &validated.token,
                        validated.erase,
                        &cancel,
                        |sent, total| {
                            let _ = sender.send(Message::Progress { sent, total });
                        },
                    )
                })();
                match result {
                    Ok(()) => {
                        let _ = sender.send(Message::Finished("Flash completed.".into()));
                    }
                    Err(error) => {
                        let _ = sender.send(Message::Error(format!("Flash failed: {error:#}")));
                    }
                }
            },
        );
    }

    fn send_recovery_command(&mut self, command: &'static str, success: &'static str) {
        let index = self.selected_device;
        let stop_adb = self.stop_adb;
        self.start_task(format!("Sending {command}..."), move |sender| {
            let result = Self::open_client(index, stop_adb)
                .and_then(|mut client| client.simple_command(command));
            match result {
                Ok(()) => {
                    let _ = sender.send(Message::Finished(success.into()));
                }
                Err(error) => {
                    let _ = sender.send(Message::Error(format!("Command failed: {error:#}")));
                }
            }
        });
    }

    fn format_data(&mut self) {
        let index = self.selected_device;
        let stop_adb = self.stop_adb;
        self.confirm_format = false;
        self.start_task("Erasing user data...", move |sender| {
            let result = (|| -> anyhow::Result<()> {
                let mut client = Self::open_client(index, stop_adb)?;
                client.simple_command("format-data:")?;
                client.simple_command("reboot:")?;
                Ok(())
            })();
            match result {
                Ok(()) => {
                    let _ = sender.send(Message::Finished("Data erased; reboot requested.".into()));
                }
                Err(error) => {
                    let _ = sender.send(Message::Error(format!("Format failed: {error:#}")));
                }
            }
        });
    }

    fn drain_messages(&mut self) {
        let messages = self
            .receiver
            .as_ref()
            .map(|receiver| receiver.try_iter().collect::<Vec<_>>())
            .unwrap_or_default();
        for message in messages {
            match message {
                Message::Status(status) => self.status = status,
                Message::Error(error) => {
                    self.status = "Operation failed.".into();
                    self.log(error);
                    self.busy = false;
                    self.cancel = None;
                }
                Message::DeviceInfo(info) => {
                    self.log(format!("Detected {} ({})", info.device, info.version));
                    self.device_info = Some(info);
                    self.validated = None;
                    self.busy = false;
                }
                Message::Roms(roms) => {
                    self.rom_listing = roms;
                    self.busy = false;
                }
                Message::Validated {
                    path,
                    token,
                    erase,
                    md5,
                } => {
                    self.log(format!("Validated {} (MD5 {md5})", path.display()));
                    if erase {
                        self.log("Xiaomi requires a data wipe for this package.");
                    }
                    self.validated = Some(ValidatedRom {
                        path,
                        token,
                        erase,
                        md5,
                    });
                    self.busy = false;
                }
                Message::Progress { sent, total } => self.progress = Some((sent, total)),
                Message::Finished(message) => {
                    self.status = message.clone();
                    self.log(message);
                    self.busy = false;
                    self.cancel = None;
                }
            }
        }
    }

    fn device_label(device: &UsbDeviceInfo) -> String {
        format!(
            "[{}] {:04x}:{:04x} bus {} address {}",
            device.index, device.vendor_id, device.product_id, device.bus, device.address
        )
    }
}

impl eframe::App for SensitivityApp {
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        let state = PersistedState {
            rom_path: self.rom_path.clone(),
            stop_adb: self.stop_adb,
            language: self.language,
        };
        if let Ok(json) = serde_json::to_string(&state) {
            storage.set_string("sensitivity.state", json);
        }
    }

    fn update(&mut self, context: &egui::Context, _frame: &mut eframe::Frame) {
        context.set_visuals(egui::Visuals::dark());
        self.drain_messages();

        egui::TopBottomPanel::top("header").show(context, |ui| {
            ui.horizontal(|ui| {
                ui.heading("Sensitivity");
                ui.separator();
                ui.label(self.t("Xiaomi Recovery flash and rescue"));
                ui.separator();
                egui::ComboBox::from_id_salt("language")
                    .selected_text(match self.language {
                        Language::En => "EN",
                        Language::Es => "ES",
                    })
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut self.language, Language::En, "EN");
                        ui.selectable_value(&mut self.language, Language::Es, "ES");
                    });
            });
            ui.label(&self.status);
        });

        egui::SidePanel::left("actions")
            .resizable(false)
            .default_width(300.0)
            .show(context, |ui| {
                ui.heading(format!("1. {}", self.t("Recovery device")));
                if ui.button(self.t("Refresh USB devices")).clicked() && !self.busy {
                    self.refresh_devices();
                }
                if self.devices.is_empty() {
                    ui.label(self.t("No matching interface found."));
                } else {
                    egui::ComboBox::from_label(self.t("Interface"))
                        .selected_text(Self::device_label(
                            self.devices
                                .get(self.selected_device)
                                .unwrap_or(&self.devices[0]),
                        ))
                        .show_ui(ui, |ui| {
                            for device in &self.devices {
                                ui.selectable_value(
                                    &mut self.selected_device,
                                    device.index,
                                    Self::device_label(device),
                                );
                            }
                        });
                }
                let stop_adb_label = self.t("Stop local ADB before opening USB");
                ui.checkbox(&mut self.stop_adb, stop_adb_label)
                    .on_hover_text("Opt in only if ADB owns the Mi Assistant interface.");
                if ui
                    .add_enabled(
                        !self.busy && !self.devices.is_empty(),
                        egui::Button::new(self.t("Read device info")),
                    )
                    .clicked()
                {
                    self.read_device_info();
                }
                if ui
                    .add_enabled(
                        !self.busy && self.device_info.is_some(),
                        egui::Button::new(self.t("List allowed ROMs")),
                    )
                    .clicked()
                {
                    self.list_roms();
                }

                ui.separator();
                ui.heading(format!("2. {}", self.t("Official Recovery ROM")));
                if ui
                    .add_enabled(!self.busy, egui::Button::new(self.t("Choose ROM ZIP")))
                    .clicked()
                {
                    self.choose_rom();
                }
                if let Some(path) = &self.rom_path {
                    ui.small(path.display().to_string());
                }
                if ui
                    .add_enabled(
                        !self.busy && self.device_info.is_some() && self.rom_path.is_some(),
                        egui::Button::new(self.t("Validate ROM")),
                    )
                    .clicked()
                {
                    self.validate_rom();
                }
                if let Some(validated) = &self.validated {
                    ui.label(format!("MD5: {}", validated.md5));
                    if validated.erase {
                        ui.colored_label(
                            egui::Color32::RED,
                            self.t("Validation requires a data wipe"),
                        );
                    } else {
                        ui.colored_label(
                            egui::Color32::LIGHT_GREEN,
                            self.t("Validated; no wipe requested"),
                        );
                    }
                }
                if ui
                    .add_enabled(
                        !self.busy && self.validated.is_some(),
                        egui::Button::new(self.t("Flash validated ROM")),
                    )
                    .clicked()
                {
                    self.request_flash();
                }
                if let Some(cancel) = &self.cancel {
                    if ui.button(self.t("Cancel flash")).clicked() {
                        cancel.store(true, Ordering::Relaxed);
                        self.status = "Cancellation requested...".into();
                    }
                }
                if let Some((sent, total)) = self.progress {
                    let fraction = if total == 0 {
                        0.0
                    } else {
                        sent as f32 / total as f32
                    };
                    ui.add(egui::ProgressBar::new(fraction.clamp(0.0, 1.0)).show_percentage());
                    ui.small(format!("{sent} / {total} bytes"));
                }

                ui.separator();
                ui.heading(self.t("Recovery actions"));
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(
                            !self.busy && !self.devices.is_empty(),
                            egui::Button::new(self.t("Reboot")),
                        )
                        .clicked()
                    {
                        self.send_recovery_command("reboot:", "Reboot requested.");
                    }
                    if ui
                        .add_enabled(
                            !self.busy && !self.devices.is_empty(),
                            egui::Button::new(self.t("Erase data")),
                        )
                        .clicked()
                    {
                        self.confirm_format = true;
                    }
                });
            });

        egui::CentralPanel::default().show(context, |ui| {
            ui.heading(self.t("Device information"));
            if let Some(info) = &self.device_info {
                egui::Grid::new("device-info").striped(true).show(ui, |ui| {
                    for (label, value) in [
                        ("Device", info.device.as_str()),
                        ("Version", info.version.as_str()),
                        ("Serial", info.sn.as_str()),
                        ("Codebase", info.codebase.as_str()),
                        ("Branch", info.branch.as_str()),
                        ("Language", info.language.as_str()),
                        ("Region", info.region.as_str()),
                        ("ROM zone", info.romzone.as_str()),
                    ] {
                        ui.strong(label);
                        ui.monospace(value);
                        ui.end_row();
                    }
                });
            } else {
                ui.label(self.t("Read device info to begin."));
            }
            ui.separator();
            ui.heading(self.t("Allowed ROM response"));
            egui::ScrollArea::vertical()
                .max_height(220.0)
                .show(ui, |ui| {
                    if self.rom_listing.is_empty() {
                        ui.label(self.t("No response loaded."));
                    } else {
                        ui.monospace(&self.rom_listing);
                    }
                });
            ui.separator();
            ui.heading(self.t("Activity"));
            egui::ScrollArea::vertical()
                .stick_to_bottom(true)
                .show(ui, |ui| {
                    for line in &self.logs {
                        ui.monospace(line);
                    }
                });
        });

        if self.confirm_flash {
            let erase = self.validated.as_ref().is_some_and(|rom| rom.erase);
            egui::Window::new(self.t("Confirm flash"))
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(context, |ui| {
                    if erase {
                        ui.colored_label(
                            egui::Color32::RED,
                            self.t("Xiaomi requires this flash to permanently erase user data."),
                        );
                    } else {
                        ui.label(self.t("Flash the validated official ROM now?"));
                    }
                    ui.label(self.t("Keep the phone connected until recovery reports completion."));
                    ui.horizontal(|ui| {
                        if ui.button(self.t("Cancel")).clicked() {
                            self.confirm_flash = false;
                        }
                        let label = if erase {
                            self.t("Erase data and flash")
                        } else {
                            self.t("Flash")
                        };
                        if ui.button(label).clicked() {
                            self.start_flash();
                        }
                    });
                });
        }

        if self.confirm_format {
            egui::Window::new(self.t("Confirm data erase"))
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(context, |ui| {
                    ui.colored_label(
                        egui::Color32::RED,
                        self.t("This permanently erases all user data, then reboots the phone."),
                    );
                    ui.horizontal(|ui| {
                        if ui.button(self.t("Cancel")).clicked() {
                            self.confirm_format = false;
                        }
                        if ui.button(self.t("Erase all data")).clicked() {
                            self.format_data();
                        }
                    });
                });
        }

        context.request_repaint_after(std::time::Duration::from_millis(50));
    }
}
