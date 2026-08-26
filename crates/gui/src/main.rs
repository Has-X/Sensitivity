// Copyright (C) 2026 Chromatic
// Licensed under the GNU AGPL v3.0. See LICENSE file for details.

#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::thread;

use eframe::egui;
use sensitivity::mi::{DeviceInfo, MiClient};
use sensitivity::usb::{UsbDeviceInfo, UsbTransport};
use sensitivity::{download, sideload, util, validate};

const SERVER_URL: &str = "https://update.miui.com/updates/miotaV3.php";

fn main() -> eframe::Result<()> {
    let app_icon =
        eframe::icon_data::from_png_bytes(include_bytes!("../../../assets/sensitivity-icon.png"))
            .expect("embedded Sensitivity icon must be a valid PNG");
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1040.0, 720.0])
            .with_min_inner_size([820.0, 560.0])
            .with_icon(app_icon),
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
    Downloaded(PathBuf),
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
enum Language {
    En,
    Hu,
    Es,
    De,
    Fr,
    It,
    Pl,
    PtBr,
    Tr,
    Id,
    Ro,
    Cs,
    Sk,
    Ru,
    Uk,
    ZhCn,
    Ar,
    Vi,
    Th,
    Hi,
    ZhTw,
    Ja,
    Ko,
    Nl,
    El,
    Bg,
    Hr,
    Sr,
    Sl,
    Sv,
    Da,
    Fi,
    Nb,
    PtPt,
}

impl Language {
    const ALL: &'static [Self] = &[
        Self::En,
        Self::Hu,
        Self::Es,
        Self::De,
        Self::Fr,
        Self::It,
        Self::Pl,
        Self::PtBr,
        Self::Tr,
        Self::Id,
        Self::Ro,
        Self::Cs,
        Self::Sk,
        Self::Ru,
        Self::Uk,
        Self::ZhCn,
        Self::Ar,
        Self::Vi,
        Self::Th,
        Self::Hi,
        Self::ZhTw,
        Self::Ja,
        Self::Ko,
        Self::Nl,
        Self::El,
        Self::Bg,
        Self::Hr,
        Self::Sr,
        Self::Sl,
        Self::Sv,
        Self::Da,
        Self::Fi,
        Self::Nb,
        Self::PtPt,
    ];

    fn code(self) -> &'static str {
        match self {
            Self::En => "en",
            Self::Hu => "hu",
            Self::Es => "es",
            Self::De => "de",
            Self::Fr => "fr",
            Self::It => "it",
            Self::Pl => "pl",
            Self::PtBr => "pt-BR",
            Self::Tr => "tr",
            Self::Id => "id",
            Self::Ro => "ro",
            Self::Cs => "cs",
            Self::Sk => "sk",
            Self::Ru => "ru",
            Self::Uk => "uk",
            Self::ZhCn => "zh-CN",
            Self::Ar => "ar",
            Self::Vi => "vi",
            Self::Th => "th",
            Self::Hi => "hi",
            Self::ZhTw => "zh-TW",
            Self::Ja => "ja",
            Self::Ko => "ko",
            Self::Nl => "nl",
            Self::El => "el",
            Self::Bg => "bg",
            Self::Hr => "hr",
            Self::Sr => "sr",
            Self::Sl => "sl",
            Self::Sv => "sv",
            Self::Da => "da",
            Self::Fi => "fi",
            Self::Nb => "nb",
            Self::PtPt => "pt-PT",
        }
    }
}

