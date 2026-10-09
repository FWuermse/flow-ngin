use cgmath::{InnerSpace, Quaternion, Rotation, Vector3};
use flow_ngin::{data_structures::instance::Instance, resources::AssetFiles};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeSet, HashMap},
    sync::Arc,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BlockType {
    Stone,
    Log,
    Wood,
    Clay,
    Wheat,
    Copper,
    Iron,
    Gold,
}
impl BlockType {
    pub const ALL: [Self; 8] = [
        Self::Stone,
        Self::Log,
        Self::Wood,
        Self::Clay,
        Self::Wheat,
        Self::Copper,
        Self::Iron,
        Self::Gold,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::Stone => "Stone",
            Self::Log => "Log",
            Self::Wood => "Wood",
            Self::Clay => "Clay",
            Self::Wheat => "Wheat",
            Self::Copper => "Copper",
            Self::Iron => "Iron",
            Self::Gold => "Gold",
        }
    }
    pub fn id(self) -> String {
        self.label().to_lowercase()
    }
    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|k| k.id() == s)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct InstanceData {
    pub position: [f32; 3],
    /// Quaternion in x, y, z, w order.
    pub rotation: [f32; 4],
    pub scale: [f32; 3],
}
impl Default for InstanceData {
    fn default() -> Self {
        Self {
            position: [0.; 3],
            rotation: [0., 0., 0., 1.],
            scale: [1.; 3],
        }
    }
}
impl InstanceData {
    pub fn rotation(self) -> Quaternion<f32> {
        Quaternion::new(
            self.rotation[3],
            self.rotation[0],
            self.rotation[1],
            self.rotation[2],
        )
    }
    pub fn set_rotation(&mut self, q: Quaternion<f32>) {
        let q = q.normalize();
        self.rotation = [q.v.x, q.v.y, q.v.z, q.s];
    }
    pub fn instance(self) -> Instance {
        Instance {
            position: self.position.into(),
            rotation: self.rotation(),
            scale: self.scale.into(),
        }
    }
    pub fn valid(&self) -> bool {
        self.position
            .iter()
            .chain(&self.rotation)
            .chain(&self.scale)
            .all(|v| v.is_finite())
            && self.scale.iter().all(|&v| v > 0.)
            && (self.rotation().magnitude2() - 1.).abs() < 0.001
    }
    /// Cube asset bounds are (-.5,0,-.5)..(.5,1,.5).
    pub fn center(self) -> Vector3<f32> {
        Vector3::from(self.position)
            + self
                .rotation()
                .rotate_vector(Vector3::new(0., self.scale[1] * 0.5, 0.))
    }
    pub fn set_center(&mut self, center: Vector3<f32>) {
        self.position = (center
            - self
                .rotation()
                .rotate_vector(Vector3::new(0., self.scale[1] * 0.5, 0.)))
        .into();
    }
    pub fn translate(&mut self, delta: Vector3<f32>) {
        self.position = (Vector3::from(self.position) + delta).into();
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SchematicData {
    pub version: u32,
    pub blocks: HashMap<String, Vec<InstanceData>>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Block {
    pub id: u64,
    pub kind: BlockType,
    pub transform: InstanceData,
}
#[derive(Clone, Debug)]
pub struct Blueprint {
    pub id: u64,
    pub name: String,
    pub entry: String,
    pub files: Arc<AssetFiles>,
    pub transform: InstanceData,
    pub opacity: f32,
    pub visible: bool,
}
#[derive(Clone, Default, Debug)]
pub struct Document {
    pub blocks: Vec<Block>,
    pub blueprints: Vec<Blueprint>,
}
impl Document {
    pub fn schematic(&self) -> SchematicData {
        let mut blocks = HashMap::<String, Vec<InstanceData>>::new();
        let mut placed: Vec<_> = self.blocks.iter().collect();
        placed.sort_by_key(|b| b.id);
        for block in placed {
            blocks
                .entry(block.kind.id())
                .or_default()
                .push(block.transform);
        }
        SchematicData { version: 1, blocks }
    }
    pub fn from_schematic(data: SchematicData) -> anyhow::Result<Self> {
        anyhow::ensure!(data.version == 1, "Unsupported schematic version");
        let mut entries: Vec<_> = data.blocks.into_iter().collect();
        entries.sort_by(|a, b| a.0.cmp(&b.0));
        let mut blocks = Vec::new();
        for (name, instances) in entries {
            let kind = BlockType::parse(&name)
                .ok_or_else(|| anyhow::anyhow!("Unknown block type: {name}"))?;
            for transform in instances {
                anyhow::ensure!(transform.valid(), "Invalid {name} transform");
                blocks.push(Block {
                    id: blocks.len() as u64 + 1,
                    kind,
                    transform,
                });
            }
        }
        Ok(Self {
            blocks,
            blueprints: Vec::new(),
        })
    }
    pub fn next_id(&self) -> u64 {
        self.blocks
            .iter()
            .map(|b| b.id)
            .chain(self.blueprints.iter().map(|b| b.id))
            .max()
            .unwrap_or(0)
            + 1
    }
}

/// A transaction stores only changed objects. Blueprint file bytes remain shared.
#[derive(Clone, Default)]
pub struct Edit {
    pub before_blocks: Vec<Block>,
    pub after_blocks: Vec<Block>,
    pub before_guides: Vec<Blueprint>,
    pub after_guides: Vec<Blueprint>,
}
impl Edit {
    pub fn blocks(before_blocks: Vec<Block>, after_blocks: Vec<Block>) -> Self {
        Self {
            before_blocks,
            after_blocks,
            ..Self::default()
        }
    }
    pub fn guides(before_guides: Vec<Blueprint>, after_guides: Vec<Blueprint>) -> Self {
        Self {
            before_guides,
            after_guides,
            ..Self::default()
        }
    }
    fn apply(&self, document: &mut Document, forward: bool) {
        let block_ids: BTreeSet<_> = self
            .before_blocks
            .iter()
            .chain(&self.after_blocks)
            .map(|b| b.id)
            .collect();
        document.blocks.retain(|b| !block_ids.contains(&b.id));
        document.blocks.extend(
            if forward {
                &self.after_blocks
            } else {
                &self.before_blocks
            }
            .iter()
            .cloned(),
        );
        document.blocks.sort_by_key(|b| b.id);
        let guide_ids: BTreeSet<_> = self
            .before_guides
            .iter()
            .chain(&self.after_guides)
            .map(|b| b.id)
            .collect();
        document.blueprints.retain(|b| !guide_ids.contains(&b.id));
        document.blueprints.extend(
            if forward {
                &self.after_guides
            } else {
                &self.before_guides
            }
            .iter()
            .cloned(),
        );
        document.blueprints.sort_by_key(|b| b.id);
    }
}
#[derive(Default)]
pub struct History {
    edits: Vec<Edit>,
    cursor: usize,
    saved: Option<usize>,
}
impl History {
    pub fn clean() -> Self {
        Self {
            saved: Some(0),
            ..Self::default()
        }
    }
    pub fn dirty(&self) -> bool {
        self.saved != Some(self.cursor)
    }
    pub fn mark_saved(&mut self) {
        self.saved = Some(self.cursor);
    }
    pub fn commit(&mut self, doc: &mut Document, edit: Edit) {
        if self.saved.is_some_and(|i| i > self.cursor) {
            self.saved = None;
        }
        self.edits.truncate(self.cursor);
        edit.apply(doc, true);
        self.edits.push(edit);
        self.cursor += 1;
    }
    pub fn undo(&mut self, doc: &mut Document) {
        if self.cursor > 0 {
            self.cursor -= 1;
            self.edits[self.cursor].apply(doc, false);
        }
    }
    pub fn redo(&mut self, doc: &mut Document) {
        if self.cursor < self.edits.len() {
            self.edits[self.cursor].apply(doc, true);
            self.cursor += 1;
        }
    }
}
