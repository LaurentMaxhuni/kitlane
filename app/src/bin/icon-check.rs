#[path = "../native_icons.rs"]
#[allow(dead_code)] // Counts-only diagnostic does not render or restore cached images.
mod native_icons;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let device = kitlane_core::model::DeviceInfo {
        id: kitlane_core::new_device_id(),
        name: "Icon diagnostic".into(),
        platform: kitlane_core::model::Platform::current().ok_or("Unsupported platform")?,
        architecture: std::env::consts::ARCH.into(),
    };
    let local = kitlane_core::inventory::scan_with_icons(device)?;
    let icons = native_icons::load(&local.icons, None);
    // Inspect only aggregate coverage; never print application names or local paths.
    println!(
        "apps={} local_icon_sources={} local_icons={}",
        local.snapshot.apps.len(),
        local.icons.len(),
        icons.len()
    );
    Ok(())
}