impl Default for Language {
    fn default() -> Self {
        let locale = std::env::var("LC_ALL")
            .or_else(|_| std::env::var("LANG"))
            .unwrap_or_default();
        if locale.starts_with("hu") {
            Self::Hu
        } else if locale.starts_with("es") {
            Self::Es
        } else if locale.starts_with("de") {
            Self::De
        } else if locale.starts_with("fr") {
            Self::Fr
        } else if locale.starts_with("it") {
            Self::It
        } else if locale.starts_with("pl") {
            Self::Pl
        } else if locale.starts_with("pt-pt") {
            Self::PtPt
        } else if locale.starts_with("pt") {
            Self::PtBr
        } else if locale.starts_with("tr") {
            Self::Tr
        } else if locale.starts_with("id") {
            Self::Id
        } else if locale.starts_with("ro") {
            Self::Ro
        } else if locale.starts_with("cs") {
            Self::Cs
        } else if locale.starts_with("sk") {
            Self::Sk
        } else if locale.starts_with("ru") {
            Self::Ru
        } else if locale.starts_with("uk") {
            Self::Uk
        } else if locale.starts_with("zh-tw") {
            Self::ZhTw
        } else if locale.starts_with("zh") {
            Self::ZhCn
        } else if locale.starts_with("ar") {
            Self::Ar
        } else if locale.starts_with("vi") {
            Self::Vi
        } else if locale.starts_with("th") {
            Self::Th
        } else if locale.starts_with("hi") {
            Self::Hi
        } else if locale.starts_with("ja") {
            Self::Ja
        } else if locale.starts_with("ko") {
            Self::Ko
        } else if locale.starts_with("nl") {
            Self::Nl
        } else if locale.starts_with("el") {
            Self::El
        } else if locale.starts_with("bg") {
            Self::Bg
        } else if locale.starts_with("hr") {
            Self::Hr
        } else if locale.starts_with("sr") {
            Self::Sr
        } else if locale.starts_with("sl") {
            Self::Sl
        } else if locale.starts_with("sv") {
            Self::Sv
        } else if locale.starts_with("da") {
            Self::Da
        } else if locale.starts_with("fi") {
            Self::Fi
        } else if locale.starts_with("nb") {
            Self::Nb
        } else {
            Self::En
        }
    }
}

fn load_catalog(language: Language) -> HashMap<String, String> {
    let source = match language {
        Language::En => include_str!("../../../locales/en/gui.json"),
        Language::Hu => include_str!("../../../locales/hu/gui.json"),
        Language::Es => include_str!("../../../locales/es/gui.json"),
        Language::De => include_str!("../../../locales/de/gui.json"),
        Language::Fr => include_str!("../../../locales/fr/gui.json"),
        Language::It => include_str!("../../../locales/it/gui.json"),
        Language::Pl => include_str!("../../../locales/pl/gui.json"),
        Language::PtBr => include_str!("../../../locales/pt-BR/gui.json"),
        Language::Tr => include_str!("../../../locales/tr/gui.json"),
        Language::Id => include_str!("../../../locales/id/gui.json"),
        Language::Ro => include_str!("../../../locales/ro/gui.json"),
        Language::Cs => include_str!("../../../locales/cs/gui.json"),
        Language::Sk => include_str!("../../../locales/sk/gui.json"),
        Language::Ru => include_str!("../../../locales/ru/gui.json"),
        Language::Uk => include_str!("../../../locales/uk/gui.json"),
        Language::ZhCn => include_str!("../../../locales/zh-CN/gui.json"),
        Language::Ar => include_str!("../../../locales/ar/gui.json"),
        Language::Vi => include_str!("../../../locales/vi/gui.json"),
        Language::Th => include_str!("../../../locales/th/gui.json"),
        Language::Hi => include_str!("../../../locales/hi/gui.json"),
        Language::ZhTw => include_str!("../../../locales/zh-TW/gui.json"),
        Language::Ja => include_str!("../../../locales/ja/gui.json"),
        Language::Ko => include_str!("../../../locales/ko/gui.json"),
        Language::Nl => include_str!("../../../locales/nl/gui.json"),
        Language::El => include_str!("../../../locales/el/gui.json"),
        Language::Bg => include_str!("../../../locales/bg/gui.json"),
        Language::Hr => include_str!("../../../locales/hr/gui.json"),
        Language::Sr => include_str!("../../../locales/sr/gui.json"),
        Language::Sl => include_str!("../../../locales/sl/gui.json"),
        Language::Sv => include_str!("../../../locales/sv/gui.json"),
        Language::Da => include_str!("../../../locales/da/gui.json"),
        Language::Fi => include_str!("../../../locales/fi/gui.json"),
        Language::Nb => include_str!("../../../locales/nb/gui.json"),
        Language::PtPt => include_str!("../../../locales/pt-PT/gui.json"),
    };
    let mut catalog: HashMap<String, String> = serde_json::from_str(source).unwrap_or_default();
    let aliases: HashMap<String, String> =
        serde_json::from_str(include_str!("../../../locales/_keys/gui.json")).unwrap_or_default();
    for (id, source_key) in aliases {
        let value = catalog
            .get(&source_key)
            .cloned()
            .unwrap_or_else(|| source_key.clone());
        catalog.insert(id, value);
    }
    catalog
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
    catalog: HashMap<String, String>,
    receiver: Option<Receiver<Message>>,
    cancel: Option<Arc<AtomicBool>>,
    confirm_flash: bool,
    confirm_format: bool,
    icon_texture: egui::TextureHandle,
}

