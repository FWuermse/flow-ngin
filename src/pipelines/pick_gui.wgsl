// Vertex shader

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) tex_coords: vec2<f32>,
}

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) tex_coords: vec2<f32>,
}

@vertex
fn vs_main(
    model: VertexInput,
) -> VertexOutput {
    var out: VertexOutput;
    out.tex_coords = model.tex_coords;
    out.clip_position = vec4<f32>(model.position, 1.0);
    return out;
}

// Fragment shader

struct PickUniforms {
    id: vec4<u32>,
};

@group(0) @binding(0)
var<uniform> pickUniforms: PickUniforms;

@fragment
fn fs_main(in: VertexOutput) -> @location(0) u32 {
    return pickUniforms.id[0];
}
