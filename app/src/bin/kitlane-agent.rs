#![cfg_attr(windows, windows_subsystem = "windows")]
#[path = "../maintenance_agent.rs"]
#[allow(dead_code)] // UI helpers are exercised in the desktop binary.
mod maintenance_agent;
#[path = "../self_update.rs"]
mod self_update;

fn main() {
    if std::env::args_os()
        .skip(1)
        .any(|argument| argument != "--background")
    {
        std::process::exit(2);
    }
    if let Err(error) = maintenance_agent::run() {
        // Report only to the local status file. No inventory, credentials, or logs.
        let message: String = error
            .chars()
            .filter(|c| !c.is_control())
            .take(240)
            .collect();
        let _ = maintenance_agent::mutate(|s| {
            s.agent_status = format!("Agent stopped: {message}");
            Ok(())
        });
        std::process::exit(1);
    }
}
