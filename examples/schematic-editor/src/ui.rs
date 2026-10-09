#[cfg(not(target_arch = "wasm32"))]
use crate::files::FileIntent;
use crate::{
    Action, Event,
    document::{BlockType, Edit},
    editor::{Editor, Mode, Operation},
};
use cgmath::{Deg, Euler, Quaternion, Rad};
use flow_ngin::{
    WindowEvent,
    context::Context,
    flow::{GraphicsFlow, Out},
    render::Render,
    ui::{
        Button, Checkbox, Container, Layout, TextInput, UIElement, Value,
        image::{Atlas, Icon},
        text_label::TextLabel,
    },
};
pub const PANEL: f64 = 324.;
struct Item {
    widget: Box<dyn UIElement<Editor, Event>>,
    rect: [u32; 4],
}
struct Field {
    input: TextInput<Editor, Event>,
    value: Value<String>,
    rect: [u32; 4],
}
pub struct Ui {
    atlas: Option<std::sync::Arc<Atlas>>,
    items: Vec<Item>,
    fields: Vec<Field>,
    background: Container<Editor, Event>,
    status: TextLabel,
    pub guide_tab: bool,
    pub page: usize,
    rebuild: bool,
    cached_guide: Option<u64>,
    cached_revision: u64,
    cached_tools: Option<(Mode, Option<Operation>, usize)>,
    grid: Value<bool>,
    height: Value<bool>,
    pub confirm_open: bool,
    last_status: String,
}
impl Ui {
    pub fn new() -> Self {
        Self {
            atlas: None,
            items: vec![],
            fields: vec![],
            background: Container::new().with_background_color([43, 35, 28, 255]),
            status: TextLabel::new("")
                .font_size(15.)
                .line_height(20.)
                .color([204, 185, 154]),
            guide_tab: false,
            page: 0,
            rebuild: true,
            cached_guide: None,
            cached_revision: u64::MAX,
            cached_tools: None,
            grid: Value::new(true),
            height: Value::new(false),
            confirm_open: false,
            last_status: String::new(),
        }
    }
    pub fn init(&mut self, ctx: &mut Context, editor: &mut Editor) {
        self.background.on_init(ctx, editor);
        self.status.init(ctx);
        self.build(ctx, editor);
    }
    pub fn set_atlas(&mut self, ctx: &Context, bytes: &[u8]) -> anyhow::Result<()> {
        self.atlas = Some(std::sync::Arc::new(Atlas::from_bytes(
            &ctx.device,
            &ctx.queue,
            bytes,
            8,
            8,
        )?));
        self.needs_rebuild();
        Ok(())
    }
    pub fn focused(&self) -> bool {
        self.fields.iter().any(|f| f.input.is_focused())
    }
    pub fn needs_rebuild(&mut self) {
        self.rebuild = true;
    }
    fn widget(
        &mut self,
        ctx: &Context,
        _editor: &mut Editor,
        mut widget: impl UIElement<Editor, Event> + 'static,
        rect: [u32; 4],
    ) {
        Layout::resolve(&mut widget, rect[0], rect[1], rect[2], rect[3], &ctx.queue);
        self.items.push(Item {
            widget: Box::new(widget),
            rect,
        });
    }
    fn label(&mut self, ctx: &Context, editor: &mut Editor, text: &str, rect: [u32; 4]) {
        let mut label = TextLabel::new(text)
            .font_size(16.)
            .line_height(20.)
            .color([224, 207, 177]);
        label.init(ctx);
        self.widget(ctx, editor, label, rect);
    }
    fn button(
        &mut self,
        ctx: &Context,
        editor: &mut Editor,
        label: &str,
        action: Action,
        rect: [u32; 4],
    ) {
        let active = action_active(&action, editor, self.guide_tab);
        let mut button = Button::new()
            .fill(Icon::from_color(
                ctx,
                if active {
                    [125, 87, 38, 255]
                } else {
                    [65, 51, 38, 255]
                },
            ))
            .hover_fill(Icon::from_color(
                ctx,
                if active {
                    [153, 110, 49, 255]
                } else {
                    [105, 76, 47, 255]
                },
            ))
            .click_fill(Icon::from_color(ctx, [112, 76, 32, 255]))
            .with_text(
                TextLabel::new(label)
                    .font_size(16.)
                    .line_height(20.)
                    .color([245, 229, 198]),
            )
            .on_click(move |_, _| Event::Action(action.clone()));
        button.init(ctx);
        self.widget(ctx, editor, button, rect);
    }
    fn field(&mut self, ctx: &Context, _editor: &mut Editor, value: f32, rect: [u32; 4]) {
        let value = Value::new(format!("{value:.3}"));
        let mut input = TextInput::new()
            .height(30)
            .text_color([245, 229, 198])
            .background(Icon::from_color(ctx, [28, 23, 18, 255]))
            .font_size(16.)
            .bind(&value)
            .on_submit(|_| {
                Out::FutEvent(vec![Box::new(async { Event::Action(Action::ApplyGuide) })])
            });
        input.init(ctx);
        Layout::resolve(&mut input, rect[0], rect[1], rect[2], rect[3], &ctx.queue);
        self.fields.push(Field { input, value, rect });
    }
    fn build(&mut self, ctx: &Context, editor: &mut Editor) {
        self.items.clear();
        self.fields.clear();
        self.grid.set(editor.settings.grid);
        self.height.set(editor.settings.manual_height);
        self.cached_revision = editor.revision;
        self.cached_tools = Some(tool_state(editor));
        self.cached_guide = editor.active_guide;
        self.rebuild = false;
        self.label(ctx, editor, "FLOW / SCHEMATIC EDITOR", [12, 10, 300, 28]);
        self.button(ctx, editor, "Build", Action::Tab(false), [12, 44, 144, 32]);
        self.button(
            ctx,
            editor,
            "Blueprints",
            Action::Tab(true),
            [168, 44, 144, 32],
        );
        if self.confirm_open {
            self.label(
                ctx,
                editor,
                "Discard unsaved work and open?",
                [12, 94, 300, 36],
            );
            self.button(
                ctx,
                editor,
                "Discard & open",
                Action::ConfirmOpen,
                [12, 140, 144, 34],
            );
            self.button(
                ctx,
                editor,
                "Cancel",
                Action::CancelOpen,
                [168, 140, 144, 34],
            );
        } else if !self.guide_tab {
            self.build_tools(ctx, editor);
        } else {
            self.guide_tools(ctx, editor);
        }
        self.resize(ctx);
    }
    fn checkbox(&self, ctx: &Context) -> Checkbox<Editor, Event> {
        if let Some(atlas) = &self.atlas {
            Checkbox::new()
                .unchecked(Icon::new(ctx, atlas, 0))
                .checked_overlay(Icon::new(ctx, atlas, 41))
        } else {
            Checkbox::new()
                .unchecked(Icon::from_color(ctx, [90, 77, 60, 255]))
                .checked(Icon::from_color(ctx, [195, 147, 64, 255]))
        }
    }
    fn build_tools(&mut self, ctx: &Context, editor: &mut Editor) {
        for (i, kind) in BlockType::ALL.into_iter().enumerate() {
            let x = 12 + (i % 4) as u32 * 78;
            let y = 90 + (i / 4) as u32 * 78;
            let selected = action_active(&Action::Paint(kind), editor, self.guide_tab);
            let mut button = Button::new().on_click(move |_, _| Event::Action(Action::Paint(kind)));
            if let Some(atlas) = &self.atlas {
                button = button
                    .fill(Icon::new(ctx, atlas, if selected { 1 } else { 0 }))
                    .hover_fill(Icon::new(ctx, atlas, if selected { 1 } else { 3 }))
                    .click_fill(Icon::new(ctx, atlas, 1))
                    .with_icon(Icon::new(ctx, atlas, kind.icon_slot()))
                    .content_scale(0.7);
            } else {
                button = button.fill(Icon::from_color(ctx, [65, 51, 38, 255]));
            }
            button.init(ctx);
            self.widget(ctx, editor, button, [x + 2, y + 2, 68, 68]);
        }
        for (i, (label, mode)) in [
            ("Select", Mode::Select),
            ("Multi", Mode::Multi),
            ("Delete", Mode::Delete),
        ]
        .into_iter()
        .enumerate()
        {
            self.button(
                ctx,
                editor,
                label,
                Action::Mode(mode),
                [12 + i as u32 * 102, 252, 96, 32],
            );
        }
        let mut grid = self.checkbox(ctx).bind(&self.grid).on_change(|v| {
            Out::FutEvent(vec![Box::new(
                async move { Event::Action(Action::Grid(v)) },
            )])
        });
        grid.init(ctx);
        self.widget(ctx, editor, grid, [12, 298, 22, 22]);
        self.label(
            ctx,
            editor,
            "Grid: show + snap (1 unit)",
            [42, 296, 270, 26],
        );
        let mut height = self.checkbox(ctx).bind(&self.height).on_change(|v| {
            Out::FutEvent(vec![Box::new(
                async move { Event::Action(Action::Height(v)) },
            )])
        });
        height.init(ctx);
        self.widget(ctx, editor, height, [12, 330, 22, 22]);
        self.label(ctx, editor, "Manual height: Up / Down", [42, 328, 270, 26]);
        for (i, (label, action)) in [
            ("Move (G)", Action::Move),
            ("Rotate (R)", Action::Rotate),
            ("Copy", Action::Copy),
            ("Paste", Action::Paste),
            ("Undo", Action::Undo),
            ("Redo", Action::Redo),
        ]
        .into_iter()
        .enumerate()
        {
            self.button(
                ctx,
                editor,
                label,
                action,
                [
                    12 + (i % 2) as u32 * 156,
                    370 + (i / 2) as u32 * 38,
                    144,
                    32,
                ],
            );
        }
        for (i, label) in ["Axis X", "Axis Y", "Axis Z"].into_iter().enumerate() {
            self.button(
                ctx,
                editor,
                label,
                Action::Axis(i),
                [12 + i as u32 * 102, 488, 96, 30],
            );
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.button(
                ctx,
                editor,
                "Import files",
                Action::Files(FileIntent::ImportFiles),
                [12, 532, 144, 30],
            );
            self.button(
                ctx,
                editor,
                "Import folder",
                Action::Files(FileIntent::ImportFolder),
                [168, 532, 144, 30],
            );
            self.button(
                ctx,
                editor,
                "Open",
                Action::Files(FileIntent::Open),
                [12, 570, 96, 30],
            );
            self.button(
                ctx,
                editor,
                "Save ZIP",
                Action::Export(true),
                [114, 570, 96, 30],
            );
            self.button(
                ctx,
                editor,
                "Save .bin",
                Action::Export(false),
                [216, 570, 96, 30],
            );
        }
        self.label(
            ctx,
            editor,
            "Click: place  |  Wheel: rotate",
            [12, 612, 300, 24],
        );
        self.label(
            ctx,
            editor,
            "Esc: cancel  |  Del: remove selected",
            [12, 638, 300, 24],
        );
    }
    fn guide_tools(&mut self, ctx: &Context, editor: &mut Editor) {
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.button(
                ctx,
                editor,
                "Import files",
                Action::Files(FileIntent::ImportFiles),
                [12, 90, 144, 30],
            );
            self.button(
                ctx,
                editor,
                "Import folder",
                Action::Files(FileIntent::ImportFolder),
                [168, 90, 144, 30],
            );
        }
        self.page = self
            .page
            .min(editor.document.blueprints.len().saturating_sub(1) / 4);
        let guides = editor
            .document
            .blueprints
            .iter()
            .skip(self.page * 4)
            .take(4)
            .map(|g| (g.id, g.name.clone()))
            .collect::<Vec<_>>();
        for (i, (id, name)) in guides.into_iter().enumerate() {
            let marker = if Some(id) == editor.active_guide {
                "> "
            } else {
                ""
            };
            self.button(
                ctx,
                editor,
                &format!("{marker}{name}"),
                Action::Guide(id),
                [12, 130 + i as u32 * 32, 300, 28],
            );
        }
        self.button(
            ctx,
            editor,
            "Previous",
            Action::GuidePage(-1),
            [12, 262, 144, 28],
        );
        self.button(
            ctx,
            editor,
            "Next",
            Action::GuidePage(1),
            [168, 262, 144, 28],
        );
        let Some(guide) = editor
            .document
            .blueprints
            .iter()
            .find(|g| Some(g.id) == editor.active_guide)
            .cloned()
        else {
            self.label(
                ctx,
                editor,
                "Import models to add building guides.",
                [12, 312, 300, 32],
            );
            return;
        };
        self.button(
            ctx,
            editor,
            if guide.visible {
                "Hide guide"
            } else {
                "Show guide"
            },
            Action::ToggleGuide,
            [12, 304, 144, 30],
        );
        self.button(
            ctx,
            editor,
            "Remove guide",
            Action::RemoveGuide,
            [168, 304, 144, 30],
        );
        self.label(ctx, editor, "Position: X / Y / Z", [12, 346, 300, 24]);
        for i in 0..3 {
            self.field(
                ctx,
                editor,
                guide.transform.position[i],
                [12 + i as u32 * 102, 374, 96, 30],
            );
        }
        self.label(
            ctx,
            editor,
            "Rotation: X / Y / Z (degrees)",
            [12, 414, 300, 24],
        );
        let e: Euler<Rad<f32>> = guide.transform.rotation().into();
        for (i, v) in [e.x.0.to_degrees(), e.y.0.to_degrees(), e.z.0.to_degrees()]
            .into_iter()
            .enumerate()
        {
            self.field(ctx, editor, v, [12 + i as u32 * 102, 442, 96, 30]);
        }
        self.label(
            ctx,
            editor,
            "Uniform scale       Opacity (0 - 1)",
            [12, 482, 300, 24],
        );
        self.field(ctx, editor, guide.transform.scale[0], [12, 510, 144, 30]);
        self.field(ctx, editor, guide.opacity, [168, 510, 144, 30]);
        self.button(
            ctx,
            editor,
            "Apply transform / opacity",
            Action::ApplyGuide,
            [12, 552, 300, 34],
        );
        self.label(
            ctx,
            editor,
            "Original files + Instance are saved.",
            [12, 602, 300, 26],
        );
    }
    pub fn apply_guide(&mut self, editor: &mut Editor) -> anyhow::Result<()> {
        if self.fields.len() != 8 {
            return Ok(());
        }
        let values = self
            .fields
            .iter()
            .map(|f| f.value.get().parse::<f32>())
            .collect::<Result<Vec<_>, _>>()?;
        anyhow::ensure!(
            values.iter().all(|v| v.is_finite())
                && values[6] > 0.
                && (0.0..=1.0).contains(&values[7]),
            "Use finite numbers, positive scale, and opacity 0 to 1"
        );
        let Some(before) = editor
            .document
            .blueprints
            .iter()
            .find(|g| Some(g.id) == editor.active_guide)
            .cloned()
        else {
            return Ok(());
        };
        let mut after = before.clone();
        after.transform.position = values[..3].try_into()?;
        after.transform.set_rotation(Quaternion::from(Euler {
            x: Deg(values[3]),
            y: Deg(values[4]),
            z: Deg(values[5]),
        }));
        after.transform.scale = [values[6]; 3];
        after.opacity = values[7];
        editor.edit(Edit::guides(vec![before], vec![after]));
        self.rebuild = true;
        Ok(())
    }
    pub fn resize(&mut self, ctx: &Context) {
        // Keep every control accessible in small windows by scaling the panel's
        // vertical layout; the canvas still uses physical pixel coordinates.
        let factor = (ctx.config.height.saturating_sub(90) as f32 / 670.)
            .min(1.)
            .max(0.25);
        Layout::resolve(
            &mut self.background,
            0,
            0,
            (PANEL as u32).min(ctx.config.width),
            ctx.config.height,
            &ctx.queue,
        );
        for item in &mut self.items {
            let [x, y, w, h] = item.rect;
            item.widget.resolve(
                x,
                (y as f32 * factor) as u32,
                w,
                (h as f32 * factor).max(12.) as u32,
                &ctx.queue,
            );
        }
        for field in &mut self.fields {
            let [x, y, w, h] = field.rect;
            field.input.resolve(
                x,
                (y as f32 * factor) as u32,
                w,
                (h as f32 * factor).max(12.) as u32,
                &ctx.queue,
            );
        }
        Layout::resolve(
            &mut self.status,
            12,
            ctx.config.height.saturating_sub(86),
            300,
            82,
            &ctx.queue,
        );
    }
    pub fn update(
        &mut self,
        ctx: &Context,
        editor: &mut Editor,
        dt: std::time::Duration,
    ) -> Out<Editor, Event> {
        if self.rebuild
            || (!self.guide_tab && self.cached_tools != Some(tool_state(editor)))
            || self.cached_guide != editor.active_guide
            || (self.guide_tab && self.cached_revision != editor.revision && !self.focused())
        {
            self.build(ctx, editor);
        }
        let mut outs = Vec::new();
        for item in &mut self.items {
            outs.push(item.widget.on_update(ctx, editor, dt));
        }
        for field in &mut self.fields {
            outs.push(field.input.on_update(ctx, editor, dt));
        }
        let mode = editor
            .preview
            .as_ref()
            .map(|p| format!("{:?}", p.operation))
            .unwrap_or_else(|| format!("{:?}", editor.mode));
        let status = wrap(&editor.status, 36);
        let status = format!(
            "{}\n{}{} | {} blocks | {} selected\nAxis {} | {}",
            status,
            if editor.dirty() { "* " } else { "" },
            mode,
            editor.document.blocks.len(),
            editor.selected.len(),
            ["X", "Y", "Z"][editor.axis],
            if editor.busy { "Working..." } else { "Ready" }
        );
        if status != self.last_status {
            self.status.set_text(&status);
            self.last_status = status;
        }
        Out::Composed(outs)
    }
    pub fn window(
        &mut self,
        ctx: &Context,
        editor: &mut Editor,
        event: &WindowEvent,
    ) -> Out<Editor, Event> {
        if matches!(event, WindowEvent::Resized(_)) {
            self.resize(ctx);
        }
        let mut outs = self
            .items
            .iter_mut()
            .map(|item| item.widget.on_window_events(ctx, editor, event))
            .collect::<Vec<_>>();
        outs.extend(
            self.fields
                .iter_mut()
                .map(|f| f.input.on_window_events(ctx, editor, event)),
        );
        Out::Composed(outs)
    }
    pub fn render<'a, 'p>(&'a self) -> Render<'a, 'p>
    where
        'p: 'a,
    {
        let mut renders = vec![self.background.on_render(), self.status.render()];
        renders.extend(self.items.iter().map(|item| item.widget.on_render()));
        renders.extend(self.fields.iter().map(|f| f.input.on_render()));
        Render::Composed(renders)
    }
}
fn wrap(text: &str, width: usize) -> String {
    let mut out = String::new();
    let mut column = 0;
    for word in text.split_whitespace() {
        if column + word.len() > width {
            out.push('\n');
            column = 0;
        }
        if column > 0 {
            out.push(' ');
            column += 1;
        }
        out.push_str(word);
        column += word.len();
        if out.lines().count() >= 3 {
            break;
        }
    }
    out
}

