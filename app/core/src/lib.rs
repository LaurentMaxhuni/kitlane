pub mod catalog;
pub mod inventory;
pub mod model;
pub mod store;

pub fn new_device_id() -> String {
    uuid::Uuid::new_v4().to_string()
}
