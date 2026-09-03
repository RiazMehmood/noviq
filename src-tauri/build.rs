fn main() {
    // F014: only generate the Tauri context when building the desktop app.
    // The headless `noviq-server` (feature "server", no "desktop") has no Tauri
    // runtime, so running tauri_build::build() there is unnecessary. Build
    // scripts can't see crate features via cfg!, so check the env var Cargo
    // exports for the enabled feature.
    if std::env::var_os("CARGO_FEATURE_DESKTOP").is_some() {
        tauri_build::build();
    }
}
