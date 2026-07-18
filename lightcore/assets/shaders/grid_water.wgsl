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

struct GridWaterMaterial {
    time: f32,
    tile: f32,
    enabled: f32,
    mask_low: u32,
    mask_high: u32,
    _pad1: u32,
    _pad2: u32,
    _pad3: u32,
    half_size: vec4<f32>,
};

@group(1) @binding(0) var<uniform> material: GridWaterMaterial;

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

fn is_cell_active(cx: i32, cy: i32) -> bool {
    let gx = cx + 3;
    let gy = cy + 3;
    if (gx < 0 || gx >= 8 || gy < 0 || gy >= 8) {
        return false;
    }
    let bit_idx = u32(gy * 8 + gx);
    if (bit_idx < 32u) {
        return ((material.mask_low >> bit_idx) & 1u) != 0u;
    } else {
        return ((material.mask_high >> (bit_idx - 32u)) & 1u) != 0u;
    }
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

    let warped = world;
    let coord = warped / material.tile + vec2<f32>(0.5, 0.5);

    let cx = i32(floor(coord.x + 0.5));
    let cy = i32(floor(coord.y + 0.5));

    let fx = fract(coord.x + 0.5);
    let fy = fract(coord.y + 0.5);

    var active = is_cell_active(cx, cy);
    if (fx < 0.05) {
        active = active || is_cell_active(cx - 1, cy);
    }
    if (fx > 0.95) {
        active = active || is_cell_active(cx + 1, cy);
    }
    if (fy < 0.05) {
        active = active || is_cell_active(cx, cy - 1);
    }
    if (fy > 0.95) {
        active = active || is_cell_active(cx, cy + 1);
    }

    if (!active) {
        return vec4<f32>(0.0);
    }

    let lines = line_alpha(coord) * 0.24;
    let points = point_alpha(coord) * 0.75;
    let shimmer = 0.88 + 0.12 * sin(material.time * 1.7 + warped.x * 0.018 + warped.y * 0.011);

    let base = vec3<f32>(0.003, 0.012, 0.036);
    let grid = vec3<f32>(0.045, 0.28, 0.75) * (points + lines) * shimmer;
    var color = vec4<f32>(base + grid, board_mask * saturate(0.18 + points * 0.50 + lines));

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
