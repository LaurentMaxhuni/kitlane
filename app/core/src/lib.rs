pub mod catalog;
pub mod inventory;
pub mod inventory_linux;
pub mod inventory_macos;
pub mod maintenance;
pub mod model;
pub mod planner;
pub mod program_updates;
pub mod store;
pub mod suggestions;

pub fn new_device_id() -> String {
    uuid::Uuid::new_v4().to_string()
}
