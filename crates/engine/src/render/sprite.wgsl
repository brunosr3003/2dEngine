struct Camera {
    view_proj: mat4x4<f32>,
};

@group(0) @binding(0) var<uniform> camera: Camera;
@group(0) @binding(1) var atlas_tex: texture_2d<f32>;
@group(0) @binding(2) var atlas_sampler: sampler;

struct VertexIn {
    @location(0) corner: vec2<f32>,
    @location(1) inst_pos: vec2<f32>,
    @location(2) inst_size: vec2<f32>,
    @location(3) inst_uv_min: vec2<f32>,
    @location(4) inst_uv_max: vec2<f32>,
    @location(5) inst_tint: vec4<f32>,
    @location(6) inst_rot: f32,
};

struct VertexOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) tint: vec4<f32>,
};

@vertex
fn vs_main(in: VertexIn) -> VertexOut {
    let c = cos(in.inst_rot);
    let s = sin(in.inst_rot);
    let scaled = vec2<f32>(in.corner.x * in.inst_size.x, in.corner.y * in.inst_size.y);
    let rotated = vec2<f32>(scaled.x * c - scaled.y * s, scaled.x * s + scaled.y * c);
    let world = rotated + in.inst_pos;

    var out: VertexOut;
    out.pos = camera.view_proj * vec4<f32>(world, 0.0, 1.0);
    // Corner [-0.5,0.5] -> [0,1]. Flip Y para atlas com origem top-left.
    let uv_t = in.corner + vec2<f32>(0.5, 0.5);
    out.uv = mix(in.inst_uv_min, in.inst_uv_max, vec2<f32>(uv_t.x, 1.0 - uv_t.y));
    out.tint = in.inst_tint;
    return out;
}

@fragment
fn fs_main(in: VertexOut) -> @location(0) vec4<f32> {
    let tex = textureSample(atlas_tex, atlas_sampler, in.uv);
    return tex * in.tint;
}
