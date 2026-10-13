// Hide the console in distributed Release builds; keep it for developer Debug builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    device_manager::run()
}
