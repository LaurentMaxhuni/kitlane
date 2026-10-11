pub mod catalog;
pub mod inventory;
pub mod inventory_linux;
pub mod inventory_macos;
pub mod model;
pub mod planner;
pub mod store;

pub fn new_device_id() -> String {
    uuid::Uuid::new_v4().to_string()
}
