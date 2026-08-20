//! ESP / box drawing — highlight boxes in 3D world

use crate::world::WorldVertex;

/// 3D AABB frame (12 edge lines). Lines are added as width-less quads to the vertex buffer.
pub struct EspBox {
    pub vertices: Vec<WorldVertex>,
}

impl EspBox {
    /// min/max world coordinates, color, thickness (world units, roughly px)
    pub fn new(
        min: [f32; 3],
        max: [f32; 3],
        color: [f32; 4],
        width: f32,
    ) -> Self {
        let (x0, y0, z0) = (min[0], min[1], min[2]);
        let (x1, y1, z1) = (max[0], max[1], max[2]);
        let w = width;

        // 12 kenar
        let edges: [[[f32; 3]; 2]; 12] = [
            [[x0, y0, z0], [x1, y0, z0]],
            [[x0, y0, z1], [x1, y0, z1]],
            [[x0, y1, z0], [x1, y1, z0]],
            [[x0, y1, z1], [x1, y1, z1]],
            [[x0, y0, z0], [x0, y0, z1]],
            [[x1, y0, z0], [x1, y0, z1]],
            [[x0, y1, z0], [x0, y1, z1]],
            [[x1, y1, z0], [x1, y1, z1]],
            [[x0, y0, z0], [x0, y1, z0]],
            [[x1, y0, z0], [x1, y1, z0]],
            [[x0, y0, z1], [x0, y1, z1]],
            [[x1, y0, z1], [x1, y1, z1]],
        ];

        let mut vertices = Vec::with_capacity(12 * 6);
        for [a, b] in edges {
            // quad: rectangle of width w along the a->b line
            let dir = [
                b[0] - a[0],
                b[1] - a[1],
                b[2] - a[2],
            ];
            let len = (dir[0] * dir[0] + dir[1] * dir[1] + dir[2] * dir[2]).sqrt().max(1e-6);
            // up vector; if edge is vertical, use x direction
            let up = if dir[1].abs() / len > 0.999 {
                [1.0f32, 0.0, 0.0]
            } else {
                [0.0f32, 1.0, 0.0]
            };
            // horizontal vector perpendicular to edge (not screen-space, simple approach)
            let cross = [
                dir[1] * up[2] - dir[2] * up[1],
                dir[2] * up[0] - dir[0] * up[2],
                dir[0] * up[1] - dir[1] * up[0],
            ];
            let clen = (cross[0] * cross[0] + cross[1] * cross[1] + cross[2] * cross[2])
                .sqrt()
                .max(1e-6);
            let n = [
                cross[0] / clen * w * 0.5,
                cross[1] / clen * w * 0.5,
                cross[2] / clen * w * 0.5,
            ];

            let q = [
                [a[0] + n[0], a[1] + n[1], a[2] + n[2]],
                [b[0] + n[0], b[1] + n[1], b[2] + n[2]],
                [b[0] - n[0], b[1] - n[1], b[2] - n[2]],
                [a[0] - n[0], a[1] - n[1], a[2] - n[2]],
            ];
            for i in [0usize, 1, 2, 0, 2, 3] {
                let p = q[i];
                vertices.push(WorldVertex {
                    pos: p,
                    uv: [0.0, 0.0],
                    color,
                    light: 0xFF,
                });
            }
        }

        Self { vertices }
    }
}