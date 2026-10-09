use crate::document::{InstanceData, SchematicData};
use anyhow::{Result, ensure};
use std::{
    collections::HashMap,
    io::{Cursor, Read},
};
pub fn encode(data: &SchematicData) -> Result<Vec<u8>> {
    ensure!(data.version == 1, "Unsupported schematic version");
    let mut bytes = Vec::new();
    bytes.extend(1u32.to_le_bytes());
    bytes.extend(u32::try_from(data.blocks.len())?.to_le_bytes());
    let mut entries: Vec<_> = data.blocks.iter().collect();
    entries.sort_by_key(|(name, _)| *name);
    for (name, instances) in entries {
        bytes.extend(u16::try_from(name.len())?.to_le_bytes());
        bytes.extend(name.as_bytes());
        bytes.extend(u32::try_from(instances.len())?.to_le_bytes());
        for instance in instances {
            ensure!(instance.valid(), "Invalid transform");
            for v in instance
                .position
                .iter()
                .chain(&instance.rotation)
                .chain(&instance.scale)
            {
                bytes.extend(v.to_le_bytes());
            }
        }
    }
    Ok(bytes)
}
fn read<const N: usize>(reader: &mut Cursor<&[u8]>) -> Result<[u8; N]> {
    let mut data = [0; N];
    reader.read_exact(&mut data)?;
    Ok(data)
}
pub fn decode(bytes: &[u8]) -> Result<SchematicData> {
    let mut r = Cursor::new(bytes);
    let version = u32::from_le_bytes(read(&mut r)?);
    ensure!(version == 1, "Unsupported schematic version {version}");
    let count = u32::from_le_bytes(read(&mut r)?) as usize;
    ensure!(count <= bytes.len() / 6, "Invalid block count");
    let mut blocks = HashMap::new();
    for _ in 0..count {
        let len = u16::from_le_bytes(read(&mut r)?) as usize;
        let mut name = vec![0; len];
        r.read_exact(&mut name)?;
        let name = String::from_utf8(name)?;
        let count = u32::from_le_bytes(read(&mut r)?) as usize;
        ensure!(
            count <= (bytes.len() - r.position() as usize) / 40,
            "Truncated instance data"
        );
        let mut instances = Vec::with_capacity(count);
        for _ in 0..count {
            let mut values = [0.; 10];
            for v in &mut values {
                *v = f32::from_le_bytes(read(&mut r)?);
            }
            let transform = InstanceData {
                position: values[..3].try_into()?,
                rotation: values[3..7].try_into()?,
                scale: values[7..].try_into()?,
            };
            ensure!(transform.valid(), "Invalid transform");
            instances.push(transform);
        }
        ensure!(
            blocks.insert(name, instances).is_none(),
            "Duplicate block type"
        );
    }
    ensure!(
        r.position() as usize == bytes.len(),
        "Unexpected trailing bytes"
    );
    Ok(SchematicData { version, blocks })
}
