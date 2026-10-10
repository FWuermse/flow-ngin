mod assets;
mod blueprints;
pub mod codec;
pub mod document;
pub mod editor;
mod files;
pub mod geometry;
mod input;
pub mod project;
mod scene;
#[cfg(test)]
mod tests;
mod ui;

use blueprints::GuideCache;
use document::{BlockType, Document, Edit};
use editor::{Editor, Mode, Settings};
use files::FileIntent;
use flow_ngin::{
    WindowEvent,
    context::{Context, InitContext},
    flow::{FlowConstructor, GraphicsFlow, Out},
    render::Render,
    resources::AssetFiles,
};
use input::Input;
use std::collections::VecDeque;

#[derive(Clone)]
pub enum Action {
    DismissStatus,
    Paint(BlockType),
    Mode(Mode),
    Grid(bool),
    Height(bool),
    Axis(usize),
    Move,
    Rotate,
    Copy,
    Paste,
    Undo,
    Redo,
    DeleteSelection,
    Escape,
    Tab(bool),
    Guide(u64),
    GuidePage(i32),
    ApplyGuide,
    ToggleGuide,
    RemoveGuide,
    Files(FileIntent),
    Export(bool),
    ConfirmOpen,
    CancelOpen,
}
pub enum Event {
    BlockAsset(BlockType, Result<AssetFiles, String>),
    AtlasAsset(Result<Vec<u8>, String>),
    Action(Action),
    Files(FileIntent, AssetFiles),
    Saved { revision: u64, project: bool },
    Error(String),
    Idle,
}
struct EditorFlow {
    ui: ui::Ui,
    scene: scene::Scene,
    input: Input,
    events: VecDeque<Event>,
    pending_open: Option<(Document, Settings, GuideCache)>,
    assets_remaining: usize,
    asset_errors: Vec<String>,
}
impl EditorFlow {
    fn new(gpu: &InitContext) -> Self {
        Self {
            ui: ui::Ui::new(),
            scene: scene::Scene::new(gpu).expect("Embedded cube assets must load"),
            input: Input::default(),
            events: VecDeque::new(),
            pending_open: None,
            assets_remaining: 0,
            asset_errors: Vec::new(),
        }
    }
    fn asset_finished(&mut self, state: &mut Editor, label: &str, result: Result<(), String>) {
        self.assets_remaining = self.assets_remaining.saturating_sub(1);
        if let Err(error) = result {
            self.asset_errors.push(format!("{label}: {error}"));
        }
        // Background block downloads must not hide file import results/errors.
        if !state.status.starts_with("Loading block") {
            return;
        }
        state.status = if self.assets_remaining > 0 {
            format!(
                "Loading block assets ({} remaining)…",
                self.assets_remaining
            )
        } else if self.asset_errors.is_empty() {
            "Block models and icons loaded; gold uses the placeholder cube".into()
        } else {
            format!(
                "Some assets could not load; placeholders retained. {}",
                self.asset_errors.join("; ")
            )
        };
    }
    fn event(&mut self, ctx: &Context, state: &mut Editor, event: Event) -> Out<Editor, Event> {
        match event {
            Event::BlockAsset(kind, result) => {
                let result = result.and_then(|files| {
                    self.scene
                        .set_block_model(kind, &files, &InitContext::from(ctx))
                        .map_err(|e| format!("{e:#}"))
                });
                self.asset_finished(state, kind.label(), result);
            }
            Event::AtlasAsset(result) => {
                let result = result
                    .and_then(|bytes| self.ui.set_atlas(ctx, &bytes).map_err(|e| format!("{e:#}")));
                self.asset_finished(state, "Icons", result);
            }
            Event::Idle => {
                state.busy = false;
                state.status = "File selection canceled".into();
            }
            Event::Error(error) => {
                state.busy = false;
                state.status = error;
            }
            Event::Saved { revision, project } => {
                state.busy = false;
                if project && revision == state.revision {
                    state.history.mark_saved();
                }
                state.status = if project {
                    "Project bundle saved"
                } else {
                    "Schematic binary saved (guides are only in the project ZIP)"
                }
                .into();
            }
            Event::Files(intent, files) => {
                state.busy = false;
                let result = (|| -> anyhow::Result<()> {
                    if intent == FileIntent::Open {
                        anyhow::ensure!(
                            files.len() == 1,
                            "Select one project ZIP or schematic binary"
                        );
                        let (name, data) = files.iter().next().unwrap();
                        let (document, settings) = project::open(name, data)?;
                        let cache = blueprints::load_document(&document, &InitContext::from(ctx))?;
                        self.pending_open = Some((document, settings, cache));
                        if state.dirty() {
                            self.ui.confirm_open = true;
                            self.ui.needs_rebuild();
                            state.escape();
                        } else {
                            self.accept_open(state);
                        }
                    } else {
                        let (guides, cache) =
                            blueprints::import(files, state.next_id, &InitContext::from(ctx))?;
                        state.next_id += guides.len() as u64;
                        state.active_guide = guides.first().map(|g| g.id);
                        state.status = format!("Imported {} blueprint guides", guides.len());
                        self.scene.guides.extend(cache);
                        state.edit(Edit::guides(vec![], guides));
                        self.ui.guide_tab = true;
                        self.ui.needs_rebuild();
                        state.escape();
                    }
                    Ok(())
                })();
                if let Err(e) = result {
                    state.status = format!("{e:#}");
                }
            }
            Event::Action(action) => {
                match action {
                    Action::Paint(kind) => state.paint(kind),
                    Action::Mode(mode) => state.set_mode(mode),
                    Action::Grid(value) => {
                        state.set_grid(value);
                        self.ui.needs_rebuild();
                    }
                    Action::Height(value) => {
                        state.settings.manual_height = value;
                        self.ui.needs_rebuild();
                    }
                    Action::Axis(axis) => state.axis = axis,
                    Action::Move => state.begin_transform(false, Input::floor(ctx)),
                    Action::Rotate => state.begin_transform(true, Input::floor(ctx)),
                    Action::Copy => state.copy(),
                    Action::Paste => state.paste(),
                    Action::Undo => state.undo(false),
                    Action::Redo => state.undo(true),
                    Action::DeleteSelection => {
                        state.delete_ids(&state.selected.iter().copied().collect::<Vec<_>>())
                    }
                    Action::DismissStatus => state.status.clear(),
                    Action::Escape => {
                        state.escape();
                        self.ui.confirm_open = false;
                        self.pending_open = None;
                        self.ui.needs_rebuild();
                    }
                    Action::Tab(guides) => {
                        self.ui.guide_tab = guides;
                        self.ui.needs_rebuild();
                    }
                    Action::Guide(id) => {
                        state.active_guide = Some(id);
                        self.ui.needs_rebuild();
                    }
                    Action::GuidePage(delta) => {
                        self.ui.page = self.ui.page.saturating_add_signed(delta as isize);
                        self.ui.needs_rebuild();
                    }
                    Action::ApplyGuide => {
                        if let Err(e) = self.ui.apply_guide(state) {
                            state.status = e.to_string();
                        }
                    }
                    Action::ToggleGuide | Action::RemoveGuide => {
                        if let Some(before) = state
                            .document
                            .blueprints
                            .iter()
                            .find(|g| Some(g.id) == state.active_guide)
                            .cloned()
                        {
                            let after = if matches!(action, Action::RemoveGuide) {
                                vec![]
                            } else {
                                let mut g = before.clone();
                                g.visible = !g.visible;
                                vec![g]
                            };
                            state.edit(Edit::guides(vec![before], after));
                            self.ui.needs_rebuild();
                        }
                    }
                    Action::Files(intent) => {
                        if state.busy {
                            return Out::Empty;
                        }
                        #[cfg(not(target_arch = "wasm32"))]
                        {
                            state.busy = true;
                            state.status = "Choose files or a folder to load…".into();
                            return Out::FutEvent(vec![Box::new(files::pick(intent))]);
                        }
                        #[cfg(target_arch = "wasm32")]
                        {
                            let _ = intent;
                            state.status = "Use the file controls above the canvas".into();
                        }
                    }
                    Action::Export(project) => {
                        if state.busy {
                            return Out::Empty;
                        }
                        let data = if project {
                            project::encode(&state.document, state.settings)
                        } else {
                            codec::encode(&state.document.schematic())
                        };
                        match data {
                            Ok(data) => {
                                state.busy = true;
                                return Out::FutEvent(vec![Box::new(files::save(
                                    if project {
                                        "schematic-project.zip"
                                    } else {
                                        "schematic.bin"
                                    },
                                    data,
                                    state.revision,
                                    project,
                                ))]);
                            }
                            Err(e) => state.status = format!("Export failed: {e:#}"),
                        }
                    }
                    Action::ConfirmOpen => self.accept_open(state),
                    Action::CancelOpen => {
                        self.pending_open = None;
                        self.ui.confirm_open = false;
                        self.ui.needs_rebuild();
                    }
                }
                if state.preview.is_some() {
                    state.update_position(if Input::in_scene(ctx) {
                        Input::floor(ctx)
                    } else {
                        None
                    });
                }
            }
        }
        Out::Empty
    }
    fn accept_open(&mut self, state: &mut Editor) {
        if let Some((document, settings, cache)) = self.pending_open.take() {
            state.replace(document, settings);
            self.scene.guides = cache;
            state.status = "Project opened".into();
        }
        self.ui.confirm_open = false;
        self.ui.needs_rebuild();
        self.input.reset();
    }
}
impl GraphicsFlow<Editor, Event> for EditorFlow {
    fn on_init(&mut self, ctx: &mut Context, state: &mut Editor) -> Out<Editor, Event> {
        ctx.automatic_camera_controls = false;
        ctx.camera.camera = Input::initial_camera();
        // Broad fill makes shaded faces readable without losing the normal-map key light.
        ctx.light.uniform.ambient_strength = 0.45;
        ctx.queue
            .write_buffer(&ctx.light.buffer, 0, bytemuck::bytes_of(&ctx.light.uniform));
        ctx.clear_colour = wgpu::Color {
            r: 0.075,
            g: 0.063,
            b: 0.049,
            a: 1.,
        };
        #[cfg(not(target_arch = "wasm32"))]
        {
            let _ = ctx
                .window
                .request_inner_size(winit::dpi::LogicalSize::new(1280., 820.));
        }
        ctx.window.set_title("Flow schematic editor");
        self.scene.init(ctx);
        state.status = "Loading block models and icons…".into();
        self.ui.init(ctx, state);
        let client = reqwest::Client::new();
        let mut downloads = Vec::new();
        for kind in BlockType::ALL {
            if let Some(path) = kind.model_path() {
                let client = client.clone();
                downloads.push(Out::FutEvent(vec![Box::new(async move {
                    Event::BlockAsset(
                        kind,
                        assets::model_files(&client, path)
                            .await
                            .map_err(|e| format!("{e:#}")),
                    )
                })]));
            }
        }
        downloads.push(Out::FutEvent(vec![Box::new(async move {
            Event::AtlasAsset(
                assets::download(&client, assets::ATLAS_PATH)
                    .await
                    .map_err(|e| format!("{e:#}")),
            )
        })]));
        self.assets_remaining = downloads.len();
        Out::Composed(downloads)
    }
    fn on_update(
        &mut self,
        ctx: &Context,
        state: &mut Editor,
        dt: std::time::Duration,
    ) -> Out<Editor, Event> {
        #[cfg(target_arch = "wasm32")]
        while let Some(event) = files::next() {
            self.events.push_back(event);
        }
        let mut outs = Vec::new();
        while let Some(event) = self.events.pop_front() {
            outs.push(self.event(ctx, state, event));
        }
        outs.push(self.ui.update(ctx, state, dt));
        outs.push(
            self.input
                .update(ctx, dt, self.ui.focused() || self.ui.confirm_open),
        );
        if state.preview.is_some() && Input::in_scene(ctx) {
            state.update_position(Input::floor(ctx));
        }
        self.scene.update(ctx, state, self.input.rectangle);
        if let Some(error) = self.scene.error.take() {
            state.status = error;
        }
        Out::Composed(outs)
    }
    fn on_window_events(
        &mut self,
        ctx: &Context,
        state: &mut Editor,
        event: &WindowEvent,
    ) -> Out<Editor, Event> {
        let focused = self.ui.focused() || self.ui.confirm_open;
        if let Some(action) = self.input.window(ctx, state, event, focused) {
            self.events.push_back(Event::Action(action));
        }
        self.ui.window(ctx, state, event)
    }
    fn on_custom_events(&mut self, _: &Context, _: &mut Editor, event: Event) -> Option<Event> {
        self.events.push_back(event);
        None
    }
    fn on_render<'pass>(&self) -> Render<'_, 'pass> {
        Render::Composed(vec![self.scene.render(), self.ui.render()])
    }
}
pub fn launch() {
    if let Err(error) = try_launch() {
        eprintln!("Editor failed to start: {error:#}");
    }
}

fn try_launch() -> anyhow::Result<()> {
    let editor: FlowConstructor<Editor, Event> = Box::new(|gpu| {
        Box::pin(async move { Box::new(EditorFlow::new(&gpu)) as Box<dyn GraphicsFlow<_, _>> })
    });
    flow_ngin::flow::run(vec![editor])
}
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen::prelude::wasm_bindgen]
pub fn run_web(_token: String) -> Result<(), wasm_bindgen::JsValue> {
    // Reserved for future authenticated services; no token is consumed or sent yet.
    console_error_panic_hook::set_once();
    try_launch().map_err(|error| {
        wasm_bindgen::JsValue::from_str(&format!("Editor failed to start: {error:#}"))
    })
}
#[cfg(target_arch = "wasm32")]
pub use files::{file_action, receive_files};
