// Vertex shader

struct Camera {
    view_pos: vec4<f32>,
    view_proj: mat4x4<f32>,
}
@group(1) @binding(0)
var<uniform> camera: Camera;

struct Light {
    position: vec3<f32>,
    radius: f32,
    color: vec3<f32>,
}
@group(2) @binding(0)
var<uniform> light: Light;

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) tex_coords: vec2<f32>,
    @location(2) normal: vec3<f32>,
    @location(3) tangent: vec3<f32>,
    @location(4) bitangent: vec3<f32>,
}
struct InstanceInput {
    @location(5) model_matrix_0: vec4<f32>,
    @location(6) model_matrix_1: vec4<f32>,
    @location(7) model_matrix_2: vec4<f32>,
    @location(8) model_matrix_3: vec4<f32>,
    @location(9) normal_matrix_0: vec3<f32>,
    @location(10) normal_matrix_1: vec3<f32>,
    @location(11) normal_matrix_2: vec3<f32>,
    @location(12) handedness: f32,
}

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) tex_coords: vec2<f32>,
    @location(1) tangent_position: vec3<f32>,
    @location(2) tangent_light_position: vec3<f32>,
    @location(3) tangent_view_position: vec3<f32>,
}

@vertex
fn vs_main(
    model: VertexInput,
    instance: InstanceInput,
) -> VertexOutput {
    let model_matrix = mat4x4<f32>(
        instance.model_matrix_0,
        instance.model_matrix_1,
        instance.model_matrix_2,
        instance.model_matrix_3,
    );
    let normal_matrix = mat3x3<f32>(
        instance.normal_matrix_0,
        instance.normal_matrix_1,
        instance.normal_matrix_2,
    );
    let handedness = instance.handedness;

    // Construct the tangent matrix
    let world_normal = normalize(normal_matrix * model.normal) * handedness;
    let world_tangent = normalize(normal_matrix * model.tangent) * handedness;
    let world_bitangent = normalize(normal_matrix * model.bitangent) * handedness;
    let tangent_matrix = transpose(mat3x3<f32>(
        world_tangent,
        world_bitangent,
        world_normal,
    ));

    let world_position = model_matrix * vec4<f32>(model.position, 1.0);

    var out: VertexOutput;
    out.clip_position = camera.view_proj * world_position;
    out.tex_coords = model.tex_coords;
    out.tangent_position = tangent_matrix * world_position.xyz;
    out.tangent_view_position = tangent_matrix * camera.view_pos.xyz;
    out.tangent_light_position = tangent_matrix * light.position;
    return out;
}

// Fragment shader

@group(0) @binding(0)
var t_diffuse: texture_2d<f32>;
@group(0)@binding(1)
var s_diffuse: sampler;
@group(0)@binding(2)
var t_normal: texture_2d<f32>;
@group(0) @binding(3)
var s_normal: sampler;

struct MaterialParams {
    base_color_factor: vec4<f32>,
    metallic: f32,
    roughness: f32,
}
@group(0) @binding(4)
var<uniform> material: MaterialParams;

const PI: f32 = 3.14159265359;

fn distribution_ggx(n_dot_h: f32, alpha: f32) -> f32 {
    let a2 = alpha * alpha;
    let denom = n_dot_h * n_dot_h * (a2 - 1.0) + 1.0;
    return a2 / (PI * denom * denom);
}

fn geometry_smith(n_dot_v: f32, n_dot_l: f32, alpha: f32) -> f32 {
    let r = sqrt(alpha) + 1.0;
    let k = (r * r) / 8.0;
    let gv = n_dot_v / (n_dot_v * (1.0 - k) + k);
    let gl = n_dot_l / (n_dot_l * (1.0 - k) + k);
    return gv * gl;
}

fn fresnel_schlick(cos_theta: f32, f0: vec3<f32>) -> vec3<f32> {
    return f0 + (vec3<f32>(1.0) - f0) * pow(1.0 - cos_theta, 5.0);
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let tex_sample = textureSample(t_diffuse, s_diffuse, in.tex_coords);
    let albedo = tex_sample * material.base_color_factor;
    let object_normal = textureSample(t_normal, s_normal, in.tex_coords);

    var n = object_normal.xyz * 2.0 - 1.0;
    n.z = abs(n.z);
    n = normalize(n);

    let light_vec = in.tangent_light_position - in.tangent_position;
    let dist_to_light = length(light_vec);
    let l = light_vec / max(dist_to_light, 1e-4);
    let v = normalize(in.tangent_view_position - in.tangent_position);
    let h = normalize(v + l);

    let n_dot_l = max(dot(n, l), 0.0);
    let n_dot_v = max(dot(n, v), 0.0);
    let n_dot_h = max(dot(n, h), 0.0);
    let v_dot_h = max(dot(v, h), 0.0);

    let roughness = max(material.roughness, 0.045);
    let alpha = roughness * roughness;
    let alpha_eff = clamp(alpha + light.radius / (2.0 * dist_to_light), alpha, 1.0);
    let f0 = mix(vec3<f32>(0.04), albedo.rgb, material.metallic);

    let d = distribution_ggx(n_dot_h, alpha_eff);
    let g = geometry_smith(n_dot_v, n_dot_l, alpha_eff);
    let f = fresnel_schlick(v_dot_h, f0);

    let specular = (d * g * f) / max(4.0 * n_dot_v * n_dot_l, 1e-4);
    let kd = (vec3<f32>(1.0) - f) * (1.0 - material.metallic);
    let diffuse = kd * albedo.rgb;

    let ambient = vec3<f32>(0.1) * albedo.rgb;
    let color = ambient + (diffuse + specular) * light.color * n_dot_l;

    return vec4<f32>(color, albedo.a);
}
