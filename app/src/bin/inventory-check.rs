use kitlane_core::{
    catalog::{AssociationSource, Catalog, MatchOutcome},
    inventory,
    model::{DeviceInfo, Platform},
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let platform = Platform::current().ok_or("Unsupported operating system")?;
    let snapshot = inventory::scan(DeviceInfo {
        id: kitlane_core::new_device_id(),
        name: "Inventory diagnostic".into(),
        platform,
        architecture: std::env::consts::ARCH.into(),
    })?;
    let catalog = Catalog::bundled()?;
    let matches = snapshot
        .apps
        .iter()
        .filter(|app| matches!(catalog.match_app(app), MatchOutcome::Matched(_)))
        .count();
    let provider_matches = snapshot
        .apps
        .iter()
        .filter(|app| {
            matches!(catalog.match_app(app), MatchOutcome::Matched(product)
            if product.association == AssociationSource::WingetProductCode)
        })
        .count();
    println!(
        "Read-only scan: {} applications; {} catalog matches ({} via WinGet identifiers); {} coverage warnings.",
        snapshot.apps.len(),
        matches,
        provider_matches,
        snapshot.warnings.len()
    );
    Ok(())
}
