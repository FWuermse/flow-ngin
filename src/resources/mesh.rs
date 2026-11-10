use std::{collections::HashMap, num::TryFromIntError};

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

// TODO: make private once terrain generation is public
pub fn compute_tangents(vertices: &mut Vec<model::ModelVertex>, indices: &[u32]) {
    let mut tan1 = vec![cgmath::Vector3::zero(); vertices.len()];
    let mut tan2 = vec![cgmath::Vector3::zero(); vertices.len()];

    for c in indices.chunks_exact(3) {
        let (sdir, tdir) = triangle_dirs(vertices, c);
        for &i in c {
            tan1[i as usize] += sdir;
            tan2[i as usize] += tdir;
        }
    }

    for (i, vert) in vertices.iter_mut().enumerate() {
        finish_tangent(vert, tan1[i], tan2[i]);
    }
}

/// Recomputes tangents/bitangents only for the vertices in `targets`.
///
/// `indices` must contain **every** triangle that touches a target vertex (extra triangles are fine,
/// they're ignored for non-targets). Cost is O(`indices.len()` + `targets.len()`), independent of the
/// total mesh size, which makes it suitable for local edits on large meshes such as terrain grids.
///
/// Gives the same result for the targets as [`compute_tangents`] on the full mesh.
pub fn compute_tangents_partial(
    vertices: &mut [model::ModelVertex],
    indices: &[u32],
    targets: &[u32],
) {
    let zero = cgmath::Vector3::zero();
    let mut acc: HashMap<u32, (cgmath::Vector3<f32>, cgmath::Vector3<f32>)> =
        targets.iter().map(|&i| (i, (zero, zero))).collect();

    for c in indices.chunks_exact(3) {
        if !c.iter().any(|i| acc.contains_key(i)) {
            continue;
        }
        let (sdir, tdir) = triangle_dirs(vertices, c);
        for i in c {
            if let Some((t1, t2)) = acc.get_mut(i) {
                *t1 += sdir;
                *t2 += tdir;
            }
        }
    }

    for (i, (t1, t2)) in acc {
        finish_tangent(&mut vertices[i as usize], t1, t2);
    }
}

/// Per-triangle tangent (sdir) and bitangent (tdir) directions, unnormalized.
fn triangle_dirs(
    vertices: &[model::ModelVertex],
    tri: &[u32],
) -> (cgmath::Vector3<f32>, cgmath::Vector3<f32>) {
    let v1 = &vertices[tri[0] as usize];
    let v2 = &vertices[tri[1] as usize];
    let v3 = &vertices[tri[2] as usize];

    let p1: cgmath::Vector3<f32> = v1.position.into();
    let e1 = cgmath::Vector3::from(v2.position) - p1;
    let e2 = cgmath::Vector3::from(v3.position) - p1;

    let w1: cgmath::Vector2<f32> = v1.tex_coords.into();
    let (s1, t1) = (v2.tex_coords[0] - w1.x, v2.tex_coords[1] - w1.y);
    let (s2, t2) = (v3.tex_coords[0] - w1.x, v3.tex_coords[1] - w1.y);

    let r_denom = s1 * t2 - s2 * t1;
    let r = if r_denom.abs() < 1e-6 {
        0.0
    } else {
        1.0 / r_denom
    };

    ((e1 * t2 - e2 * t1) * r, (e2 * s1 - e1 * s2) * r)
}

