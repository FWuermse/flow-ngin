use crate::{
    document::{Block, BlockType, Document, Edit, History, InstanceData},
    geometry,
};
use cgmath::{Deg, Quaternion, Rotation, Rotation3, Vector3};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Settings {
    pub grid: bool,
    pub manual_height: bool,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            grid: true,
            manual_height: false,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Mode {
    Select,
    Multi,
    Delete,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Operation {
    Paint(BlockType),
    Move,
    Rotate,
    Paste,
}
pub struct Preview {
    pub operation: Operation,
    pub before: Vec<Block>,
    pub blocks: Vec<Block>,
    pub pivot: Vector3<f32>,
    pub valid: bool,
    pub has_ray: bool,
    pub cursor_anchor: Option<[f32; 2]>,
    pub movement_base: Vec<Block>,
}
impl Preview {
    pub fn excluded(&self) -> Vec<u64> {
        self.before.iter().map(|b| b.id).collect()
    }
    pub fn translate(&mut self, d: Vector3<f32>) {
        for b in &mut self.blocks {
            b.transform.translate(d);
        }
        self.pivot += d;
    }
}
pub struct Editor {
    pub document: Document,
    pub history: History,
    pub settings: Settings,
    pub mode: Mode,
    pub selected: BTreeSet<u64>,
    pub clipboard: Vec<Block>,
    pub preview: Option<Preview>,
    pub axis: usize,
    pub next_id: u64,
    pub revision: u64,
    pub scene_revision: u64,
    pub status: String,
    pub active_guide: Option<u64>,
    pub busy: bool,
}
impl Default for Editor {
    fn default() -> Self {
        Self {
            document: Document::default(),
            history: History::clean(),
            settings: Settings::default(),
            mode: Mode::Select,
            selected: BTreeSet::new(),
            clipboard: Vec::new(),
            preview: None,
            axis: 1,
            next_id: 1,
            revision: 0,
            scene_revision: 0,
            status: "Choose a block to build. Right-drag to look; WASD and Q/E to move.".into(),
            active_guide: None,
            busy: false,
        }
    }
}
impl Editor {
    pub fn dirty(&self) -> bool {
        self.history.dirty()
    }
    pub fn changed(&mut self) {
        self.revision += 1;
        self.scene_revision += 1;
    }
    pub fn edit(&mut self, edit: Edit) {
        self.history.commit(&mut self.document, edit);
        self.changed();
    }
    pub fn selected_blocks(&self) -> Vec<Block> {
        self.document
            .blocks
            .iter()
            .filter(|b| self.selected.contains(&b.id))
            .cloned()
            .collect()
    }
    pub fn escape(&mut self) {
        self.preview = None;
        self.selected.clear();
        self.mode = Mode::Select;
        self.scene_revision += 1;
    }
    pub fn set_mode(&mut self, mode: Mode) {
        self.preview = None;
        self.mode = mode;
        self.scene_revision += 1;
    }
    pub fn set_grid(&mut self, enabled: bool) {
        let reset = enabled && !self.settings.grid;
        self.settings.grid = enabled;
        if reset {
            if let Some(preview) = &mut self.preview {
                for block in &mut preview.blocks {
                    let center = block.transform.center();
                    block.transform.rotation = [0., 0., 0., 1.];
                    block.transform.set_center(center);
                }
                if let Some(bounds) = geometry::group_bounds(&preview.blocks) {
                    preview.pivot = bounds.center();
                    preview.translate(Vector3::new(
                        preview.pivot.x.round() - preview.pivot.x,
                        bounds.min.y.round() - bounds.min.y,
                        preview.pivot.z.round() - preview.pivot.z,
                    ));
                }
                // A move must not restore its pre-reset transforms on the next ray update.
                preview.movement_base = preview.blocks.clone();
                preview.cursor_anchor = None;
            }
            self.validate();
        }
        self.scene_revision += 1;
    }
    pub fn paint(&mut self, kind: BlockType) {
        self.escape();
        let block = Block {
            id: self.next_id,
            kind,
            transform: InstanceData::default(),
        };
        self.next_id += 1;
        self.preview = Some(Preview {
            operation: Operation::Paint(kind),
            before: vec![],
            blocks: vec![block.clone()],
            pivot: block.transform.center(),
            valid: false,
            has_ray: false,
            cursor_anchor: None,
            movement_base: vec![block],
        });
    }
    pub fn begin_transform(&mut self, rotate: bool, cursor: Option<[f32; 2]>) {
        self.preview = None;
        let blocks = self.selected_blocks();
        if blocks.is_empty() {
            return;
        }
        self.preview = Some(Preview {
            operation: if rotate {
                Operation::Rotate
            } else {
                Operation::Move
            },
            pivot: geometry::group_bounds(&blocks).unwrap().center(),
            before: blocks.clone(),
            movement_base: blocks.clone(),
            blocks,
            valid: true,
            has_ray: true,
            cursor_anchor: cursor,
        });
        self.scene_revision += 1;
    }
    pub fn copy(&mut self) {
        self.clipboard = self.selected_blocks();
        self.status = format!("Copied {} blocks", self.clipboard.len());
    }
    pub fn paste(&mut self) {
        if self.clipboard.is_empty() {
            return;
        }
        self.preview = None;
        let mut blocks = self.clipboard.clone();
        for b in &mut blocks {
            b.id = self.next_id;
            self.next_id += 1;
        }
        self.preview = Some(Preview {
            operation: Operation::Paste,
            pivot: geometry::group_bounds(&blocks).unwrap().center(),
            before: vec![],
            movement_base: blocks.clone(),
            blocks,
            valid: false,
            has_ray: false,
            cursor_anchor: None,
        });
        self.scene_revision += 1;
    }
    pub fn update_position(&mut self, cursor: Option<[f32; 2]>) {
        let Some(p) = &mut self.preview else {
            return;
        };
        if p.operation == Operation::Rotate {
            self.validate();
            return;
        }
        let Some([x, z]) = cursor else {
            p.has_ray = false;
            p.valid = false;
            self.scene_revision += 1;
            return;
        };
        p.has_ray = true;
        let target = if self.settings.grid {
            [x.round(), z.round()]
        } else {
            [x, z]
        };
        let center = geometry::group_bounds(&p.blocks).unwrap().center();
        let delta = match p.operation {
            Operation::Move => {
                let anchor = *p.cursor_anchor.get_or_insert([x, z]);
                let base = geometry::group_bounds(&p.movement_base).unwrap().center();
                let dx = x - anchor[0];
                let dz = z - anchor[1];
                Vector3::new(
                    base.x + if self.settings.grid { dx.round() } else { dx } - center.x,
                    0.,
                    base.z + if self.settings.grid { dz.round() } else { dz } - center.z,
                )
            }
            _ => Vector3::new(target[0] - center.x, 0., target[1] - center.z),
        };
        p.translate(delta);
        if !self.settings.manual_height {
            let minimum = geometry::group_bounds(&p.blocks).unwrap().min.y;
            p.translate(Vector3::new(0., -minimum, 0.));
            let excluded = p.excluded();
            let mut raise = 0f32;
            for b in &p.blocks {
                let bb = geometry::bounds(b.transform);
                for other in self
                    .document
                    .blocks
                    .iter()
                    .filter(|b| !excluded.contains(&b.id))
                {
                    let ob = geometry::bounds(other.transform);
                    if bb.footprint_overlaps(ob) {
                        raise = raise.max(ob.max.y - bb.min.y);
                    }
                }
            }
            if self.settings.grid {
                raise = (raise - geometry::EPS).ceil().max(0.);
            }
            p.translate(Vector3::new(0., raise, 0.));
        }
        self.validate();
        self.scene_revision += 1;
    }
    pub fn validate(&mut self) {
        if let Some(p) = &mut self.preview {
            p.valid = p.has_ray
                && geometry::valid_placement(&p.blocks, &self.document.blocks, &p.excluded());
        }
    }
    pub fn rotate(&mut self, steps: f32) {
        let Some(p) = &mut self.preview else {
            return;
        };
        let axis = [Vector3::unit_x(), Vector3::unit_y(), Vector3::unit_z()][self.axis];
        let q = Quaternion::from_axis_angle(
            axis,
            Deg(steps * if self.settings.grid { 90. } else { 15. }),
        );
        for b in &mut p.blocks {
            let center = p.pivot + q.rotate_vector(b.transform.center() - p.pivot);
            b.transform.set_rotation(q * b.transform.rotation());
            b.transform.set_center(center);
        }
        self.validate();
        self.scene_revision += 1;
    }
    pub fn height(&mut self, direction: f32) {
        if !self.settings.manual_height {
            return;
        }
        if let Some(p) = &mut self.preview {
            p.translate(Vector3::new(
                0.,
                direction * if self.settings.grid { 1. } else { 0.25 },
                0.,
            ));
        }
        self.validate();
        self.scene_revision += 1;
    }
    pub fn commit_preview(&mut self) -> bool {
        if !self.preview.as_ref().is_some_and(|p| p.valid) {
            self.status = "Cannot place here: overlap, below ground, or no ground ray.".into();
            return false;
        }
        let p = self.preview.take().unwrap();
        let operation = p.operation;
        let placed = p.blocks.clone();
        self.edit(Edit::blocks(p.before, p.blocks));
        if let Operation::Paint(kind) = operation {
            self.paint(kind);
            let next = self.preview.as_mut().unwrap();
            next.blocks[0].transform = placed[0].transform;
            next.pivot = placed[0].transform.center();
            next.has_ray = true;
        } else {
            self.selected = placed.iter().map(|b| b.id).collect();
        }
        true
    }
    pub fn select(&mut self, id: Option<u64>, add: bool) {
        if self.mode == Mode::Delete {
            if let Some(id) = id {
                self.delete_ids(&[id]);
            }
            return;
        }
        if self.mode == Mode::Multi || add {
            if let Some(id) = id {
                if !self.selected.insert(id) {
                    self.selected.remove(&id);
                }
            } else if !add {
                self.selected.clear();
            }
        } else {
            self.selected = id.into_iter().collect();
        }
        self.scene_revision += 1;
    }
    pub fn delete_ids(&mut self, ids: &[u64]) {
        self.preview = None;
        let before = self
            .document
            .blocks
            .iter()
            .filter(|b| ids.contains(&b.id))
            .cloned()
            .collect::<Vec<_>>();
        if !before.is_empty() {
            self.edit(Edit::blocks(before, vec![]));
        }
        self.selected.retain(|id| !ids.contains(id));
    }
    pub fn undo(&mut self, redo: bool) {
        self.escape();
        if redo {
            self.history.redo(&mut self.document);
        } else {
            self.history.undo(&mut self.document);
        }
        self.changed();
    }
    pub fn replace(&mut self, document: Document, settings: Settings) {
        self.escape();
        self.next_id = document.next_id();
        self.document = document;
        self.settings = settings;
        self.history = History::clean();
        self.active_guide = self.document.blueprints.first().map(|b| b.id);
        self.changed();
    }
}
