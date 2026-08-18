struct Uniforms {
    mvp: mat4x4<f32>,
    color: vec4<f32>,
};
@group(0) @binding(0) var<uniform> u: Uniforms;

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
};

const OUTLINE_WIDTH: f32 = 0.02;

@vertex
fn vs_main(input: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    let expanded = input.position + normalize(input.normal) * OUTLINE_WIDTH;
    out.clip_position = u.mvp * vec4<f32>(expanded, 1.0);
    return out;
}

@fragment
fn fs_main() -> @location(0) vec4<f32> {
    return u.color; // solid outline color, e.g. golden yellow
}