@group(0) @binding(0) var<uniform> info: vec4<f32>;
@group(0) @binding(1) var labels: texture_2d<f32>;
@group(0) @binding(2) var label_sampler: sampler;

@vertex fn vs_main(@builtin(vertex_index) vertex: u32) -> @builtin(position) vec4<f32> {
    let positions = array<vec2<f32>, 3>(vec2(-1.0, -1.0), vec2(3.0, -1.0), vec2(-1.0, 3.0));
    return vec4(positions[vertex], 0.0, 1.0);
}

fn rx(p: vec3<f32>, a: f32) -> vec3<f32> {
    return vec3(p.x, cos(a) * p.y - sin(a) * p.z, sin(a) * p.y + cos(a) * p.z);
}
fn ry(p: vec3<f32>, a: f32) -> vec3<f32> {
    return vec3(cos(a) * p.x + sin(a) * p.z, p.y, -sin(a) * p.x + cos(a) * p.z);
}
fn rz(p: vec3<f32>, a: f32) -> vec3<f32> {
    return vec3(cos(a) * p.x - sin(a) * p.y, sin(a) * p.x + cos(a) * p.y, p.z);
}
// Inverse CSS transforms, converted from downward Y to upward Y.
fn outer_local(p: vec3<f32>, t: f32) -> vec3<f32> {
    return ry(rx(p, -0.174533), -(0.785398 - t * 6.2831853 / 7.0));
}
fn inner_local(p: vec3<f32>, t: f32) -> vec3<f32> {
    let phase = t * 6.2831853 / 14.0;
    return rz(ry(rx(p, phase), -(0.785398 + phase * 2.0)), -phase);
}
fn box_hit(origin: vec3<f32>, direction: vec3<f32>, half_size: f32) -> vec2<f32> {
    let safe_direction = select(vec3(-1.0), vec3(1.0), direction >= vec3(0.0)) * max(abs(direction), vec3(0.00001));
    let a = (vec3(-half_size) - origin) / safe_direction;
    let b = (vec3(half_size) - origin) / safe_direction;
    let near = min(a, b);
    let far = max(a, b);
    return vec2(max(max(near.x, near.y), near.z), min(min(far.x, far.y), far.z));
}
fn normal_at(p: vec3<f32>) -> vec3<f32> {
    let q = abs(p);
    if q.x > max(q.y, q.z) { return vec3(sign(p.x), 0.0, 0.0); }
    if q.y > q.z { return vec3(0.0, sign(p.y), 0.0); }
    return vec3(0.0, 0.0, sign(p.z));
}
fn hash2(p: vec2<f32>) -> vec2<f32> {
    let q = vec2(dot(p, vec2(127.1, 311.7)), dot(p, vec2(269.5, 183.3)));
    return fract(sin(q) * 43758.5453);
}
fn noise(p: vec2<f32>) -> f32 {
    let cell = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    return mix(mix(hash2(cell).x, hash2(cell + vec2(1.0, 0.0)).x, u.x),
        mix(hash2(cell + vec2(0.0, 1.0)).x, hash2(cell + vec2(1.0)).x, u.x), u.y);
}
fn surface_uv(p: vec3<f32>, n: vec3<f32>) -> vec2<f32> {
    if abs(n.y) > 0.5 { return vec2(p.x, p.z * n.y); }
    if abs(n.x) > 0.5 { return vec2(-p.z * n.x, p.y); }
    return vec2(p.x * n.z, p.y);
}
fn surface_vector(v: vec2<f32>, n: vec3<f32>) -> vec3<f32> {
    if abs(n.y) > 0.5 { return vec3(v.x, 0.0, v.y * n.y); }
    if abs(n.x) > 0.5 { return vec3(0.0, v.y, -v.x * n.x); }
    return vec3(v.x * n.z, v.y, 0.0);
}
struct Moisture {
    slope: vec2<f32>,
    water: f32,
    fog: f32,
}
fn moisture(p: vec3<f32>, n: vec3<f32>) -> Moisture {
    let uv = surface_uv(p, n);
    let seed = dot(n, vec3(13.0, 29.0, 47.0));
    let fog = 0.42 + noise(uv * vec2(5.0, 1.5) + seed) * 0.20;
    if abs(n.y) > 0.5 { return Moisture(vec2(0.0), 0.0, fog); }
    var slope = vec2(0.0);
    var water = 0.0;
    for (var i = 0; i < 2; i += 1) {
        let random = hash2(vec2(f32(i) + 3.0, seed));
        let start = 0.35 + random.y * 0.6;
        let end = start - (0.45 + random.x * 0.65);
        let center = -0.65 + f32(i) * 1.1 + random.x * 0.25;
        let bend = sin(uv.y * 4.0 + random.y * 6.0) * 0.012;
        let width = 0.018 + random.y * 0.012;
        let x = (uv.x - center - bend) / width;
        let fade = smoothstep(end, end + 0.12, uv.y)
            * (1.0 - smoothstep(start - 0.18, start, uv.y));
        let trail = exp(-x * x * 1.5) * fade;
        let bead_uv = vec2(uv.x - center - sin(end * 4.0 + random.y * 6.0) * 0.012, uv.y - end)
            / (width * vec2(1.15, 1.8));
        let bead = exp(-dot(bead_uv, bead_uv) * 1.8);
        slope += vec2(x * trail * 0.035, 0.0) + bead_uv * bead * 0.06;
        water += trail * 0.10 + bead * 0.12;
    }
    return Moisture(slope, water, fog - water * 0.18);
}
fn outer_world(p: vec3<f32>, t: f32) -> vec3<f32> {
    return rx(ry(p, 0.785398 - t * 6.2831853 / 7.0), 0.174533);
}
fn label_tap(uv: vec2<f32>, face: f32, lod: f32) -> f32 {
    let tile = clamp(uv, vec2(0.0), vec2(1.0));
    // Higher mip levels merge neighboring atlas tiles.
    return textureSampleLevel(labels, label_sampler, vec2(tile.x, (face + tile.y) * 0.25), min(lod, 5.0)).a;
}
fn face_label(color: vec3<f32>, p: vec3<f32>, direction: vec3<f32>, scale: f32) -> vec3<f32> {
    let normal = normal_at(p);
    if abs(normal.y) > 0.5 { return color; }
    let face = select(select(2.0, 0.0, normal.z > 0.0), select(3.0, 1.0, normal.x > 0.0), abs(normal.x) > 0.5);
    let face_uv = surface_uv(p, normal) * vec2(0.5, -0.5) + 0.5;
    let uv = (face_uv - 0.5) / vec2(0.96, 0.30) + 0.5;
    if any(uv < vec2(0.0)) || any(uv > vec2(1.0)) { return color; }
    let facing = abs(dot(normal, normalize(direction)));
    let lod = clamp(log2(512.0 / (115.2 * scale * max(facing, 0.12))), 0.0, 5.0);
    let ink = label_tap(uv, face, lod);
    let shadow = label_tap(uv - vec2(0.004, 0.022), face, lod + 0.7);
    let beneath = color * (1.0 - shadow * 0.78);
    return mix(beneath, vec3(0.92, 0.83, 0.66), ink * 0.96);
}
fn highlight(reflection: vec3<f32>, p: vec3<f32>, light: vec3<f32>, sharpness: f32) -> f32 {
    return pow(max(dot(reflection, normalize(light - p)), 0.0), sharpness);
}
fn glass(color: vec3<f32>, p: vec3<f32>, direction: vec3<f32>, wet: Moisture, t: f32) -> vec3<f32> {
    let flat = normal_at(p);
    let bevel = normalize(sign(p) * smoothstep(vec3(0.965), vec3(1.005), abs(p)));
    let normal = normalize(bevel + surface_vector(wet.slope, flat));
    let view = -normalize(direction);
    let facing = max(dot(normal, view), 0.0);
    let fresnel = 0.04 + 0.96 * pow(1.0 - facing, 5.0);
    let reflection = outer_world(reflect(-view, normal), t);
    let world_p = outer_world(p, t);
    let environment = vec3(0.07, 0.12, 0.14) * smoothstep(-0.3, 0.9, reflection.y)
        + vec3(2.5, 3.0, 3.2) * highlight(reflection, world_p, vec3(-3.0, 4.0, 4.0), 12.0)
        + vec3(1.6, 1.3, 0.9) * highlight(reflection, world_p, vec3(4.0, 1.5, 3.0), 36.0)
        + vec3(0.65, 1.04, 1.10) * highlight(reflection, world_p, vec3(-3.0, 3.0, -4.0), 10.0)
        + vec3(0.72, 0.81, 0.81) * highlight(reflection, world_p, vec3(0.0, 4.0, -5.0), 16.0)
            * (0.45 + 0.55 * smoothstep(-1.0, 1.0, world_p.x));
    let thickness = 0.10 / max(abs(dot(flat, view)), 0.20);
    let absorption = exp(-vec3(1.4, 0.35, 0.25) * thickness);
    let transmission = color * absorption * (1.0 - fresnel);
    let light = normalize(outer_local(vec3(-3.0, 5.0, 6.0), t));
    let diffuse = max(dot(flat, light), 0.0);
    let frost = vec3(0.028, 0.048, 0.052) * (0.3 + diffuse * 0.9)
        * (0.6 + wet.fog * 0.7) * (1.0 - wet.water * 0.22);
    let glint = vec3(0.45, 0.53, 0.48) * wet.water * pow(max(dot(normal,
        normalize(view + light)), 0.0), 24.0);
    return transmission + environment * (fresnel + 0.025 * wet.fog) + frost + glint;
}
// Gaussian blur taps distributed along a golden-angle spiral.
fn frost_sample(i: i32) -> vec2<f32> {
    let radius = sqrt(-0.5 * log(1.0 - (f32(i) + 0.5) / 32.0));
    let angle = f32(i) * 2.39996323;
    return radius * vec2(cos(angle), sin(angle));
}
fn inner_light(origin: vec3<f32>, direction: vec3<f32>, light: vec3<f32>) -> vec3<f32> {
    let hit = box_hit(origin, direction, 0.5);
    if hit.y < max(hit.x, 0.0) { return vec3(0.0); }
    let p = origin + direction * hit.x;
    let normal = normal_at(p);
    let diffuse = max(dot(normal, light), 0.0);
    let depth = max(hit.y - max(hit.x, 0.0), 0.0);
    let volume = 1.0 - exp(-depth * 3.0);
    let core = exp(-dot(p, p) * 2.0);
    return vec3(0.48, 1.8, 0.065) * (0.25 + diffuse * 0.75 + volume * 0.45)
        + vec3(0.9, 1.3, 0.42) * core * 0.65;
}

