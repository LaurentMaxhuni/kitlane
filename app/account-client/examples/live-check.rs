//! Opt-in development Google/PKCE/Windows-vault check; never scans or uploads apps.
use kitlane_account_client::{Client, Error};
use kitlane_core::model::{DeviceInfo, Platform};
use kitlane_sync_client::{OsVault, Vault};
use sha2::{Digest, Sha256};
use std::sync::atomic::AtomicBool;

const ENDPOINT: &str =
    "https://br-damp-cell-batye88a-kitlane.compute.c-8.eu-central-1.aws.neon.tech";

struct Cleanup(String);
impl Drop for Cleanup {
    fn drop(&mut self) {
        let _ = OsVault.delete(&self.0);
        let _ = OsVault.delete(&format!("{}-installation", self.0));
    }
}

fn main() -> Result<(), Error> {
    let device = DeviceInfo {
        id: kitlane_core::new_device_id(),
        name: "Kitlane development sign-in check".into(),
        platform: Platform::current().ok_or(Error::Configuration)?,
        architecture: std::env::consts::ARCH.into(),
    };
    let _cleanup = Cleanup(format!(
        "cloud-v2-{:x}-{}",
        Sha256::digest(ENDPOINT.as_bytes()),
        device.id
    ));
    let mut client = Client::new(ENDPOINT, &device)?;
    println!("Authorize the development sign-in check in the browser.");
    client.sign_in(&AtomicBool::new(false))?;
    println!("Google callback, PKCE exchange and OS-vault persistence passed.");
    let mut restored = Client::new(ENDPOINT, &device)?;
    let check = (|| {
        if restored.restore()?.is_none() || !restored.session_valid() {
            return Err(Error::Unauthorized);
        }
        restored.devices()?;
        println!("New-client restoration, refresh rotation and authenticated request passed.");
        Ok(())
    })();
    let logout = restored.sign_out();
    check?;
    logout?;
    if Client::new(ENDPOINT, &device)?.restore()?.is_some() {
        return Err(Error::InvalidResponse);
    }
    println!("Remote sign-out and local credential removal passed. No inventory uploaded.");
    Ok(())
}
