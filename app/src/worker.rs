use kitlane_account_client::{Account, PairCode};
use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread,
};

use kitlane_core::{
    inventory,
    model::{AppSettings, DeviceInfo, InventorySnapshot, Platform},
    store::Store,
};

pub enum Command {
    Scan,
    SaveSettings(AppSettings),
    SaveSetup { name: String, settings: AppSettings },
    SignIn,
    SignOut,
    GenerateCode,
    PairCode { code: zeroize::Zeroizing<String> },
    RefreshDevices,
    Sync { snapshot: Box<InventorySnapshot> },
}

pub enum Event {
    Ready {
        device: DeviceInfo,
        settings: AppSettings,
        cached: Option<InventorySnapshot>,
    },
    Scanned(InventorySnapshot),
    IconsLoaded(crate::shell::native_icons::Icons),
    ScanFailed(String),
    StorageFailed(String),
    SetupSaved {
        device: DeviceInfo,
        settings: AppSettings,
    },
    SetupFailed(String),
    AccountReady(Account),
    SignedOut(Option<String>),
    CodeGenerated(PairCode),
    Devices(Vec<DeviceInfo>),
    Synced(Vec<InventorySnapshot>),
    SyncFailed(String),
}

pub struct Worker {
    commands: Option<mpsc::SyncSender<Command>>,
    pub events: mpsc::Receiver<Event>,
    handle: Option<thread::JoinHandle<()>>,
    cancel: Arc<AtomicBool>,
}

#[cfg(test)]
pub(crate) fn test_worker() -> (Worker, mpsc::Receiver<Command>, mpsc::Sender<Event>) {
    let (commands, receiver) = mpsc::sync_channel(4);
    let (sender, events) = mpsc::channel();
    (
        Worker {
            commands: Some(commands),
            events,
            handle: None,
            cancel: Arc::new(AtomicBool::new(false)),
        },
        receiver,
        sender,
    )
}

