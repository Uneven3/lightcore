use bevy::asset::{Handle, load_internal_asset, uuid_handle};
use bevy::color::LinearRgba;
use bevy::mesh::MeshVertexBufferLayoutRef;
use bevy::prelude::*;
use bevy::reflect::TypePath;
use bevy::render::render_resource::{
    AsBindGroup, BlendComponent, BlendFactor, BlendOperation, BlendState, RenderPipelineDescriptor,
    SpecializedMeshPipelineError,
};
use bevy::shader::{Shader, ShaderRef};
use bevy::sprite_render::{AlphaMode2d, Material2d, Material2dKey, Material2dPlugin};

const ADDITIVE_SHADER_HANDLE: Handle<Shader> = uuid_handle!("5fc9c836-c295-458b-b5ea-833ba88c3178");

pub(crate) struct AdditiveMaterialPlugin;

impl Plugin for AdditiveMaterialPlugin {
    fn build(&self, app: &mut App) {
        load_internal_asset!(
            app,
            ADDITIVE_SHADER_HANDLE,
            "../../assets/shaders/additive_glow.wgsl",
            Shader::from_wgsl
        );
        app.add_plugins(Material2dPlugin::<AdditiveMaterial>::default());
    }
}

/// A textured quad blended ADDITIVELY (`dst + src·alpha`) instead of the normal
/// `src·alpha + dst·(1−alpha)` alpha blend `Sprite`/`ColorMaterial` use. Standard alpha blend
/// AVERAGES overlapping translucent colors toward gray/brown — the "dirty paint" look a captured
/// light's shard or glow halo gets whenever it crosses another light, tile, or shard. Additive
/// blend only ever SUMS brightness, so overlaps brighten toward white the way real overlapping
/// light does — without a real post-process Bloom pass (measured ~70% of frame budget on the
/// target GPU across several render-target resolutions, see `GlowSettings`'s doc comment; that's
/// why the halo system fakes glow with alpha-blended stacked sprites instead).
#[derive(Asset, TypePath, AsBindGroup, Clone)]
pub(crate) struct AdditiveMaterial {
    #[uniform(0)]
    pub(crate) color: LinearRgba,
    #[texture(1)]
    #[sampler(2)]
    pub(crate) texture: Handle<Image>,
}

impl Material2d for AdditiveMaterial {
    fn fragment_shader() -> ShaderRef {
        ADDITIVE_SHADER_HANDLE.into()
    }

    fn alpha_mode(&self) -> AlphaMode2d {
        AlphaMode2d::Blend
    }

    /// Overrides the base mesh2d pipeline's `BlendState::ALPHA_BLENDING` (set by `alpha_mode`
    /// above, which we still need for `Blend` to land in the transparent pass with depth-write
    /// off) with a true additive blend: color sums (`src·alpha + dst`), alpha passes the
    /// destination through untouched so stacking additive quads doesn't corrupt the framebuffer's
    /// own alpha.
    fn specialize(
        descriptor: &mut RenderPipelineDescriptor,
        _layout: &MeshVertexBufferLayoutRef,
        _key: Material2dKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        if let Some(target) = descriptor
            .fragment
            .as_mut()
            .and_then(|f| f.targets.first_mut())
            .and_then(|t| t.as_mut())
        {
            target.blend = Some(BlendState {
                color: BlendComponent {
                    src_factor: BlendFactor::SrcAlpha,
                    dst_factor: BlendFactor::One,
                    operation: BlendOperation::Add,
                },
                alpha: BlendComponent {
                    src_factor: BlendFactor::Zero,
                    dst_factor: BlendFactor::One,
                    operation: BlendOperation::Add,
                },
            });
        }
        Ok(())
    }
}
