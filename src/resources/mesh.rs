use std::num::TryFromIntError;

use cgmath::{InnerSpace, Zero};
use wgpu::util::DeviceExt;

use crate::data_structures::model;

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
                    tangent: [0.0; 3],
                    bitangent: [0.0; 3],
                })
                .collect::<Vec<_>>();

            let indices = &m.mesh.indices;
            compute_tangents(&mut vertices, indices);

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

pub(crate) fn compute_tangents(vertices: &mut Vec<model::ModelVertex>, indices: &[u32]) {
    let mut tan1 = vec![cgmath::Vector3::zero(); vertices.len()];
    let mut tan2 = vec![cgmath::Vector3::zero(); vertices.len()];

    for c in indices.chunks(3) {
        if c.len() < 3 {
            break;
        }

        let i1 = c[0] as usize;
        let i2 = c[1] as usize;
        let i3 = c[2] as usize;

        let v1 = &vertices[i1];
        let v2 = &vertices[i2];
        let v3 = &vertices[i3];

        let p1: cgmath::Vector3<f32> = v1.position.into();
        let p2: cgmath::Vector3<f32> = v2.position.into();
        let p3: cgmath::Vector3<f32> = v3.position.into();

        let w1: cgmath::Vector2<f32> = v1.tex_coords.into();
        let w2: cgmath::Vector2<f32> = v2.tex_coords.into();
        let w3: cgmath::Vector2<f32> = v3.tex_coords.into();

        let x1 = p2.x - p1.x;
        let x2 = p3.x - p1.x;
        let y1 = p2.y - p1.y;
        let y2 = p3.y - p1.y;
        let z1 = p2.z - p1.z;
        let z2 = p3.z - p1.z;

        let s1 = w2.x - w1.x;
        let s2 = w3.x - w1.x;
        let t1 = w2.y - w1.y;
        let t2 = w3.y - w1.y;

        let r_denom = s1 * t2 - s2 * t1;
        let r = if r_denom.abs() < 1e-6 {
            0.0
        } else {
            1.0 / r_denom
        };

        let sdir = cgmath::Vector3::new(
            (t2 * x1 - t1 * x2) * r,
            (t2 * y1 - t1 * y2) * r,
            (t2 * z1 - t1 * z2) * r,
        );

        let tdir = cgmath::Vector3::new(
            (s1 * x2 - s2 * x1) * r,
            (s1 * y2 - s2 * y1) * r,
            (s1 * z2 - s2 * z1) * r,
        );

        tan1[i1] += sdir;
        tan1[i2] += sdir;
        tan1[i3] += sdir;

        tan2[i1] += tdir;
        tan2[i2] += tdir;
        tan2[i3] += tdir;
    }

    for (i, vert) in vertices.iter_mut().enumerate() {
        let n: cgmath::Vector3<f32> = vert.normal.into();
        let t = tan1[i];

        // Gram-Schmidt orthogonalize
        let tangent_xyz = (t - n * n.dot(t)).normalize();

        let w = if n.cross(t).dot(tan2[i]) < 0.0 {
            -1.0
        } else {
            1.0
        };

        if tangent_xyz.x.is_nan() {
            vert.tangent = [1.0, 0.0, 0.0];
            vert.bitangent = [0.0, 1.0, 0.0];
        } else {
            vert.tangent = tangent_xyz.into();
            let bitangent = n.cross(tangent_xyz) * w;
            vert.bitangent = bitangent.into();
        }
    }
}