impl Worker {
    pub fn cancel_sign_in(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
    pub fn try_send(&self, command: Command) -> Result<(), mpsc::TrySendError<Command>> {
        if matches!(&command, Command::SignIn) {
            self.cancel.store(false, Ordering::Relaxed);
        }
        match &self.commands {
            Some(sender) => sender.try_send(command),
            None => Err(mpsc::TrySendError::Disconnected(command)),
        }
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        self.cancel_sign_in();
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
    let cancel = Arc::new(AtomicBool::new(false));
    let worker_cancel = cancel.clone();
    let handle = thread::spawn(move || {
        let emit = |event| {
            let delivered = events.send(event).is_ok();
            ctx.request_repaint();
            delivered
        };
        let (store, mut device, settings, cached) = match initialize() {
            Ok(state) => state,
            Err(error) => {
                emit(Event::StorageFailed(error));
                return;
            }
        };
        let icon_directory = state_path()
            .ok()
            .and_then(|path| path.parent().map(|parent| parent.join("icons")));
        let cached_icons = cached.as_ref().map(|snapshot| {
            crate::shell::native_icons::from_cache(snapshot, icon_directory.as_deref())
        });
        if !emit(Event::Ready {
            device: device.clone(),
            settings,
            cached,
        }) {
            return;
        }
        if let Some(icons) = cached_icons {
            emit(Event::IconsLoaded(icons));
        }
        let mut account_client = kitlane_account_client::Client::new(
            option_env!("KITLANE_ACCOUNT_API").unwrap_or(""),
            &device,
        );
        let mut signed_in = false;
        match account_client
            .as_mut()
            .map_err(|e| *e)
            .and_then(|client| client.restore())
        {
            Ok(Some(account)) => {
                signed_in = true;
                emit(Event::AccountReady(account));
            }
            Ok(None) => {
                emit(Event::SignedOut(None));
            }
            Err(error) => {
                emit(Event::SignedOut(Some(error.to_string())));
            }
        }
        loop {
            let received = commands.recv_timeout(std::time::Duration::from_secs(1));
            if signed_in
                && let Err(error) = account_client
                    .as_mut()
                    .map_err(|e| *e)
                    .and_then(|client| client.maintain_session())
            {
                signed_in = false;
                emit(Event::SignedOut(Some(error.to_string())));
            }
            let command = match received {
                Ok(command) => command,
                Err(mpsc::RecvTimeoutError::Timeout) => continue,
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            };
            match command {
                Command::Scan if signed_in => match inventory::scan_with_icons(device.clone()) {
                    Ok(local) => {
                        let snapshot = local.snapshot;
                        let mut icons = crate::shell::native_icons::from_cache(
                            &snapshot,
                            icon_directory.as_deref(),
                        );
                        icons.extend(crate::shell::native_icons::load(
                            &local.icons,
                            icon_directory.as_deref(),
                        ));
                        emit(Event::IconsLoaded(icons));
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
                Command::Scan => {
                    emit(Event::ScanFailed(
                        "Sign in with Google before scanning.".into(),
                    ));
                }
                Command::SaveSettings(settings) => {
                    if let Err(error) = store.save_settings(&settings)
                        && !emit(Event::StorageFailed(format!(
                            "Settings could not be saved: {error}"
                        )))
                    {
                        return;
                    }
                }
                Command::SaveSetup { name, settings } => {
                    let mut updated = device.clone();
                    updated.name = name.trim().to_owned();
                    match store.save_setup(&updated, &settings) {
                        Ok(()) => {
                            let renamed = updated != device;
                            device = updated;
                            if renamed
                                && let Ok(client) = &mut account_client
                                && let Err(error) = client.update_device(&device)
                            {
                                if !client.session_valid() {
                                    signed_in = false;
                                    emit(Event::SignedOut(Some(error.to_string())));
                                } else {
                                    emit(Event::SyncFailed(format!(
                                        "Name saved locally; cloud update failed. {}",
                                        error
                                    )));
                                }
                            }
                            emit(Event::SetupSaved {
                                device: device.clone(),
                                settings,
                            });
                        }
                        Err(error) => {
                            emit(Event::SetupFailed(format!(
                                "Setup could not be saved: {error}"
                            )));
                        }
                    }
                }
                Command::SignIn => {
                    let result = account_client
                        .as_mut()
                        .map_err(|e| *e)
                        .and_then(|client| client.sign_in(&worker_cancel));
                    match result {
                        Ok(account) => {
                            signed_in = true;
                            emit(Event::AccountReady(account));
                        }
                        Err(e) => {
                            signed_in = false;
                            emit(Event::SignedOut(Some(e.to_string())));
                        }
                    }
                }
                Command::SignOut => {
                    signed_in = false;
                    let result = account_client
                        .as_mut()
                        .map_err(|e| *e)
                        .and_then(|client| client.sign_out());
                    emit(Event::SignedOut(result.err().map(|e| e.to_string())));
                }
                command => {
                    let result = (|| {
                        if !signed_in {
                            return Err(kitlane_account_client::Error::Unauthorized);
                        }
                        let client = account_client.as_mut().map_err(|e| *e)?;
                        match command {
                            Command::GenerateCode => {
                                emit(Event::CodeGenerated(client.create_pairing_code()?));
                            }
                            Command::PairCode { code } => {
                                emit(Event::Devices(client.pair(&code)?));
                            }
                            Command::RefreshDevices => {
                                emit(Event::Devices(client.devices()?));
                            }
                            Command::Sync { snapshot } => {
                                emit(Event::Synced(client.sync(&snapshot)?));
                            }
                            _ => unreachable!(),
                        }
                        Ok(())
                    })();
                    if let Err(error) = result {
                        if error == kitlane_account_client::Error::Unauthorized
                            || account_client
                                .as_ref()
                                .is_ok_and(|client| !client.session_valid())
                        {
                            signed_in = false;
                            emit(Event::SignedOut(Some(error.to_string())));
                        } else {
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
        cancel,
    }
}
