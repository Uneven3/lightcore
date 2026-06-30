#import bevy_sprite::{
    mesh2d_vertex_output::VertexOutput,
    mesh2d_view_bindings::view,
}

#ifdef TONEMAP_IN_SHADER
#import bevy_core_pipeline::tonemapping
#endif
#ifdef SRGB_OUTPUT
#import bevy_render::color_operations::linear_to_srgb
#endif
#ifdef OKLAB_OUTPUT
#import bevy_render::color_operations::linear_rgb_to_oklab
#endif

const RIPPLE_SLOTS: u32 = 8u;

struct GridWaterMaterial {
    time: f32,
    tile: f32,
    enabled: f32,
    _pad: f32,
    half_size: vec4<f32>,
    ripples: array<vec4<f32>, 8>,
};

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> material: GridWaterMaterial;

fn saturate(v: f32) -> f32 {
    return clamp(v, 0.0, 1.0);
}

fn line_alpha(coord: vec2<f32>) -> f32 {
    let cell = abs(fract(coord) - vec2<f32>(0.5));
    let dist_to_line = min(cell.x, cell.y);
    return 1.0 - smoothstep(0.004, 0.014, dist_to_line);
}

fn point_alpha(coord: vec2<f32>) -> f32 {
    let cell = fract(coord) - vec2<f32>(0.5);
    let d = length(cell);
    return 1.0 - smoothstep(0.025, 0.060, d);
}

fn ripple_field(world: vec2<f32>) -> vec2<f32> {
    var offset = vec2<f32>(0.0, 0.0);
    for (var i = 0u; i < RIPPLE_SLOTS; i = i + 1u) {
        let r = material.ripples[i];
        let strength = r.z;
        let age = material.time - r.w;
        if (strength > 0.001 && age >= 0.0 && age < 2.4) {
            let delta = world - r.xy;
            let dist = max(length(delta), 0.001);
            let dir = delta / dist;
            let wave = sin(dist * 0.052 - age * 8.5);
            let falloff = exp(-dist * 0.010) * exp(-age * 1.35);
            offset = offset + dir * wave * falloff * strength * 14.0;
        }
    }
    return offset;
}

@fragment
fn fragment(mesh: VertexOutput) -> @location(0) vec4<f32> {
    if (material.enabled < 0.5) {
        return vec4<f32>(0.0);
    }

    let world = mesh.world_position.xy;
    let edge = max(abs(world.x) / material.half_size.x, abs(world.y) / material.half_size.y);
    let board_mask = 1.0 - smoothstep(0.90, 1.0, edge);
    if (board_mask <= 0.001) {
        return vec4<f32>(0.0);
    }

    let displacement = ripple_field(world);
    let warped = world + displacement;
    let coord = warped / material.tile + vec2<f32>(0.5, 0.5);

    let lines = line_alpha(coord) * 0.16;
    let points = point_alpha(coord) * 0.58;
    let shimmer = 0.88 + 0.12 * sin(material.time * 1.7 + warped.x * 0.018 + warped.y * 0.011);
    let wake = saturate(length(displacement) / 20.0);

    let base = vec3<f32>(0.002, 0.010, 0.032);
    let grid = vec3<f32>(0.025, 0.18, 0.54) * (points + lines) * shimmer;
    let foam = vec3<f32>(0.07, 0.38, 0.78) * wake * (0.15 + points * 0.35);
    var color = vec4<f32>(base + grid + foam, board_mask * saturate(0.10 + points * 0.45 + lines + wake * 0.22));

#ifdef TONEMAP_IN_SHADER
    color = tonemapping::tone_mapping(color, view.color_grading);
#endif
#ifdef SRGB_OUTPUT
    color = vec4(linear_to_srgb(color.rgb), color.a);
#endif
#ifdef OKLAB_OUTPUT
    color = vec4(linear_rgb_to_oklab(color.rgb), color.a);
#endif

    return color;
}
