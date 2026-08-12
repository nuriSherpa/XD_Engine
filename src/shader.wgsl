struct Uniforms {
    mvp: mat4x4<f32>,
    color: vec4<f32>, // color.a > 0.0 => draw flat `color.rgb` instead of normal-shaded
};

@group(0) @binding(0)
var<uniform> uniforms: Uniforms;

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) normal: vec3<f32>,
};

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    out.clip_position = uniforms.mvp * vec4<f32>(in.position, 1.0);
    out.normal = in.normal;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    if (uniforms.color.a > 0.0) {
        return vec4<f32>(uniforms.color.rgb, 1.0);
    }

    let n = normalize(in.normal);
    let normal_color = n * 0.5 + vec3<f32>(0.5, 0.5, 0.5);
    let light_dir = normalize(vec3<f32>(0.5, 0.8, 0.6));
    let diffuse = max(dot(n, light_dir), 0.0);
    let ambient = 0.3;
    let brightness = ambient + diffuse * 0.7;
    return vec4<f32>(normal_color * brightness, 1.0);
}