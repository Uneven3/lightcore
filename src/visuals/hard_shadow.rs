use bevy::prelude::*;

use crate::core::components::HardShadow;
use crate::core::components::HardShadowLabel;

/// Keeps a `HardShadow` tile's hit-counter label in sync whenever it takes a hit.
pub(crate) fn update_hard_shadow_label(
    shadows: Query<(&HardShadow, &Children), Changed<HardShadow>>,
    mut labels: Query<&mut Text2d, With<HardShadowLabel>>,
) {
    for (hard, children) in &shadows {
        for &child in children {
            if let Ok(mut text) = labels.get_mut(child) {
                text.0 = hard.0.to_string();
            }
        }
    }
}