fn tool_state(editor: &Editor) -> (Mode, Option<Operation>, usize) {
    (
        editor.mode,
        editor.preview.as_ref().map(|p| p.operation),
        editor.axis,
    )
}

fn action_active(action: &Action, editor: &Editor, guide_tab: bool) -> bool {
    let (_, operation, _) = tool_state(editor);
    match action {
        Action::Tab(tab) => *tab == guide_tab,
        Action::Mode(mode) => operation.is_none() && *mode == editor.mode,
        Action::Paint(kind) => operation == Some(Operation::Paint(*kind)),
        Action::Move => operation == Some(Operation::Move),
        Action::Rotate => operation == Some(Operation::Rotate),
        Action::Paste => operation == Some(Operation::Paste),
        Action::Axis(axis) => *axis == editor.axis,
        Action::Guide(id) => editor.active_guide == Some(*id),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn active_controls_follow_modes_previews_and_escape() {
        let mut editor = Editor::default();
        assert!(action_active(&Action::Mode(Mode::Select), &editor, false));
        assert!(action_active(&Action::Axis(1), &editor, false));
        editor.set_mode(Mode::Multi);
        assert!(action_active(&Action::Mode(Mode::Multi), &editor, false));
        assert!(!action_active(&Action::Mode(Mode::Select), &editor, false));
        editor.paint(BlockType::Stone);
        assert!(action_active(
            &Action::Paint(BlockType::Stone),
            &editor,
            false
        ));
        assert!(!action_active(
            &Action::Paint(BlockType::Log),
            &editor,
            false
        ));
        assert!(!action_active(&Action::Mode(editor.mode), &editor, false));
        editor.axis = 0;
        assert!(action_active(&Action::Axis(0), &editor, false));
        assert!(!action_active(&Action::Axis(1), &editor, false));
        editor.escape();
        assert!(!action_active(
            &Action::Paint(BlockType::Stone),
            &editor,
            false
        ));
        assert!(action_active(&Action::Mode(Mode::Select), &editor, false));
        assert!(action_active(&Action::Tab(true), &editor, true));
        assert!(!action_active(&Action::Tab(false), &editor, true));
    }
}
