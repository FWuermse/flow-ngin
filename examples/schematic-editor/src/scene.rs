use crate::{
    blueprints::{self, GuideCache},
    document::BlockType,
    editor::Editor,
    geometry,
    ui::PANEL,
};
use bytemuck::{Pod, Zeroable};
use cgmath::{InnerSpace, Point3};
use flow_ngin::{
    context::{Context, GPUResource, InitContext},
    data_structures::{
        block::{BuildingBlocks, WorldCoordMesh},
        instance::InstanceRaw,
        model::{Model, ModelVertex, Vertex},
        texture::Texture,
    },
    render::Render,
    resources::{AssetFiles, load_model_obj_from_files},
};
use std::{collections::HashMap, sync::Arc};
use wgpu::util::DeviceExt;
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Line {
    position: [f32; 3],
    color: [f32; 4],
}
impl Line {
    fn layout() -> wgpu::VertexBufferLayout<'static> {
        const ATTRS: [wgpu::VertexAttribute; 2] =
            wgpu::vertex_attr_array![0=>Float32x3,1=>Float32x4];
        wgpu::VertexBufferLayout {
            array_stride: 28,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &ATTRS,
        }
    }
}
const LINES: &str = r#"
struct Camera { pos:vec4<f32>, vp:mat4x4<f32> }
@group(0) @binding(0) var<uniform> camera:Camera;
struct Output { @builtin(position) position:vec4<f32>, @location(0) color:vec4<f32> }
@vertex fn vs_main(@location(0) position:vec3<f32>, @location(1) color:vec4<f32>)->Output {
 var out:Output; out.position=camera.vp*vec4<f32>(position,1.); out.color=color; return out;
}
@vertex fn vs_screen(@location(0) position:vec3<f32>, @location(1) color:vec4<f32>)->Output {
 var out:Output; out.position=vec4<f32>(position,1.); out.color=color; return out;
}
@fragment fn fs_main(in:Output)->@location(0) vec4<f32> { return in.color; }
"#;
// A shaded plane avoids backend-dependent line clipping and subpixel flicker.
// Both world axes are evaluated independently and fade before undersampling.
const GRID: &str = r#"
struct Camera { pos:vec4<f32>, vp:mat4x4<f32> }
@group(0) @binding(0) var<uniform> camera:Camera;
struct Output { @builtin(position) position:vec4<f32>, @location(0) world:vec2<f32>, @location(1) offset:vec2<f32> }
@vertex fn vs_main(@location(0) position:vec3<f32>)->Output {
 let world = position + vec3<f32>(floor(camera.pos.x), 0., floor(camera.pos.z));
 var out:Output;
 out.position = camera.vp * vec4<f32>(world, 1.);
 out.world = world.xz;
 out.offset = world.xz - camera.pos.xz;
 return out;
}
@fragment fn fs_main(in:Output)->@location(0) vec4<f32> {
 let footprint = max(fwidth(in.world), vec2<f32>(0.00001));
 // Blocks are centered at integer X/Z coordinates, with edges at half units.
 let distance = abs(fract(in.world) - 0.5);
 let coverage = (1. - smoothstep(vec2<f32>(0.), footprint, distance))
     * (1. - smoothstep(vec2<f32>(0.35), vec2<f32>(1.), footprint));
 let fade = 1. - smoothstep(40., 80., length(in.offset));
 return vec4<f32>(0.32, 0.23, 0.13, max(coverage.x, coverage.y) * fade * 0.65);
}
"#;
const GHOST: &str = r#"
struct Camera { pos:vec4<f32>, vp:mat4x4<f32> }
@group(0) @binding(0) var<uniform> camera:Camera;
@group(1) @binding(0) var<uniform> tint:vec4<f32>;
@vertex fn vs_main(@location(0) position:vec3<f32>, @location(5) m0:vec4<f32>, @location(6) m1:vec4<f32>, @location(7) m2:vec4<f32>, @location(8) m3:vec4<f32>)->@builtin(position) vec4<f32> {
 return camera.vp*mat4x4<f32>(m0,m1,m2,m3)*vec4<f32>(position,1.);
}
@fragment fn fs_main()->@location(0) vec4<f32> { return tint; }
"#;
struct Paint {
    buffer: wgpu::Buffer,
    bind: wgpu::BindGroup,
}
impl Paint {
    fn new(ctx: &Context, layout: &wgpu::BindGroupLayout, color: [f32; 4]) -> Self {
        let buffer = ctx
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Guide tint"),
                contents: bytemuck::bytes_of(&color),
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            });
        let bind = ctx.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Guide tint"),
            layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: buffer.as_entire_binding(),
            }],
        });
        Self { buffer, bind }
    }
}
pub struct Scene {
    batches: Vec<BuildingBlocks>,
    previews: Vec<BuildingBlocks>,
    pub guides: GuideCache,
    lines: Option<wgpu::Buffer>,
    line_count: u32,
    grid: Option<wgpu::Buffer>,
    grid_visible: bool,
    grid_pipeline: Option<wgpu::RenderPipeline>,
    marquee: Option<wgpu::Buffer>,
    marquee_count: u32,
    line_pipeline: Option<wgpu::RenderPipeline>,
    screen_pipeline: Option<wgpu::RenderPipeline>,
    ghost_pipeline: Option<wgpu::RenderPipeline>,
    tint_layout: Option<wgpu::BindGroupLayout>,
    ghost_tint: Option<Paint>,
    guide_tints: HashMap<u64, Paint>,
    guide_order: Vec<u64>,
    revision: u64,
    placed_revision: u64,
    excluded: Vec<u64>,
    pub error: Option<String>,
}
impl Scene {
    pub fn new(gpu: &InitContext) -> anyhow::Result<Self> {
        let files = AssetFiles::from([
            (
                "cube.obj".into(),
                include_bytes!("../../../assets/cube.obj").to_vec(),
            ),
            (
                "cube.mtl".into(),
                include_bytes!("../../../assets/cube.mtl").to_vec(),
            ),
            (
                "cube-diffuse.jpg".into(),
                include_bytes!("../../../assets/cube-diffuse.jpg").to_vec(),
            ),
            (
                "cube-normal.png".into(),
                include_bytes!("../../../assets/cube-normal.png").to_vec(),
            ),
        ]);
        let model = load_model_obj_from_files("cube.obj", &files, &gpu.device, &gpu.queue)?;
        let model = Arc::new(model);
        Ok(Self {
            batches: Self::batches(&model, gpu),
            previews: Self::batches(&model, gpu),
            guides: HashMap::new(),
            lines: None,
            line_count: 0,
            grid: None,
            grid_visible: true,
            grid_pipeline: None,
            marquee: None,
            marquee_count: 0,
            line_pipeline: None,
            screen_pipeline: None,
            ghost_pipeline: None,
            tint_layout: None,
            ghost_tint: None,
            guide_tints: HashMap::new(),
            guide_order: vec![],
            revision: u64::MAX,
            placed_revision: u64::MAX,
            excluded: vec![],
            error: None,
        })
    }
    fn batches(model: &Arc<Model>, gpu: &InitContext) -> Vec<BuildingBlocks> {
        BlockType::ALL
            .iter()
            .map(|_| BuildingBlocks::from_shared_model(0u32, &gpu.device, model.clone(), vec![]))
            .collect()
    }
    pub fn set_block_model(
        &mut self,
        kind: BlockType,
        files: &AssetFiles,
        gpu: &InitContext,
    ) -> anyhow::Result<()> {
        let model = Arc::new(load_model_obj_from_files(
            crate::assets::ENTRY,
            files,
            &gpu.device,
            &gpu.queue,
        )?);
        let index = BlockType::ALL.iter().position(|k| *k == kind).unwrap();
        self.batches[index] =
            BuildingBlocks::from_shared_model(0u32, &gpu.device, model.clone(), vec![]);
        self.previews[index] = BuildingBlocks::from_shared_model(0u32, &gpu.device, model, vec![]);
        self.revision = u64::MAX;
        self.placed_revision = u64::MAX;
        Ok(())
    }
    pub fn init(&mut self, ctx: &Context) {
        let tint = ctx
            .device
            .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Guide tint layout"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
            });
        self.ghost_tint = Some(Paint::new(ctx, &tint, [0.9, 0.65, 0.28, 0.5]));
        self.line_pipeline = Some(pipeline(
            ctx,
            LINES,
            "vs_main",
            &[Some(Line::layout())],
            &[Some(&ctx.camera.bind_group_layout)],
            true,
            true,
        ));
        self.grid_pipeline = Some(pipeline(
            ctx,
            GRID,
            "vs_main",
            &[Some(Line::layout())],
            &[Some(&ctx.camera.bind_group_layout)],
            false,
            true,
        ));
        let vertices = [
            [-96., 0.002, -96.],
            [96., 0.002, -96.],
            [96., 0.002, 96.],
            [-96., 0.002, -96.],
            [96., 0.002, 96.],
            [-96., 0.002, 96.],
        ]
        .map(|position| Line {
            position,
            color: [0.; 4],
        });
        self.grid = buffer(ctx, &vertices, "Unit grid plane");
        self.screen_pipeline = Some(pipeline(
            ctx,
            LINES,
            "vs_screen",
            &[Some(Line::layout())],
            &[Some(&ctx.camera.bind_group_layout)],
            true,
            false,
        ));
        self.ghost_pipeline = Some(pipeline(
            ctx,
            GHOST,
            "vs_main",
            &[Some(ModelVertex::desc()), Some(InstanceRaw::desc())],
            &[Some(&ctx.camera.bind_group_layout), Some(&tint)],
            false,
            true,
        ));
        self.tint_layout = Some(tint);
    }
    pub fn update(
        &mut self,
        ctx: &Context,
        editor: &Editor,
        rectangle: Option<([f64; 2], [f64; 2])>,
    ) {
        let excluded = editor
            .preview
            .as_ref()
            .map(|p| p.excluded())
            .unwrap_or_default();
        if self.placed_revision != editor.revision || self.excluded != excluded {
            for (i, kind) in BlockType::ALL.into_iter().enumerate() {
                self.batches[i].set_instances(
                    editor
                        .document
                        .blocks
                        .iter()
                        .filter(|b| b.kind == kind && !excluded.contains(&b.id))
                        .map(|b| b.transform.instance())
                        .collect(),
                );
                self.batches[i].write_to_buffer(&ctx.queue, &ctx.device);
            }
            self.placed_revision = editor.revision;
            self.excluded = excluded.clone();
        }
        if self.revision != editor.scene_revision {
            for (i, kind) in BlockType::ALL.into_iter().enumerate() {
                self.previews[i].set_instances(
                    editor
                        .preview
                        .iter()
                        .filter(|p| p.has_ray)
                        .flat_map(|p| p.blocks.iter())
                        .filter(|b| b.kind == kind)
                        .map(|b| b.transform.instance())
                        .collect(),
                );
                self.previews[i].write_to_buffer(&ctx.queue, &ctx.device);
            }
            if let Some(p) = &editor.preview {
                let color = if p.valid {
                    [0.9f32, 0.65, 0.28, 0.5]
                } else {
                    [0.85, 0.16, 0.08, 0.6]
                };
                ctx.queue.write_buffer(
                    &self.ghost_tint.as_ref().unwrap().buffer,
                    0,
                    bytemuck::bytes_of(&color),
                );
            }
            self.revision = editor.scene_revision;
        }
        // Retain undone guide GPU resources; history shares the original asset bytes.
        for guide in &editor.document.blueprints {
            if !self.guides.contains_key(&guide.id) {
                match blueprints::load(guide, &InitContext::from(ctx)) {
                    Ok(gpu) => {
                        self.guides.insert(guide.id, gpu);
                    }
                    Err(e) => {
                        self.error = Some(format!("Guide {}: {e:#}", guide.name));
                        continue;
                    }
                }
            }
            let gpu = self.guides.get_mut(&guide.id).unwrap();
            if gpu.transform != guide.transform {
                gpu.root.set_local_transform(0, guide.transform.instance());
                gpu.root.update_world_transform_all();
                gpu.root.write_to_buffers(&ctx.queue, &ctx.device);
                gpu.transform = guide.transform;
            }
            let paint = self.guide_tints.entry(guide.id).or_insert_with(|| {
                Paint::new(
                    ctx,
                    self.tint_layout.as_ref().unwrap(),
                    [0.65, 0.52, 0.34, guide.opacity],
                )
            });
            ctx.queue.write_buffer(
                &paint.buffer,
                0,
                bytemuck::bytes_of(&[0.65f32, 0.52, 0.34, guide.opacity]),
            );
        }
        let mut guides = editor
            .document
            .blueprints
            .iter()
            .filter(|g| g.visible && g.opacity > 0.)
            .collect::<Vec<_>>();
        guides.sort_by(|a, b| {
            let da = (Point3::from(a.transform.position) - ctx.camera.camera.position).magnitude2();
            let db = (Point3::from(b.transform.position) - ctx.camera.camera.position).magnitude2();
            db.total_cmp(&da)
        });
        self.guide_order = guides.iter().map(|g| g.id).collect();
        let mut lines = Vec::new();
        self.grid_visible = editor.settings.grid;
        for b in editor
            .document
            .blocks
            .iter()
            .filter(|b| editor.selected.contains(&b.id) && !excluded.contains(&b.id))
        {
            let c = geometry::corners(b.transform);
            for i in 0..8 {
                for axis in [1, 2, 4] {
                    let j = i ^ axis;
                    if i < j {
                        add_line(&mut lines, c[i].into(), c[j].into(), [1., 0.72, 0.28, 1.]);
                    }
                }
            }
        }
        self.line_count = lines.len() as u32;
        self.lines = buffer(ctx, &lines, "Editor lines");
        let mut marquee = Vec::new();
        if let Some((a, b)) = rectangle {
            let p = |x: f64, y: f64| {
                [
                    (x / ctx.config.width as f64 * 2. - 1.) as f32,
                    (1. - y / ctx.config.height as f64 * 2.) as f32,
                    0.,
                ]
            };
            let corners = [p(a[0], a[1]), p(b[0], a[1]), p(b[0], b[1]), p(a[0], b[1])];
            for i in 0..4 {
                add_line(
                    &mut marquee,
                    corners[i],
                    corners[(i + 1) % 4],
                    [0.95, 0.72, 0.38, 1.],
                );
            }
        }
        self.marquee_count = marquee.len() as u32;
        self.marquee = buffer(ctx, &marquee, "Selection rectangle");
    }
    pub fn render<'a, 'p>(&'a self) -> Render<'a, 'p>
    where
        'p: 'a,
    {
        let mut renders = self
            .batches
            .iter()
            .map(|b| b.get_render())
            .collect::<Vec<_>>();
        renders.push(Render::Custom(Box::new(move |ctx, pass| {
            let left = (PANEL as u32).min(ctx.config.width);
            if ctx.config.width <= left {
                return;
            }
            pass.set_scissor_rect(left, 0, ctx.config.width - left, ctx.config.height);
            pass.set_bind_group(0, &ctx.camera.bind_group, &[]);
            if self.grid_visible {
                pass.set_pipeline(self.grid_pipeline.as_ref().unwrap());
                pass.set_vertex_buffer(0, self.grid.as_ref().unwrap().slice(..));
                pass.draw(0..6, 0..1);
            }
            if let Some(lines) = &self.lines {
                pass.set_pipeline(self.line_pipeline.as_ref().unwrap());
                pass.set_vertex_buffer(0, lines.slice(..));
                pass.draw(0..self.line_count, 0..1);
            }
            pass.set_pipeline(self.ghost_pipeline.as_ref().unwrap());
            for id in &self.guide_order {
                if let (Some(gpu), Some(tint)) = (self.guides.get(id), self.guide_tints.get(id)) {
                    pass.set_bind_group(1, &tint.bind, &[]);
                    for instance in gpu.root.get_renders() {
                        pass.set_vertex_buffer(1, instance.instance.slice(..));
                        for mesh in &instance.model.meshes {
                            pass.set_vertex_buffer(0, mesh.vertex_buffer.slice(..));
                            pass.set_index_buffer(
                                mesh.index_buffer.slice(..),
                                wgpu::IndexFormat::Uint32,
                            );
                            pass.draw_indexed(0..mesh.num_elements, 0, 0..instance.amount as u32);
                        }
                    }
                }
            }
            pass.set_bind_group(1, &self.ghost_tint.as_ref().unwrap().bind, &[]);
            for batch in &self.previews {
                let instance = batch.to_instanced();
                if instance.amount == 0 {
                    continue;
                }
                pass.set_vertex_buffer(1, instance.instance.slice(..));
                for mesh in &instance.model.meshes {
                    pass.set_vertex_buffer(0, mesh.vertex_buffer.slice(..));
                    pass.set_index_buffer(mesh.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
                    pass.draw_indexed(0..mesh.num_elements, 0, 0..instance.amount as u32);
                }
            }
            if let Some(rectangle) = &self.marquee {
                pass.set_pipeline(self.screen_pipeline.as_ref().unwrap());
                pass.set_vertex_buffer(0, rectangle.slice(..));
                pass.draw(0..self.marquee_count, 0..1);
            }
            pass.set_scissor_rect(0, 0, ctx.config.width, ctx.config.height);
        })));
        Render::Composed(renders)
    }
}
fn add_line(lines: &mut Vec<Line>, a: [f32; 3], b: [f32; 3], color: [f32; 4]) {
    lines.push(Line { position: a, color });
    lines.push(Line { position: b, color });
}
fn buffer(ctx: &Context, lines: &[Line], name: &str) -> Option<wgpu::Buffer> {
    (!lines.is_empty()).then(|| {
        ctx.device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some(name),
                contents: bytemuck::cast_slice(lines),
                usage: wgpu::BufferUsages::VERTEX,
            })
    })
}
fn pipeline(
    ctx: &Context,
    source: &str,
    vertex: &str,
    buffers: &[Option<wgpu::VertexBufferLayout<'_>>],
    layouts: &[Option<&wgpu::BindGroupLayout>],
    lines: bool,
    depth: bool,
) -> wgpu::RenderPipeline {
    let shader = ctx
        .device
        .create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Editor overlay"),
            source: wgpu::ShaderSource::Wgsl(source.into()),
        });
    let layout = ctx
        .device
        .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Editor overlay"),
            bind_group_layouts: layouts,
            ..Default::default()
        });
    ctx.device
        .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Editor overlay"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some(vertex),
                buffers,
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: ctx.config.format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: if lines {
                    wgpu::PrimitiveTopology::LineList
                } else {
                    wgpu::PrimitiveTopology::TriangleList
                },
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: Texture::DEPTH_FORMAT,
                depth_write_enabled: Some(false),
                depth_compare: Some(if depth {
                    wgpu::CompareFunction::LessEqual
                } else {
                    wgpu::CompareFunction::Always
                }),
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: wgpu::MultisampleState {
                count: ctx.anti_aliasing.sample_count(),
                ..Default::default()
            },
            multiview_mask: None,
            cache: None,
        })
}
