use glam::{Mat4, Vec3};

pub struct Frustum {
    planes: [[f32; 4]; 6],
}

impl Frustum {
    pub fn from_view_proj(vp: &Mat4) -> Self {
        let m = vp.to_cols_array();
        // rows of the matrix (column-major)
        let r = |i: usize| [m[i], m[i + 4], m[i + 8], m[i + 12]];
        let (r0, r1, r2, r3) = (r(0), r(1), r(2), r(3));

        let mut planes = [[0.0f32; 4]; 6];
        planes[0] = [
            r3[0] + r0[0],
            r3[1] + r0[1],
            r3[2] + r0[2],
            r3[3] + r0[3],
        ]; // left
        planes[1] = [
            r3[0] - r0[0],
            r3[1] - r0[1],
            r3[2] - r0[2],
            r3[3] - r0[3],
        ]; // right
        planes[2] = [
            r3[0] + r1[0],
            r3[1] + r1[1],
            r3[2] + r1[2],
            r3[3] + r1[3],
        ]; // bottom
        planes[3] = [
            r3[0] - r1[0],
            r3[1] - r1[1],
            r3[2] - r1[2],
            r3[3] - r1[3],
        ]; // top
        planes[4] = [
            r3[0] + r2[0],
            r3[1] + r2[1],
            r3[2] + r2[2],
            r3[3] + r2[3],
        ]; // near
        planes[5] = [
            r3[0] - r2[0],
            r3[1] - r2[1],
            r3[2] - r2[2],
            r3[3] - r2[3],
        ]; // far

        for p in planes.iter_mut() {
            let len = (p[0] * p[0] + p[1] * p[1] + p[2] * p[2]).sqrt();
            if len > 0.0 {
                for c in p.iter_mut() {
                    *c /= len;
                }
            }
        }

        Self { planes }
    }

    /// AABB vs frustum test
    pub fn intersects_aabb(&self, min: Vec3, max: Vec3) -> bool {
        for p in &self.planes {
            let px = if p[0] >= 0.0 { max.x } else { min.x };
            let py = if p[1] >= 0.0 { max.y } else { min.y };
            let pz = if p[2] >= 0.0 { max.z } else { min.z };
            if p[0] * px + p[1] * py + p[2] * pz + p[3] < 0.0 {
                return false;
            }
        }
        true
    }
}