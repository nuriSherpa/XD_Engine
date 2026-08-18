struct Uniforms {
    view_proj: mat4x4<f32>,
    camera_pos: vec4<f32>,
    params: vec4<f32>, // x: cell_size, y: fade_near, z: fade_far, w: unused
};
@group(0) @binding(0) var<uniform> u: Uniforms;

struct VOut {
    @builtin(position) clip_pos: vec4<f32>,
    @location(0) world_pos: vec3<f32>,
};

@vertex
fn vs_main(@location(0) pos: vec3<f32>) -> VOut {
    var out: VOut;
    out.world_pos = pos;
    out.clip_pos = u.view_proj * vec4<f32>(pos, 1.0);
    return out;
}

// Returns 0..1 line intensity for a given cell size, clamped so it never
// goes numerically unstable at grazing angles (this is the key fix).
fn grid_line(coord: vec2<f32>, cell: f32) -> f32 {
    let c = coord / cell;
    let deriv = max(fwidth(c), vec2<f32>(0.0001));
    let g = abs(fract(c - 0.5) - 0.5) / deriv;
    let line = 1.0 - min(min(g.x, g.y), 1.0);
    return clamp(line, 0.0, 1.0);
}

@fragment
fn fs_main(in: VOut) -> @location(0) vec4<f32> {
    let p = in.world_pos.xz;
    let base = u.params.x;
    let fade_near = u.params.y;
    let fade_far = u.params.z;

    // How many base-cells fit in one pixel right now — drives the LOD blend.
    let pixels_per_cell = base / max(length(fwidth(p)), 0.0001);
    // 0 = fully on 'base' tier, 1 = fully on the next coarser tier (base*10).
    let lod_blend = clamp(1.0 - pixels_per_cell / 4.0, 0.0, 1.0);

    let minor_a = grid_line(p, base);
    let minor_b = grid_line(p, base * 10.0);
    let minor = mix(minor_a, minor_b, lod_blend);

    let major_a = grid_line(p, base * 10.0);
    let major_b = grid_line(p, base * 100.0);
    let major = mix(major_a, major_b, lod_blend);

    var color = vec3<f32>(0.35, 0.35, 0.38) * minor * 0.5;
    color = mix(color, vec3<f32>(0.55, 0.55, 0.6), major);

    let axis_w = max(fwidth(p) * 1.5, vec2<f32>(0.01));
    if (abs(p.y) < axis_w.y) { color = vec3<f32>(0.9, 0.2, 0.2); }
    if (abs(p.x) < axis_w.x) { color = vec3<f32>(0.2, 0.4, 0.9); }

    let dist = distance(in.world_pos, u.camera_pos.xyz);
    let fade = 1.0 - smoothstep(fade_near, fade_far, dist);

    let alpha = max(minor, major) * fade;
    if (alpha < 0.02) { discard; }
    return vec4<f32>(color, alpha);
}