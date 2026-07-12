//! Cross-platform save-file read/write. `run.rs` and `campaign.rs` both need to persist a small
//! text blob under a stable name ("run.txt", "campaign.txt") — this module is the ONE place that
//! knows how, so the two callers don't each carry their own copy of the native-vs-wasm branching
//! (they used to; the two copies were already drifting — see the git history around this file).
//!
//! Two backends: a plain file under an OS-appropriate directory (`save_file_path`, native targets),
//! and the browser's `localStorage` for wasm32 — native filesystem access isn't available there.
//! Before this module, the wasm32 path was a silent no-op (`load_*_text`/`save_*_text` returned
//! `None`/`Ok(())` without doing anything): every web build lost the player's run and campaign
//! progress on every page refresh, with no error or warning anywhere. `localStorage` is small
//! (usually 5-10MB per origin) and synchronous, which is exactly the shape these tiny save blobs
//! need.

/// Loads the named save file's raw text, or `None` if it doesn't exist yet (first launch) or
/// can't be read.
pub(crate) fn load_save_file(file_name: &str) -> Option<String> {
    backend::load(file_name)
}

/// Writes `contents` as the named save file, creating any parent directory first (native) or the
/// `localStorage` entry (wasm). Returns the underlying error message on failure so callers can log
/// it — a failed save should never panic (loss of progress is bad; crashing over it is worse).
pub(crate) fn write_save_file(file_name: &str, contents: &str) -> Result<(), String> {
    backend::save(file_name, contents)
}

#[cfg(not(target_arch = "wasm32"))]
mod backend {
    use super::save_file_path;

    pub(super) fn load(file_name: &str) -> Option<String> {
        std::fs::read_to_string(save_file_path(file_name)).ok()
    }

    pub(super) fn save(file_name: &str, contents: &str) -> Result<(), String> {
        let path = save_file_path(file_name);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|err| err.to_string())?;
        }
        std::fs::write(path, contents).map_err(|err| err.to_string())
    }
}

#[cfg(target_arch = "wasm32")]
mod backend {
    /// `web_sys::window()`/`Window::local_storage()` both return `Option`/`Result<Option<_>, _>`
    /// (e.g. `None` in a sandboxed iframe without storage access, or a worker with no `Window` at
    /// all) — every step is fallible, unlike native's "the filesystem exists" assumption.
    pub(super) fn load(file_name: &str) -> Option<String> {
        let storage = web_sys::window()?.local_storage().ok()??;
        storage.get_item(file_name).ok()?
    }

    pub(super) fn save(file_name: &str, contents: &str) -> Result<(), String> {
        let window = web_sys::window().ok_or("no browser window")?;
        let storage = window
            .local_storage()
            .map_err(|_| "localStorage access denied".to_string())?
            .ok_or("localStorage unavailable")?;
        storage
            .set_item(file_name, contents)
            .map_err(|_| "localStorage.setItem failed (quota exceeded?)".to_string())
    }
}

#[cfg(all(not(target_arch = "wasm32"), target_os = "android"))]
fn save_file_path(file_name: &str) -> std::path::PathBuf {
    std::path::PathBuf::from("/data/data/com.uneven3.lightcore/files").join(file_name)
}

#[cfg(all(not(target_arch = "wasm32"), target_os = "windows"))]
fn save_file_path(file_name: &str) -> std::path::PathBuf {
    if let Ok(appdata) = std::env::var("APPDATA") {
        return std::path::PathBuf::from(appdata)
            .join("Lightcore")
            .join(file_name);
    }
    fallback_save_file_path(file_name)
}

#[cfg(all(not(target_arch = "wasm32"), target_os = "macos"))]
fn save_file_path(file_name: &str) -> std::path::PathBuf {
    if let Ok(home) = std::env::var("HOME") {
        return std::path::PathBuf::from(home)
            .join("Library")
            .join("Application Support")
            .join("Lightcore")
            .join(file_name);
    }
    fallback_save_file_path(file_name)
}

#[cfg(all(
    not(target_arch = "wasm32"),
    not(any(target_os = "android", target_os = "windows", target_os = "macos"))
))]
fn save_file_path(file_name: &str) -> std::path::PathBuf {
    if let Ok(data_home) = std::env::var("XDG_DATA_HOME") {
        return std::path::PathBuf::from(data_home)
            .join("lightcore")
            .join(file_name);
    }
    if let Ok(home) = std::env::var("HOME") {
        return std::path::PathBuf::from(home)
            .join(".local")
            .join("share")
            .join("lightcore")
            .join(file_name);
    }
    fallback_save_file_path(file_name)
}

#[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
fn fallback_save_file_path(file_name: &str) -> std::path::PathBuf {
    std::env::current_dir()
        .unwrap_or_else(|_| std::path::PathBuf::from("."))
        .join(format!("lightcore_{file_name}"))
}
