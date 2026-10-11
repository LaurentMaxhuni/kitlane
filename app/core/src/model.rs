use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Platform {
    Windows,
    Macos,
    Linux,
}

impl Platform {
    pub fn current() -> Option<Self> {
        match std::env::consts::OS {
            "windows" => Some(Self::Windows),
            "macos" => Some(Self::Macos),
            "linux" => Some(Self::Linux),
            _ => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Windows => "Windows",
            Self::Macos => "macOS",
            Self::Linux => "Linux",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeviceInfo {
    pub id: String,
    pub name: String,
    pub platform: Platform,
    pub architecture: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InstallScope {
    User,
    Machine,
    Unknown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InventorySource {
    WindowsRegistry,
    MacosBundle,
    Flatpak,
    Manual,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InstalledApp {
    pub id: String,
    pub name: String,
    pub publisher: Option<String>,
    pub version: Option<String>,
    pub scope: InstallScope,
    pub source: InventorySource,
    /// Observed bundle/Flatpak identifier. Metadata alone does not verify identity.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub native_id: Option<String>,
    /// Observed package origin; meaningful only for package-manager inventory.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub origin: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InventorySnapshot {
    pub device: DeviceInfo,
    pub scanned_at_unix_secs: u64,
    pub apps: Vec<InstalledApp>,
    pub warnings: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AppSettings {
    pub dark_theme: bool,
}

fn valid_text(value: &str, max: usize) -> bool {
    !value.trim().is_empty() && value.len() <= max && !value.chars().any(char::is_control)
}

impl DeviceInfo {
    pub fn validate(&self) -> Result<(), &'static str> {
        if !uuid::Uuid::parse_str(&self.id)
            .is_ok_and(|id| !id.is_nil() && id.to_string() == self.id)
            || !valid_text(&self.name, 128)
            || !valid_text(&self.architecture, 32)
        {
            return Err("Invalid device identity or metadata");
        }
        Ok(())
    }
}

impl InventorySnapshot {
    pub fn validate(&self) -> Result<(), &'static str> {
        self.device.validate()?;
        if self.apps.len() > 10_000 || self.warnings.len() > 64 {
            return Err("Inventory exceeds allowed record count");
        }
        let mut ids = std::collections::HashSet::with_capacity(self.apps.len());
        for app in &self.apps {
            if !valid_text(&app.id, 256)
                || !valid_text(&app.name, 512)
                || app
                    .publisher
                    .as_deref()
                    .is_some_and(|v| !valid_text(v, 512))
                || app.version.as_deref().is_some_and(|v| !valid_text(v, 128))
                || app.native_id.as_deref().is_some_and(|v| !valid_text(v, 256))
                || app.origin.as_deref().is_some_and(|v| !valid_text(v, 256))
                || !ids.insert(&app.id)
            {
                return Err("Invalid or duplicate application record");
            }
        }
        if self.warnings.iter().any(|v| !valid_text(v, 1024)) {
            return Err("Invalid inventory warning");
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_duplicate_ids_and_control_characters() {
        let app = InstalledApp {
            id: "registry-app".into(),
            name: "Example".into(),
            publisher: None,
            version: None,
            scope: InstallScope::User,
            source: InventorySource::WindowsRegistry,
            native_id: None,
            origin: None,
        };
        let mut snapshot = InventorySnapshot {
            device: DeviceInfo {
                id: uuid::Uuid::new_v4().to_string(),
                name: "This device".into(),
                platform: Platform::Windows,
                architecture: "x86_64".into(),
            },
            scanned_at_unix_secs: 0,
            apps: vec![app.clone()],
            warnings: vec![],
        };
        assert!(snapshot.validate().is_ok());
        snapshot.apps.push(app);
        assert!(snapshot.validate().is_err());
        snapshot.apps.pop();
        snapshot.apps[0].name.push('\n');
        assert!(snapshot.validate().is_err());
    }
}
