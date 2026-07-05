#[cfg(all(not(target_arch = "wasm32"), target_os = "android"))]
pub(crate) fn save_file_path(file_name: &str) -> std::path::PathBuf {
    std::path::PathBuf::from("/data/data/com.uneven3.lightcore/files").join(file_name)
}

#[cfg(all(not(target_arch = "wasm32"), target_os = "windows"))]
pub(crate) fn save_file_path(file_name: &str) -> std::path::PathBuf {
    if let Ok(appdata) = std::env::var("APPDATA") {
        return std::path::PathBuf::from(appdata)
            .join("Lightcore")
            .join(file_name);
    }
    fallback_save_file_path(file_name)
}

#[cfg(all(not(target_arch = "wasm32"), target_os = "macos"))]
pub(crate) fn save_file_path(file_name: &str) -> std::path::PathBuf {
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
pub(crate) fn save_file_path(file_name: &str) -> std::path::PathBuf {
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