impl SensitivityApp {
    fn new(context: &eframe::CreationContext<'_>) -> Self {
        let persisted = context
            .storage
            .and_then(|storage| storage.get_string("sensitivity.state"))
            .and_then(|json| serde_json::from_str::<PersistedState>(&json).ok())
            .unwrap_or_default();
        let icon = eframe::icon_data::from_png_bytes(include_bytes!(
            "../../../assets/sensitivity-icon.png"
        ))
        .expect("embedded Sensitivity icon must be a valid PNG");
        let icon_image = egui::ColorImage::from_rgba_unmultiplied(
            [icon.width as usize, icon.height as usize],
            &icon.rgba,
        );
        let icon_texture = context.egui_ctx.load_texture(
            "sensitivity-icon",
            icon_image,
            egui::TextureOptions::LINEAR,
        );
        let mut app = Self {
            devices: Vec::new(),
            selected_device: 0,
            device_info: None,
            rom_path: persisted.rom_path,
            validated: None,
            rom_listing: String::new(),
            logs: Vec::new(),
            status: String::new(),
            progress: None,
            busy: false,
            stop_adb: persisted.stop_adb,
            language: persisted.language,
            catalog: load_catalog(persisted.language),
            receiver: None,
            cancel: None,
            confirm_flash: false,
            confirm_format: false,
            icon_texture,
        };
        app.status = app.t("status.initial");
        app.refresh_devices();
        app
    }

    fn log(&mut self, message: impl Into<String>) {
        self.logs.push(message.into());
        if self.logs.len() > 400 {
            self.logs.drain(..self.logs.len() - 400);
        }
    }

    fn t(&self, key: &str) -> String {
        self.catalog
            .get(key)
            .cloned()
            .unwrap_or_else(|| key.to_owned())
    }

