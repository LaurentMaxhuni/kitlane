use std::{path::PathBuf, sync::mpsc, thread};

use kitlane_core::{
    inventory,
    model::{AppSettings, DeviceInfo, InventorySnapshot, Platform},
    store::Store,
};

pub enum Command {
    Scan,
    SaveSettings(AppSettings),
    CheckEnrollment {
        endpoint: String,
    },
    Enroll {
        endpoint: String,
        pairing_token: zeroize::Zeroizing<String>,
    },
    Sync {
        endpoint: String,
        snapshot: Box<InventorySnapshot>,
    },
    Disconnect {
        endpoint: String,
    },
}

pub enum Event {
    Ready {
        device: DeviceInfo,
        settings: AppSettings,
        cached: Option<InventorySnapshot>,
    },
    Scanned(InventorySnapshot),
    ScanFailed(String),
    StorageFailed(String),
    EnrollmentState(bool),
    Synced(Vec<InventorySnapshot>),
    SyncFailed(String),
}

pub struct Worker {
    commands: Option<mpsc::SyncSender<Command>>,
    pub events: mpsc::Receiver<Event>,
    handle: Option<thread::JoinHandle<()>>,
}

impl Worker {
    pub fn try_send(&self, command: Command) -> Result<(), mpsc::TrySendError<Command>> {
        match &self.commands {
            Some(sender) => sender.try_send(command),
            None => Err(mpsc::TrySendError::Disconnected(command)),
        }
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        self.commands.take();
        if let Some(handle) = self.handle.take() {
            // Drain queued settings writes before process exit.
            let _ = handle.join();
        }
    }
}

fn state_path() -> Result<PathBuf, String> {
    let base = if cfg!(windows) {
        std::env::var_os("LOCALAPPDATA").map(PathBuf::from)
    } else if cfg!(target_os = "macos") {
        std::env::var_os("HOME").map(|v| PathBuf::from(v).join("Library/Application Support"))
    } else {
        std::env::var_os("XDG_STATE_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|v| PathBuf::from(v).join(".local/state")))
    }
    .ok_or("Cannot locate local application storage")?;
    if !base.is_absolute() {
        return Err("Application storage path must be absolute".into());
    }
    let directory = base.join("Kitlane");
    std::fs::create_dir_all(&directory).map_err(|e| format!("Cannot create local storage: {e}"))?;
    Ok(directory.join("state.redb"))
}

fn initialize() -> Result<(Store, DeviceInfo, AppSettings, Option<InventorySnapshot>), String> {
    let store =
        Store::open(state_path()?).map_err(|e| format!("Cannot open local database: {e}"))?;
    let device = match store.load_device().map_err(|e| e.to_string())? {
        Some(device) => device,
        None => {
            let platform = Platform::current().ok_or("Unsupported operating system")?;
            let device = DeviceInfo {
                id: kitlane_core::new_device_id(),
                name: format!("This {} device", platform.label()),
                platform,
                architecture: std::env::consts::ARCH.into(),
            };
            store.save_device(&device).map_err(|e| e.to_string())?;
            device
        }
    };
    let settings = store.load_settings().map_err(|e| e.to_string())?;
    let cached = store.load_snapshot().map_err(|e| e.to_string())?;
    Ok((store, device, settings, cached))
}

pub fn start(ctx: eframe::egui::Context) -> Worker {
    let (command_sender, commands) = mpsc::sync_channel(4);
    let (events, event_receiver) = mpsc::channel();
    let handle = thread::spawn(move || {
        let emit = |event| {
            let delivered = events.send(event).is_ok();
            ctx.request_repaint();
            delivered
        };
        let (store, device, settings, cached) = match initialize() {
            Ok(state) => state,
            Err(error) => {
                emit(Event::StorageFailed(error));
                return;
            }
        };
        if !emit(Event::Ready {
            device: device.clone(),
            settings,
            cached,
        }) {
            return;
        }
        while let Ok(command) = commands.recv() {
            match command {
                Command::Scan => match inventory::scan(device.clone()) {
                    Ok(snapshot) => {
                        if let Err(error) = store.save_snapshot(&snapshot) {
                            emit(Event::StorageFailed(format!(
                                "Inventory could not be saved: {error}"
                            )));
                        }
                        if !emit(Event::Scanned(snapshot)) {
                            return;
                        }
                    }
                    Err(error) => {
                        if !emit(Event::ScanFailed(error.to_string())) {
                            return;
                        }
                    }
                },
                Command::SaveSettings(settings) => {
                    if let Err(error) = store.save_settings(&settings)
                        && !emit(Event::StorageFailed(format!(
                            "Settings could not be saved: {error}"
                        )))
                    {
                        return;
                    }
                }
                Command::CheckEnrollment { endpoint } => {
                    let result = kitlane_sync_client::Client::new(&endpoint, &device)
                        .and_then(|client| client.is_enrolled());
                    match result {
                        Ok(enrolled) => {
                            emit(Event::EnrollmentState(enrolled));
                        }
                        Err(error) => {
                            emit(Event::SyncFailed(error.to_string()));
                        }
                    }
                }
                Command::Enroll {
                    endpoint,
                    pairing_token,
                } => {
                    let result = kitlane_sync_client::Client::new(&endpoint, &device)
                        .and_then(|client| client.enroll(&pairing_token));
                    match result {
                        Ok(()) => {
                            emit(Event::EnrollmentState(true));
                        }
                        Err(error) => {
                            emit(Event::SyncFailed(error.to_string()));
                        }
                    }
                }
                Command::Sync { endpoint, snapshot } => {
                    let result = kitlane_sync_client::Client::new(&endpoint, &device)
                        .and_then(|client| client.sync(&snapshot));
                    match result {
                        Ok(result) => {
                            emit(Event::Synced(result.peers));
                        }
                        Err(error) => {
                            emit(Event::SyncFailed(error.to_string()));
                        }
                    }
                }
                Command::Disconnect { endpoint } => {
                    let result = kitlane_sync_client::Client::new(&endpoint, &device)
                        .and_then(|client| client.disconnect());
                    match result {
                        Ok(()) => {
                            emit(Event::EnrollmentState(false));
                        }
                        Err(error) => {
                            emit(Event::SyncFailed(error.to_string()));
                        }
                    }
                }
            }
        }
    });
    Worker {
        commands: Some(command_sender),
        events: event_receiver,
        handle: Some(handle),
    }
}
