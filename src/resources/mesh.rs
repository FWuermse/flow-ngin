use core::f32;
use std::num::TryFromIntError;

use cgmath::num_traits::ToPrimitive;
use wgpu::util::DeviceExt;

use crate::data_structures::model;

/**
 * Obj files don't come with tangents and bitangents so they have to be calculated for
 * normal maps to work correctly.
 *
 * TODO: retire once file-types are supported that come with calculated tangents (bitangents are easy to get from tangents)
 */
pub fn load_meshes(
    models: &Vec<tobj::Model>,
    file_name: &str,
    device: &wgpu::Device,
) -> Vec<Result<model::Mesh, TryFromIntError>> {
    models
        .into_iter()
        .map(|m| {
            let mut vertices = (0..m.mesh.positions.len() / 3)
                .map(|i| model::ModelVertex {
                    position: [
                        m.mesh.positions[i * 3],
                        m.mesh.positions[i * 3 + 1],
                        m.mesh.positions[i * 3 + 2],
                    ],
                    tex_coords: [
                        m.mesh.texcoords.get(i * 2).map_or(0.0, |f| *f),
                        1.0 - m.mesh.texcoords.get(i * 2 + 1).map_or(0.0, |f| *f),
                    ],
                    normal: [
                        m.mesh.normals.get(i * 3).map_or(0.0, |f| *f),
                        m.mesh.normals.get(i * 3 + 1).map_or(0.0, |f| *f),
                        m.mesh.normals.get(i * 3 + 2).map_or(0.0, |f| *f),
                    ],
                    // We'll calculate these later
                    tangent: [0.0; 3],
                    bitangent: [0.0; 3],
                })
                .collect::<Vec<_>>();

            let indices = &m.mesh.indices;
            let mut triangles_included = vec![0; vertices.len()];

            // Calculate tangents and bitangets. We're going to
            // use the triangles, so we need to loop through the
            // indices in chunks of 3
            for c in indices.chunks(3) {
                let v0 = vertices[usize::try_from(c[0])?];
                let v1 = vertices[usize::try_from(c[1])?];
                let v2 = vertices[usize::try_from(c[2])?];

                let pos0: cgmath::Vector3<_> = v0.position.into();
                let pos1: cgmath::Vector3<_> = v1.position.into();
                let pos2: cgmath::Vector3<_> = v2.position.into();

                let uv0: cgmath::Vector2<_> = v0.tex_coords.into();
                let uv1: cgmath::Vector2<_> = v1.tex_coords.into();
                let uv2: cgmath::Vector2<_> = v2.tex_coords.into();

                // Calculate the edges of the triangle
                let delta_pos1 = pos1 - pos0;
                let delta_pos2 = pos2 - pos0;

                // This will give us a direction to calculate the
                // tangent and bitangent
                let delta_uv1 = uv1 - uv0;
                let delta_uv2 = uv2 - uv0;

                // Solving the following system of equations will
                // give us the tangent and bitangent.
                //     delta_pos1 = delta_uv1.x * T + delta_u.y * B
                //     delta_pos2 = delta_uv2.x * T + delta_uv2.y * B
                let r = 1.0 / (delta_uv1.x * delta_uv2.y - delta_uv1.y * delta_uv2.x);
                let tangent = (delta_pos1 * delta_uv2.y - delta_pos2 * delta_uv1.y) * r;
                // We flip the bitangent to enable right-handed normal
                // maps with wgpu texture coordinate system
                let bitangent = (delta_pos2 * delta_uv1.x - delta_pos1 * delta_uv2.x) * -r;

                // We'll use the same tangent/bitangent for each vertex in the triangle
                vertices[usize::try_from(c[0])?].tangent =
                    (tangent + cgmath::Vector3::from(vertices[usize::try_from(c[0])?].tangent)).into();
                vertices[usize::try_from(c[1])?].tangent =
                    (tangent + cgmath::Vector3::from(vertices[usize::try_from(c[1])?].tangent)).into();
                vertices[usize::try_from(c[2])?].tangent =
                    (tangent + cgmath::Vector3::from(vertices[usize::try_from(c[2])?].tangent)).into();
                vertices[usize::try_from(c[0])?].bitangent =
                    (bitangent + cgmath::Vector3::from(vertices[usize::try_from(c[0])?].bitangent)).into();
                vertices[usize::try_from(c[1])?].bitangent =
                    (bitangent + cgmath::Vector3::from(vertices[usize::try_from(c[1])?].bitangent)).into();
                vertices[usize::try_from(c[2])?].bitangent =
                    (bitangent + cgmath::Vector3::from(vertices[usize::try_from(c[2])?].bitangent)).into();

                // Used to average the tangents/bitangents
                triangles_included[usize::try_from(c[0])?] += 1;
                triangles_included[usize::try_from(c[1])?] += 1;
                triangles_included[usize::try_from(c[2])?] += 1;
            }

            // Average the tangents/bitangents
            for (i, n) in triangles_included.into_iter().enumerate() {
                let denom = 1.0 / n.to_f32().unwrap_or(f32::MAX);
                let v = &mut vertices[i];
                v.tangent = (cgmath::Vector3::from(v.tangent) * denom).into();
                v.bitangent = (cgmath::Vector3::from(v.bitangent) * denom).into();
            }

            let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some(&format!("{:?} Vertex Buffer", file_name)),
                contents: bytemuck::cast_slice(&vertices),
                usage: wgpu::BufferUsages::VERTEX,
            });

            let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some(&format!("{:?} Index Buffer", file_name)),
                // The indices are for positions, texels, and normals because wet set `single_index` to true
                contents: bytemuck::cast_slice(&m.mesh.indices),
                usage: wgpu::BufferUsages::INDEX,
            });

            Ok(model::Mesh {
                name: file_name.to_string(),
                vertex_buffer,
                index_buffer,
                num_elements: u32::try_from(m.mesh.indices.len())?,
                material: m.mesh.material_id.unwrap_or(0),
            })
        })
        .collect::<Vec<_>>()
}
