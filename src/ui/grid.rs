use instant::Duration;

use crate::{
    context::Context,
    flow::{GraphicsFlow, Out},
    render::Render,
    ui::{
        Placement,
        container::{Container, merge_outs},
        layout::{Layout, UIElement},
    },
};

/// A grid layout that divides its local space into up to 12×12 cells.
///
/// Each cell is backed by a [`Container`], so children placed into a cell
/// inherit the container's alignment / background capabilities.
///
/// # Example
///
/// ```no_run
/// use flow_ngin::ui::grid::Grid;
/// use flow_ngin::ui::image::Icon;
///
/// // 8 columns, 1 row – splits the screen into 8 vertical strips.
/// let grid = Grid::<State, Event>::new(8, 1)
///     .with_child(0, 0, some_icon)
///     .with_child(3, 0, other_icon);
/// ```
pub struct Grid<S, E> {
    placement: Placement,
    x: u32,
    y: u32,
    width: u32,
    height: u32,
    screen_width: u32,
    screen_height: u32,
    cols: u32,
    rows: u32,
    /// Flat vec of `cols * rows` cells, row-major order.
    cells: Vec<Container<S, E>>,
}

impl<S: 'static, E: 'static> Grid<S, E> {
    /// Create a grid with the given number of columns and rows (each clamped to 1..=12).
    pub fn new(cols: u32, rows: u32) -> Self {
        let cols = cols.clamp(1, 12);
        let rows = rows.clamp(1, 12);
        let mut cells = Vec::with_capacity((cols * rows) as usize);
        for _ in 0..(cols * rows) {
            cells.push(Container::new());
        }
        Self {
            placement: Placement::default(),
            x: 0,
            y: 0,
            width: 0,
            height: 0,
            screen_width: 0,
            screen_height: 0,
            cols,
            rows,
            cells,
        }
    }

    /// Place a child element into the cell at `(col, row)`.
    ///
    /// Panics if `col >= cols` or `row >= rows`.
    pub fn with_child(mut self, col: u32, row: u32, child: impl UIElement<S, E> + 'static) -> Self {
        assert!(col < self.cols, "col {col} out of range (max {})", self.cols - 1);
        assert!(row < self.rows, "row {row} out of range (max {})", self.rows - 1);
        let idx = (row * self.cols + col) as usize;
        let cell = self.cells.remove(idx).with_child(child);
        self.cells.insert(idx, cell);
        self
    }

    /// Place a child element into the cell at `(col, row)` — non-panicking version.
    ///
    /// Returns `self` unchanged if coordinates are out of range.
    pub fn try_with_child(
        mut self,
        col: u32,
        row: u32,
        child: impl UIElement<S, E> + 'static,
    ) -> Self {
        if col < self.cols && row < self.rows {
            let idx = (row * self.cols + col) as usize;
            let cell = self.cells.remove(idx).with_child(child);
            self.cells.insert(idx, cell);
        }
        self
    }

    /// Set a background colour on a specific cell.
    pub fn with_cell_background_color(mut self, col: u32, row: u32, rgba: [u8; 4]) -> Self {
        if col < self.cols && row < self.rows {
            let idx = (row * self.cols + col) as usize;
            let cell = self.cells.remove(idx).with_background_color(rgba);
            self.cells.insert(idx, cell);
        }
        self
    }

    pub fn halign(mut self, align: crate::ui::HAlign) -> Self {
        self.placement.halign = align;
        self
    }

    pub fn valign(mut self, align: crate::ui::VAlign) -> Self {
        self.placement.valign = align;
        self
    }

    pub fn width(mut self, w: u32) -> Self {
        self.placement.width = Some(w);
        self
    }

    pub fn height(mut self, h: u32) -> Self {
        self.placement.height = Some(h);
        self
    }

    /// Resolve cell positions from the grid's own bounds.
    fn resolve_cells(&mut self, queue: &wgpu::Queue) {
        let cell_w = self.width / self.cols;
        let cell_h = self.height / self.rows;
        for row in 0..self.rows {
            for col in 0..self.cols {
                let idx = (row * self.cols + col) as usize;
                let cx = self.x + col * cell_w;
                let cy = self.y + row * cell_h;
                Layout::resolve(&mut self.cells[idx], cx, cy, cell_w, cell_h, queue);
            }
        }
    }
}

impl<S: 'static, E: 'static> GraphicsFlow<S, E> for Grid<S, E> {
    fn on_init(&mut self, ctx: &mut Context, state: &mut S) -> Out<S, E> {
        self.screen_width = ctx.config.width;
        self.screen_height = ctx.config.height;

        let (x, y, w, h) = self.placement.resolve(0, 0, ctx.config.width, ctx.config.height);
        self.x = x;
        self.y = y;
        self.width = w;
        self.height = h;

        for cell in &mut self.cells {
            cell.on_init(ctx, state);
        }

        self.resolve_cells(&ctx.queue);
        Out::Empty
    }

    fn on_update(&mut self, ctx: &Context, state: &mut S, dt: Duration) -> Out<S, E> {
        merge_outs(self.cells.iter_mut().map(|c| c.on_update(ctx, state, dt)))
    }

    fn on_render<'pass>(&self) -> Render<'_, 'pass> {
        Render::Composed(self.cells.iter().map(|c| c.on_render()).collect())
    }
}

impl<S: 'static, E: 'static> Layout for Grid<S, E> {
    fn resolve(
        &mut self,
        parent_x: u32,
        parent_y: u32,
        parent_w: u32,
        parent_h: u32,
        queue: &wgpu::Queue,
    ) {
        let (x, y, w, h) = self.placement.resolve(parent_x, parent_y, parent_w, parent_h);
        self.x = x;
        self.y = y;
        self.width = w;
        self.height = h;
        self.resolve_cells(queue);
    }
}