fn soft_shadow(p: vec3<f32>, angle: f32, half_size: f32) -> f32 {
    let d = abs(rz(p, angle).xy) - half_size;
    let distance = length(max(d, vec2(0.0))) + min(max(d.x, d.y), 0.0);
    return 1.0 - smoothstep(-0.14, 0.24, distance);
}

fn scene(pixel: vec2<f32>, size: vec2<f32>, scale: f32, t: f32) -> vec3<f32> {
    var color = vec3(0.012983);
    let centered = (pixel - size * 0.5) / scale;
    let origin = vec3(0.0, 0.0, 12.5);
    let direction = normalize(vec3(centered.x / 60.0, -centered.y / 60.0, -12.5));

    let shadow_tilt = 1.919862;
    let shadow_normal = rx(vec3(0.0, 0.0, 1.0), shadow_tilt);
    let denominator = dot(shadow_normal, direction);
    if abs(denominator) > 0.0001 {
        let distance = (1.333333 - dot(shadow_normal, origin)) / denominator;
        if distance > 0.0 {
            let floor_point = rx(origin + direction * distance, -shadow_tilt);
            let phase = t * 6.2831853 / 7.0;
            let shadow = soft_shadow(floor_point, 0.785398 - phase, 1.0) * 0.18
                + soft_shadow(floor_point, 0.785398 + phase, 0.5) * 0.10;
            color *= 1.0 - shadow;
            let pool = exp(-dot(floor_point.xy, floor_point.xy * vec2(0.8, 1.5)));
            color += vec3(0.012, 0.025, 0.002) * pool;
        }
    }
    color += vec3(0.016, 0.035, 0.004) * exp(-dot(centered, centered) / 3800.0)
        + vec3(0.025, 0.045, 0.006) * exp(-dot(centered, centered) / 1400.0);
    if any(abs(centered) > vec2(150.0)) { return color; }

    let light = normalize(vec3(-3.0, 5.0, 6.0));
    let outer_origin = outer_local(origin, t);
    let outer_direction = outer_local(direction, t);
    let outer = box_hit(outer_origin, outer_direction, 1.0);
    if outer.y < max(outer.x, 0.0) { return color; }

    let front = outer_origin + outer_direction * outer.x;
    let flat = normal_at(front);
    let wet = moisture(front, flat);
    let slope = surface_vector(wet.slope, flat);
    let surface_normal = normalize(flat + slope);
    let through_glass = refract(outer_direction, surface_normal, 1.0 / 1.45);
    let pane_offset = (through_glass - outer_direction) * 0.11;
    let lens_offset = slope * -0.08;
    let refracted_origin = origin + outer_world(pane_offset + lens_offset, t);
    let inner_origin = inner_local(refracted_origin, t);
    let inner_direction = inner_local(direction, t);
    let inner_lamp = inner_local(light, t);
    let screen_x = inner_local(vec3(1.0, 0.0, 0.0), t);
    let screen_y = inner_local(vec3(0.0, 1.0, 0.0), t);
    let roughness = mix(0.006 + wet.fog * 0.005, 0.006, wet.water * 0.5);
    var emission = vec3(0.0);
    for (var i = 0; i < 32; i += 1) {
        let offset = frost_sample(i) * roughness;
        let scattered_ray = normalize(inner_direction + screen_x * offset.x + screen_y * offset.y);
        emission += inner_light(inner_origin, scattered_ray, inner_lamp) / 32.0;
    }
    let closest = origin + direction * max(-dot(origin, direction), 0.0);
    let radius2 = dot(closest, closest);
    let halo = exp(-radius2 * 2.5);
    let bloom = vec3(0.45, 1.4, 0.065) * exp(-radius2 * 2.8);
    color += emission * 0.9 + bloom * 0.28 + vec3(0.09, 0.20, 0.012) * halo;
    color = glass(color, front, outer_direction, wet, t);
    color = color / (vec3(1.0) + color);
    return face_label(color, front, outer_direction, scale);
}

@fragment fn fs_main(@builtin(position) pixel: vec4<f32>) -> @location(0) vec4<f32> {
    let size = max(info.yz, vec2(1.0));
    let scale = min(1.0, min(size.x / 400.0, size.y / 480.0));
    var color = vec3(0.0);
    for (var i = 0; i < 4; i += 1) {
        let offset = vec2(f32(i % 2), f32(i / 2)) * 0.5 - 0.25;
        color += scene(pixel.xy + offset, size, scale, info.x) * 0.25;
    }
    return vec4(color, 1.0);
}
