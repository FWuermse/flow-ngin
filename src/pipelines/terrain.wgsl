// Vertex shader

struct Camera {
    view_pos: vec4<f32>,
    view_proj: mat4x4<f32>,
}
@group(1) @binding(0)
var<uniform> camera: Camera;

struct Light {
    position: vec3<f32>,
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
}

// Data passed from vertex to fragment shader
struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) tex_coords: vec2<f32>,
    @location(1) world_pos: vec3<f32>,
    @location(2) tangent_view_position: vec3<f32>,
    @location(3) tangent_light_position: vec3<f32>,
    @location(4) world_normal: vec3<f32>,
};

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

    // Construct the tangent matrix
    let world_normal = normalize(normal_matrix * model.normal);
    let world_tangent = normalize(normal_matrix * model.tangent);
    let world_bitangent = normalize(normal_matrix * model.bitangent);
    let tangent_matrix = transpose(mat3x3<f32>(
        world_tangent,
        world_bitangent,
        world_normal,
    ));

    let world_position = model_matrix * vec4<f32>(model.position, 1.0);

    var out: VertexOutput;
    out.clip_position = camera.view_proj * world_position;
    out.tex_coords = model.tex_coords;
    out.world_pos = tangent_matrix * world_position.xyz;
    out.tangent_view_position = tangent_matrix * camera.view_pos.xyz;
    out.tangent_light_position = tangent_matrix * light.position;
    out.world_normal = world_normal;
    return out;
}

// Fragment shader

struct PathPoint {
    point: vec4<f32>,
}

const max_points = 127;

@group(0) @binding(0) var t_diffuse_grass: texture_2d<f32>;
@group(0) @binding(1) var t_normal_grass: texture_2d<f32>;
@group(0) @binding(2) var t_diffuse_rock: texture_2d<f32>;
@group(0) @binding(3) var t_normal_rock: texture_2d<f32>;
@group(0) @binding(4) var t_diffuse_sand: texture_2d<f32>;
@group(0) @binding(5) var t_normal_sand: texture_2d<f32>;
@group(0) @binding(6) var t_diffuse_path: texture_2d<f32>;
@group(0) @binding(7) var t_normal_path: texture_2d<f32>;
@group(0) @binding(8) var<uniform> path_pos: array<PathPoint, max_points>;
@group(0) @binding(9) var s_sampler: sampler;

fn blend(a: vec4<f32>, b: vec4<f32>, w: f32) -> vec4<f32> {
    return mix(a, b, saturate(w));
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let sand_height = -1.0;
    let grass_height = 2.0;
    let rock_height = 6.0;
    let transition_y_range = 2.0;
    
    let rock_slope_start = 0.4;
    let rock_slope_end = 0.7;
    
    let grass_color = textureSample(t_diffuse_grass, s_sampler, in.tex_coords);
    let grass_normal_map = textureSample(t_normal_grass, s_sampler, in.tex_coords).xyz;
    
    let rock_color = textureSample(t_diffuse_rock, s_sampler, in.tex_coords);
    let rock_normal_map = textureSample(t_normal_rock, s_sampler, in.tex_coords).xyz;
    
    let sand_color = textureSample(t_diffuse_sand, s_sampler, in.tex_coords);
    let sand_normal_map = textureSample(t_normal_sand, s_sampler, in.tex_coords).xyz;

    let path_color = textureSample(t_diffuse_path, s_sampler, in.tex_coords);
    let path_normal_map = textureSample(t_normal_path, s_sampler, in.tex_coords).xyz;
    
    let world_y = in.world_pos.z;
    
    let world_normal = normalize(in.world_normal);
    let slope = 1.0 - world_normal.y;
    
    let sand_grass_blend = smoothstep(sand_height, sand_height + transition_y_range, world_y);
    let grass_rock_blend_by_height = smoothstep(grass_height, grass_height + transition_y_range, world_y);
    let grass_rock_blend_by_slope = smoothstep(rock_slope_start, rock_slope_end, slope);
    let rock_blend = max(grass_rock_blend_by_height, grass_rock_blend_by_slope);
    
    var final_color = blend(sand_color, grass_color, sand_grass_blend);
    var final_normal_map = blend(vec4(sand_normal_map, 0.0), vec4(grass_normal_map, 0.0), sand_grass_blend).xyz;
    
    final_color = blend(final_color, rock_color, rock_blend);
    final_normal_map = blend(vec4(final_normal_map, 0.0), vec4(rock_normal_map, 0.0), rock_blend).xyz;

    let path_thickness = 1.5;

    for (var i: u32 = 0u; i < max_points - 1u;  i = i + 1u) {
        // todo: support 3d paths
        let p1 = vec2<f32>(path_pos[i].point.x, -path_pos[i].point.y);
        let p2 = vec2<f32>(path_pos[i].point.z, -path_pos[i].point.a);
        if (p1.x == 0.0 && p1.y == 0.0 && p2.x == 0.0 && p2.y == 0.0) {
            continue;
        }
        let frag_pos = in.world_pos.xy;
        let segment_vec = p2 - p1;
        let frag_vec = frag_pos - p1;
        let seg_len_sq = dot(segment_vec, segment_vec);
        var dist = 0.0;
        // Avoid division by zero
        if (seg_len_sq > 0.0) {
            let t = clamp(dot(frag_vec, segment_vec) / seg_len_sq, 0.0, 1.0);
            let closest_point_on_segment = p1 + t * segment_vec;
            dist = distance(frag_pos, closest_point_on_segment);
        } else {
            dist = distance(frag_pos, p1);
        }
        let blend = 1.0 - smoothstep(path_thickness * 0.1, path_thickness, dist);
        if (blend > 0.0) {
            final_color = mix(final_color, path_color, blend);
            final_normal_map = mix(final_normal_map, path_normal_map, blend);
        }
    }

    let tangent_normal = final_normal_map * 2.0 - 1.0;
    let light_dir = normalize(in.tangent_light_position - in.world_pos);
    let view_dir = normalize(in.tangent_view_position - in.world_pos);
    let half_dir = normalize(view_dir + light_dir);

    let diffuse_strength = max(dot(tangent_normal, light_dir), 0.0);
    let diffuse_color = light.color * diffuse_strength;

    let specular_strength = pow(max(dot(tangent_normal, half_dir), 0.0), 64.0);
    let specular_color = specular_strength * light.color;

    let ambient_strength = 0.1;
    let ambient_color = light.color * ambient_strength;
  
    let result = (ambient_color + diffuse_color + specular_color) * final_color.xyz;
    
    return vec4<f32>(result, final_color.a);
}
