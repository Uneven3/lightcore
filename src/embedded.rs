use bevy::asset::{AssetPath, embedded_path};

pub(crate) fn watermark_path() -> AssetPath<'static> {
    AssetPath::from_path_buf(embedded_path!("", "../assets/bevy_bird_dark.png"))
        .with_source("embedded")
}

pub(crate) fn back_icon_path() -> AssetPath<'static> {
    AssetPath::from_path_buf(embedded_path!("", "../assets/icons/back.png")).with_source("embedded")
}

pub(crate) fn play_icon_path() -> AssetPath<'static> {
    AssetPath::from_path_buf(embedded_path!("", "../assets/icons/play.png")).with_source("embedded")
}

pub(crate) fn settings_icon_path() -> AssetPath<'static> {
    AssetPath::from_path_buf(embedded_path!("", "../assets/icons/settings.png"))
        .with_source("embedded")
}

pub(crate) fn power_icon_path() -> AssetPath<'static> {
    AssetPath::from_path_buf(embedded_path!("", "../assets/icons/power.png"))
        .with_source("embedded")
}