    fn refresh_devices(&mut self) {
        match UsbTransport::discover() {
            Ok(devices) => {
                self.devices = devices;
                if self.selected_device >= self.devices.len() {
                    self.selected_device = 0;
                }
                self.status = match self.devices.len() {
                    0 => self.t("status.no_recovery"),
                    1 => self.t("status.one_recovery"),
                    count => self
                        .t("status.recovery_count")
                        .replace("{count}", &count.to_string()),
                };
            }
            Err(error) => {
                self.devices.clear();
                self.status = self.t("status.usb_discovery_failed");
                self.log(format!(
                    "{}: {error:#}",
                    self.t("status.usb_discovery_failed")
                ));
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
        self.start_task(self.t("status.reading_device"), move |sender| {
            let result =
                Self::open_client(index, stop_adb).and_then(|mut client| client.read_all_info());
            match result {
                Ok(info) => {
                    let _ = sender.send(Message::DeviceInfo(info));
                    let _ = sender.send(Message::Status("status.device_ready".into()));
                }
                Err(error) => {
                    let _ = sender.send(Message::Error(format!("{error:#}")));
                }
            }
        });
    }

    fn list_roms(&mut self) {
        let Some(info) = self.device_info.clone() else {
            self.log(self.t("log.read_info_first_roms"));
            return;
        };
        self.start_task(self.t("status.querying_xiaomi"), move |sender| {
            let result = (|| -> anyhow::Result<String> {
                let request = validate::build_request_json(&info, None)?;
                let response = validate::validate(SERVER_URL, &request)?;
                Ok(response.full_json.unwrap_or_else(|| {
                    response
                        .code_message
                        .unwrap_or_else(|| "status.no_rom_listing".into())
                }))
            })();
            match result {
                Ok(listing) => {
                    let pretty = serde_json::from_str::<serde_json::Value>(&listing)
                        .and_then(|value| serde_json::to_string_pretty(&value))
                        .unwrap_or(listing);
                    let _ = sender.send(Message::Roms(pretty));
                    let _ = sender.send(Message::Status("status.rom_query_complete".into()));
                }
                Err(error) => {
                    let _ =
                        sender.send(Message::Error(format!("status.rom_query_failed|{error:#}")));
                }
            }
        });
    }

    fn choose_rom(&mut self) {
        if let Some(path) = rfd::FileDialog::new()
            .add_filter(self.t("section.official_rom").as_str(), &["zip"])
            .pick_file()
        {
            self.rom_path = Some(path);
            self.validated = None;
            self.status = self.t("status.rom_selected");
        }
    }

    fn download_latest(&mut self) {
        let Some(info) = self.device_info.clone() else {
            self.log(self.t("log.read_info_first_roms"));
            return;
        };
        let output_dir = std::env::var_os("HOME")
            .map(PathBuf::from)
            .map(|home| home.join("Downloads"))
            .filter(|path| path.is_dir())
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
        self.start_task(self.t("status.downloading_latest"), move |sender| {
            let result = (|| -> anyhow::Result<PathBuf> {
                let request = validate::build_request_json(&info, None)?;
                let response = validate::validate(SERVER_URL, &request)?;
                let json = response
                    .full_json
                    .ok_or_else(|| anyhow::anyhow!("No full ROM response returned"))?;
                let (latest, mirrors) = download::parse_latest_from_json(&json)?;
                let url = download::choose_url(&mirrors, &latest.filename)
                    .ok_or_else(|| anyhow::anyhow!("No HTTPS ROM mirror returned"))?;
                let client = reqwest::blocking::Client::builder()
                    .user_agent("MiTunes_UserAgent_v3.0")
                    .build()?;
                download::download_with_md5(&client, &url, &output_dir, &latest.md5)
            })();
            match result {
                Ok(path) => {
                    let _ = sender.send(Message::Downloaded(path));
                    let _ = sender.send(Message::Status("status.downloaded".into()));
                }
                Err(error) => {
                    let _ =
                        sender.send(Message::Error(format!("status.operation_failed|{error:#}")));
                }
            }
        });
    }

    fn validate_rom(&mut self) {
        let Some(info) = self.device_info.clone() else {
            self.log(self.t("log.read_info_first_validate"));
            return;
        };
        let Some(path) = self.rom_path.clone() else {
            self.log(self.t("log.choose_rom_first"));
            return;
        };
        self.start_task(self.t("status.hashing_rom"), move |sender| {
            let result = (|| -> anyhow::Result<ValidatedRom> {
                let md5 = sensitivity::util::md5::md5_file(&path)?;
                let request = validate::build_request_json(&info, Some(md5.clone()))?;
                let response = validate::validate(SERVER_URL, &request)?;
                let token = response
                    .validate_token
                    .filter(|token| !token.is_empty())
                    .ok_or_else(|| anyhow::anyhow!("error.no_validation_token"))?;
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
                    let _ = sender.send(Message::Status("status.rom_validated".into()));
                }
                Err(error) => {
                    let _ = sender.send(Message::Error(format!(
                        "status.validation_failed|{error:#}"
                    )));
                }
            }
        });
    }

    fn request_flash(&mut self) {
        if self.validated.is_none() {
            self.log(self.t("log.validate_first"));
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
        self.start_task(self.t("status.flashing_rom"), move |sender| {
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
                    let _ = sender.send(Message::Finished("status.flash_completed".into()));
                }
                Err(error) => {
                    let _ = sender.send(Message::Error(format!("status.flash_failed|{error:#}")));
                }
            }
        });
    }

    fn send_recovery_command(&mut self, command: &'static str, success: &'static str) {
        let index = self.selected_device;
        let stop_adb = self.stop_adb;
        self.start_task(
            self.t("status.sending_command")
                .replace("{command}", command),
            move |sender| {
                let result = Self::open_client(index, stop_adb)
                    .and_then(|mut client| client.simple_command(command));
                match result {
                    Ok(()) => {
                        let _ = sender.send(Message::Finished(success.into()));
                    }
                    Err(error) => {
                        let _ =
                            sender.send(Message::Error(format!("status.command_failed|{error:#}")));
                    }
                }
            },
        );
    }

    fn format_data(&mut self) {
        let index = self.selected_device;
        let stop_adb = self.stop_adb;
        self.confirm_format = false;
        self.start_task(self.t("status.erasing_data"), move |sender| {
            let result = (|| -> anyhow::Result<()> {
                let mut client = Self::open_client(index, stop_adb)?;
                client.simple_command("format-data:")?;
                client.simple_command("reboot:")?;
                Ok(())
            })();
            match result {
                Ok(()) => {
                    let _ = sender.send(Message::Finished("status.data_erased".into()));
                }
                Err(error) => {
                    let _ = sender.send(Message::Error(format!("status.format_failed|{error:#}")));
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
                Message::Status(status) => self.status = self.t(&status),
                Message::Error(error) => {
                    self.status = self.t("status.operation_failed");
                    let display_error = error
                        .split_once('|')
                        .map(|(key, detail)| self.t(key).replace("{error}", detail))
                        .unwrap_or(error);
                    self.log(display_error);
                    self.busy = false;
                    self.cancel = None;
                }
                Message::DeviceInfo(info) => {
                    self.log(
                        self.t("log.detected_device")
                            .replace("{device}", &info.device)
                            .replace("{version}", &info.version),
                    );
                    self.device_info = Some(info);
                    self.validated = None;
                    self.busy = false;
                }
                Message::Roms(roms) => {
                    self.rom_listing = if roms == "status.no_rom_listing" {
                        self.t("status.no_rom_listing")
                    } else {
                        roms
                    };
                    self.busy = false;
                }
                Message::Downloaded(path) => {
                    self.rom_path = Some(path.clone());
                    self.validated = None;
                    self.log(self.t("status.downloaded"));
                }
                Message::Validated {
                    path,
                    token,
                    erase,
                    md5,
                } => {
                    self.log(
                        self.t("log.validated_file")
                            .replace("{path}", &path.display().to_string())
                            .replace("{md5}", &md5),
                    );
                    if erase {
                        self.log(self.t("log.package_wipe"));
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
                    self.status = self.t(&message);
                    self.log(self.t(&message));
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

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        ui.ctx().set_visuals(egui::Visuals::dark());
        self.drain_messages();

        egui::Panel::top("header").show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.add(
                    egui::Image::from_texture(&self.icon_texture)
                        .fit_to_exact_size(egui::vec2(30.0, 30.0)),
                );
                ui.heading(self.t("app.title"));
                ui.separator();
                ui.label(self.t("app.subtitle"));
                ui.separator();
                let previous_language = self.language;
                egui::ComboBox::from_id_salt("language")
                    .selected_text(self.language.code())
                    .show_ui(ui, |ui| {
                        for language in Language::ALL {
                            ui.selectable_value(&mut self.language, *language, language.code());
                        }
                    });
                if self.language != previous_language {
                    self.catalog = load_catalog(self.language);
                }
            });
            ui.label(&self.status);
        });

        egui::Panel::left("actions")
            .resizable(false)
            .default_size(300.0)
            .show(ui, |ui| {
                ui.heading(format!("1. {}", self.t("section.recovery_device")));
                if ui.button(self.t("action.refresh_usb")).clicked() && !self.busy {
                    self.refresh_devices();
                }
                if self.devices.is_empty() {
                    ui.label(self.t("status.no_matching_interface"));
                } else {
                    egui::ComboBox::from_label(self.t("label.interface"))
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
                let stop_adb_label = self.t("setting.stop_adb");
                ui.checkbox(&mut self.stop_adb, stop_adb_label)
                    .on_hover_text(self.t("log.adb_hint"));
                if ui
                    .add_enabled(
                        !self.busy && !self.devices.is_empty(),
                        egui::Button::new(self.t("action.read_device_info")),
                    )
                    .clicked()
                {
                    self.read_device_info();
                }
                if ui
                    .add_enabled(
                        !self.busy && self.device_info.is_some(),
                        egui::Button::new(self.t("action.list_allowed_roms")),
                    )
                    .clicked()
                {
                    self.list_roms();
                }

                ui.separator();
                ui.heading(format!("2. {}", self.t("section.official_rom")));
                if ui
                    .add_enabled(!self.busy, egui::Button::new(self.t("action.choose_rom")))
                    .clicked()
                {
                    self.choose_rom();
                }
                if ui
                    .add_enabled(
                        !self.busy && self.device_info.is_some(),
                        egui::Button::new(self.t("action.download_latest")),
                    )
                    .clicked()
                {
                    self.download_latest();
                }
                if let Some(path) = &self.rom_path {
                    ui.small(path.display().to_string());
                }
                if ui
                    .add_enabled(
                        !self.busy && self.device_info.is_some() && self.rom_path.is_some(),
                        egui::Button::new(self.t("action.validate_rom")),
                    )
                    .clicked()
                {
                    self.validate_rom();
                }
                if let Some(validated) = &self.validated {
                    ui.label(self.t("label.md5").replace("{md5}", &validated.md5));
                    if validated.erase {
                        ui.colored_label(egui::Color32::RED, self.t("status.validation_wipe"));
                    } else {
                        ui.colored_label(
                            egui::Color32::LIGHT_GREEN,
                            self.t("status.validation_no_wipe"),
                        );
                    }
                }
                if ui
                    .add_enabled(
                        !self.busy && self.validated.is_some(),
                        egui::Button::new(self.t("action.flash_validated")),
                    )
                    .clicked()
                {
                    self.request_flash();
                }
                if let Some(cancel) = &self.cancel {
                    if ui.button(self.t("action.cancel_flash")).clicked() {
                        cancel.store(true, Ordering::Relaxed);
                        self.status = self.t("status.cancellation_requested");
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
                ui.heading(self.t("section.recovery_actions"));
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(
                            !self.busy && !self.devices.is_empty(),
                            egui::Button::new(self.t("action.reboot")),
                        )
                        .clicked()
                    {
                        self.send_recovery_command("reboot:", "status.reboot_requested");
                    }
                    if ui
                        .add_enabled(
                            !self.busy && !self.devices.is_empty(),
                            egui::Button::new(self.t("action.erase_data")),
                        )
                        .clicked()
                    {
                        self.confirm_format = true;
                    }
                });
            });

        egui::CentralPanel::default().show(ui, |ui| {
            ui.heading(self.t("section.device_information"));
            if let Some(info) = &self.device_info {
                egui::Grid::new("device-info").striped(true).show(ui, |ui| {
                    for (label, value) in [
                        ("label.device", info.device.as_str()),
                        ("label.version", info.version.as_str()),
                        ("label.serial", info.sn.as_str()),
                        ("label.codebase", info.codebase.as_str()),
                        ("label.branch", info.branch.as_str()),
                        ("label.language", info.language.as_str()),
                        ("label.region", info.region.as_str()),
                        ("label.rom_zone", info.romzone.as_str()),
                    ] {
                        ui.strong(self.t(label));
                        ui.monospace(value);
                        ui.end_row();
                    }
                });
            } else {
                ui.label(self.t("status.read_info_first"));
            }
            ui.separator();
            ui.heading(self.t("section.allowed_rom_response"));
            egui::ScrollArea::vertical()
                .max_height(220.0)
                .show(ui, |ui| {
                    if self.rom_listing.is_empty() {
                        ui.label(self.t("status.no_response"));
                    } else {
                        ui.monospace(&self.rom_listing);
                    }
                });
            ui.separator();
            ui.heading(self.t("section.activity"));
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
            egui::Window::new(self.t("dialog.confirm_flash"))
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ui.ctx(), |ui| {
                    if erase {
                        ui.colored_label(egui::Color32::RED, self.t("dialog.flash_wipe_warning"));
                    } else {
                        ui.label(self.t("dialog.flash_question"));
                    }
                    ui.label(self.t("dialog.keep_connected"));
                    ui.horizontal(|ui| {
                        if ui.button(self.t("action.cancel")).clicked() {
                            self.confirm_flash = false;
                        }
                        let label = if erase {
                            self.t("action.erase_and_flash")
                        } else {
                            self.t("action.flash")
                        };
                        if ui.button(label).clicked() {
                            self.start_flash();
                        }
                    });
                });
        }

        if self.confirm_format {
            egui::Window::new(self.t("dialog.confirm_erase"))
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ui.ctx(), |ui| {
                    ui.colored_label(egui::Color32::RED, self.t("dialog.erase_warning"));
                    ui.horizontal(|ui| {
                        if ui.button(self.t("action.cancel")).clicked() {
                            self.confirm_format = false;
                        }
                        if ui.button(self.t("action.erase_all_data")).clicked() {
                            self.format_data();
                        }
                    });
                });
        }

        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(50));
    }
}