/// Gram-Schmidt orthogonalizes the accumulated tangent against the normal and writes the result.
fn finish_tangent(vert: &mut model::ModelVertex, t: cgmath::Vector3<f32>, t2: cgmath::Vector3<f32>) {
    let n: cgmath::Vector3<f32> = vert.normal.into();
    let tangent_xyz = (t - n * n.dot(t)).normalize();

    if tangent_xyz.x.is_nan() {
        vert.tangent = [1.0, 0.0, 0.0];
        vert.bitangent = [0.0, 1.0, 0.0];
    } else {
        let w = if n.cross(t).dot(t2) < 0.0 { -1.0 } else { 1.0 };
        vert.tangent = tangent_xyz.into();
        vert.bitangent = (n.cross(tangent_xyz) * w).into();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cgmath::InnerSpace;

    fn make_vertex(pos: [f32; 3], uv: [f32; 2], normal: [f32; 3]) -> model::ModelVertex {
        model::ModelVertex {
            position: pos,
            tex_coords: uv,
            normal,
            tangent: [0.0; 3],
            bitangent: [0.0; 3],
        }
    }

    /// A simple XZ-plane quad with up-facing normals and a standard UV layout.
    fn quad_vertices_and_indices() -> (Vec<model::ModelVertex>, Vec<u32>) {
        let verts = vec![
            make_vertex([0.0, 0.0, 0.0], [0.0, 0.0], [0.0, 1.0, 0.0]),
            make_vertex([1.0, 0.0, 0.0], [1.0, 0.0], [0.0, 1.0, 0.0]),
            make_vertex([1.0, 0.0, 1.0], [1.0, 1.0], [0.0, 1.0, 0.0]),
            make_vertex([0.0, 0.0, 1.0], [0.0, 1.0], [0.0, 1.0, 0.0]),
        ];
        let indices = vec![0, 1, 2, 0, 2, 3];
        (verts, indices)
    }

    #[test]
    fn tangent_is_orthogonal_to_normal() {
        let (mut verts, indices) = quad_vertices_and_indices();
        compute_tangents(&mut verts, &indices);
        for v in &verts {
            let n: cgmath::Vector3<f32> = v.normal.into();
            let t: cgmath::Vector3<f32> = v.tangent.into();
            let dot = n.dot(t);
            assert!(
                dot.abs() < 1e-5,
                "tangent should be orthogonal to normal, got dot={}",
                dot
            );
        }
    }

    #[test]
    fn bitangent_is_orthogonal_to_normal_and_tangent() {
        let (mut verts, indices) = quad_vertices_and_indices();
        compute_tangents(&mut verts, &indices);
        for v in &verts {
            let n: cgmath::Vector3<f32> = v.normal.into();
            let t: cgmath::Vector3<f32> = v.tangent.into();
            let b: cgmath::Vector3<f32> = v.bitangent.into();
            assert!(
                n.dot(b).abs() < 1e-5,
                "bitangent should be orthogonal to normal"
            );
            assert!(
                t.dot(b).abs() < 1e-5,
                "bitangent should be orthogonal to tangent"
            );
        }
    }

    #[test]
    fn tangent_is_unit_length() {
        let (mut verts, indices) = quad_vertices_and_indices();
        compute_tangents(&mut verts, &indices);
        for v in &verts {
            let t: cgmath::Vector3<f32> = v.tangent.into();
            let len = t.magnitude();
            assert!(
                (len - 1.0).abs() < 1e-5,
                "tangent should be unit length, got {}",
                len
            );
        }
    }

    #[test]
    fn degenerate_uv_falls_back_to_default() {
        // All vertices have identical UVs → degenerate tangent space
        let mut verts = vec![
            make_vertex([0.0, 0.0, 0.0], [0.5, 0.5], [0.0, 1.0, 0.0]),
            make_vertex([1.0, 0.0, 0.0], [0.5, 0.5], [0.0, 1.0, 0.0]),
            make_vertex([0.0, 0.0, 1.0], [0.5, 0.5], [0.0, 1.0, 0.0]),
        ];
        let indices = vec![0, 1, 2];
        compute_tangents(&mut verts, &indices);
        // Should fall back to NaN guard: tangent=[1,0,0], bitangent=[0,1,0]
        for v in &verts {
            assert_eq!(v.tangent, [1.0, 0.0, 0.0], "degenerate UVs should use fallback tangent");
            assert_eq!(
                v.bitangent,
                [0.0, 1.0, 0.0],
                "degenerate UVs should use fallback bitangent"
            );
        }
    }

    #[test]
    fn empty_indices_produces_no_tangents() {
        let mut verts = vec![
            make_vertex([0.0, 0.0, 0.0], [0.0, 0.0], [0.0, 1.0, 0.0]),
        ];
        compute_tangents(&mut verts, &[]);
        // No triangles processed → zero tangent accumulator → NaN fallback
        assert_eq!(verts[0].tangent, [1.0, 0.0, 0.0]);
        assert_eq!(verts[0].bitangent, [0.0, 1.0, 0.0]);
    }

    #[test]
    fn incomplete_triangle_chunk_is_skipped() {
        let mut verts = vec![
            make_vertex([0.0, 0.0, 0.0], [0.0, 0.0], [0.0, 1.0, 0.0]),
            make_vertex([1.0, 0.0, 0.0], [1.0, 0.0], [0.0, 1.0, 0.0]),
        ];
        // Only 2 indices => not a complete triangle
        compute_tangents(&mut verts, &[0, 1]);
        // Should not panic, should produce fallback tangents
        assert_eq!(verts[0].tangent, [1.0, 0.0, 0.0]);
    }

    /// n×n vertex grid with wavy heights, UVs along x/z, and row-major indices.
    fn bumpy_grid(n: u32) -> (Vec<model::ModelVertex>, Vec<u32>) {
        let mut verts = Vec::new();
        for z in 0..n {
            for x in 0..n {
                let (fx, fz) = (x as f32, z as f32);
                let h = (fx * 0.7).sin() + (fz * 0.4).cos();
                let normal = cgmath::Vector3::new(-0.7 * (fx * 0.7).cos(), 1.0, 0.4 * (fz * 0.4).sin())
                    .normalize();
                verts.push(make_vertex([fx, h, fz], [fx / n as f32, fz / n as f32], normal.into()));
            }
        }
        let mut indices = Vec::new();
        for z in 0..n - 1 {
            for x in 0..n - 1 {
                let i = z * n + x;
                indices.extend_from_slice(&[i, i + n, i + 1, i + 1, i + n, i + n + 1]);
            }
        }
        (verts, indices)
    }

    #[test]
    fn partial_matches_full_and_leaves_others_untouched() {
        let n = 8;
        let (mut full, indices) = bumpy_grid(n);
        compute_tangents(&mut full, &indices);

        let (mut partial, _) = bumpy_grid(n);
        let sentinel = [9.0, 9.0, 9.0];
        for v in partial.iter_mut() {
            v.tangent = sentinel;
            v.bitangent = sentinel;
        }
        // Targets: vertices in the 3x3 block at (2..5, 2..5).
        let targets: Vec<u32> = (2..5).flat_map(|z| (2..5).map(move |x| z * n + x)).collect();
        // Triangles of the quads around that block (a superset is fine).
        let mut tris = Vec::new();
        for z in 1..5 {
            for x in 1..5 {
                let q = ((z * (n - 1) + x) * 6) as usize;
                tris.extend_from_slice(&indices[q..q + 6]);
            }
        }
        compute_tangents_partial(&mut partial, &tris, &targets);

        for (i, (p, f)) in partial.iter().zip(&full).enumerate() {
            if targets.contains(&(i as u32)) {
                for k in 0..3 {
                    assert!((p.tangent[k] - f.tangent[k]).abs() < 1e-5, "tangent mismatch at {i}");
                    assert!((p.bitangent[k] - f.bitangent[k]).abs() < 1e-5, "bitangent mismatch at {i}");
                }
            } else {
                assert_eq!(p.tangent, sentinel, "vertex {i} should be untouched");
                assert_eq!(p.bitangent, sentinel, "vertex {i} should be untouched");
            }
        }
    }
}
