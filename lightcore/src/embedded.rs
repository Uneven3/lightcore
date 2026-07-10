use bevy::asset::{AssetPath, embedded_path};

pub(crate) fn watermark_path() -> AssetPath<'static> {
    AssetPath::from_path_buf(embedded_path!("", "../assets/bevy_bird_dark.png"))
        .with_source("embedded")
}
