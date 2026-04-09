// Vertex shader

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) tex_coords: vec2<f32>,
}

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) tex_coords: vec2<f32>,
}

struct ScreenSize {
    width: f32,
    height: f32,
    _pad0: f32,
    _pad1: f32,
}

@group(1) @binding(0)
var<uniform> screen: ScreenSize;

@vertex
fn vs_main(
    model: VertexInput,
) -> VertexOutput {
    var out: VertexOutput;
    out.tex_coords = model.tex_coords;
    let ndc_x = -1.0 + 2.0 * model.position.x / screen.width;
    let ndc_y =  1.0 - 2.0 * model.position.y / screen.height;
    out.clip_position = vec4<f32>(ndc_x, ndc_y, model.position.z, 1.0);
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
