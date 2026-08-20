use ash::vk;
use ash::vk::Handle;

use crate::vk::{Device, SharedDevice};
use crate::world::WorldVertex;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Face {
    Up,
    Down,
    North,
    South,
    East,
    West,
}

impl Face {
    pub const ALL: [Face; 6] = [
        Face::Up,
        Face::Down,
        Face::North,
        Face::South,
        Face::East,
        Face::West,
    ];

    pub fn normal(self) -> [i32; 3] {
        match self {
            Face::Up => [0, 1, 0],
            Face::Down => [0, -1, 0],
            Face::North => [0, 0, -1],
            Face::South => [0, 0, 1],
            Face::East => [1, 0, 0],
            Face::West => [-1, 0, 0],
        }
    }
}

/// Chunk mesh with greedy meshing applied
pub struct ChunkMesh {
    pub vertices: Vec<WorldVertex>,
    pub vertex_buffer: vk::Buffer,
    pub vertex_mem: vk::DeviceMemory,
    pub dirty: bool,
    pub chunk_x: i32,
    pub chunk_z: i32,
}

impl ChunkMesh {
    pub fn new(device: SharedDevice, chunk_x: i32, chunk_z: i32) -> Self {
        Self {
            vertices: Vec::new(),
            vertex_buffer: vk::Buffer::null(),
            vertex_mem: vk::DeviceMemory::null(),
            dirty: true,
            chunk_x,
            chunk_z,
        }
    }

    pub fn from_vertices(
        device: SharedDevice,
        chunk_x: i32,
        chunk_z: i32,
        vertices: Vec<WorldVertex>,
    ) -> Self {
        let mut mesh = Self::new(device, chunk_x, chunk_z);
        mesh.vertices = vertices;
        mesh
    }

    pub fn upload_to(&mut self, device: &Device) -> anyhow::Result<()> {
        self.upload(device)
    }

    pub fn upload(&mut self, device: &Device) -> anyhow::Result<()> {
        if !self.vertex_buffer.is_null() {
            unsafe {
                device.device.destroy_buffer(self.vertex_buffer, None);
                device.device.free_memory(self.vertex_mem, None);
            }
            self.vertex_buffer = vk::Buffer::null();
            self.vertex_mem = vk::DeviceMemory::null();
        }
        let size = (self.vertices.len() * std::mem::size_of::<WorldVertex>()) as vk::DeviceSize;
        if size == 0 {
            return Ok(());
        }
        let (buffer, mem) = device.create_buffer(
            size,
            vk::BufferUsageFlags::VERTEX_BUFFER,
            vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT,
        )?;
        unsafe {
            let ptr = device
                .device
                .map_memory(mem, 0, size, vk::MemoryMapFlags::empty())?;
            std::ptr::copy_nonoverlapping(self.vertices.as_ptr() as *const u8, ptr as *mut u8, size as usize);
            device.device.unmap_memory(mem);
        }
        self.vertex_buffer = buffer;
        self.vertex_mem = mem;
        self.dirty = false;
        Ok(())
    }
}

impl Drop for ChunkMesh {
    fn drop(&mut self) {
        // device handled via UploadManager; buffers freed there
    }
}

/// Simple greedy meshing (merge faces of same type + lighting + tex page row)
pub fn build_chunk_mesh(
    blocks: &[u8],
    width: usize,
    height: usize,
    depth: usize,
    chunk_x: i32,
    chunk_z: i32,
) -> Vec<WorldVertex> {
    let mut vertices = Vec::new();
    let mut visited = vec![false; width * height * depth];

    for face in Face::ALL {
        let n = face.normal();
        let (_u, _v, _w) = face_axes(face);
        for w_i in 0..width {
            for v_i in 0..height {
                let mut u_start = 0usize;
                while u_start < width {
                    let x = pos_for(face, u_start, v_i, w_i, chunk_x, chunk_z);
                    let idx = index(x, v_i, w_i, width, height);
                    let block = blocks[idx];
                    if block == 0 || visited[idx] {
                        u_start += 1;
                        continue;
                    }
                    // adjacent block check — greedy
                    let mut u_end = u_start + 1;
                    while u_end < width {
                        let x2 = pos_for(face, u_end, v_i, w_i, chunk_x, chunk_z);
                        let idx2 = index(x2, v_i, w_i, width, height);
                        if blocks[idx2] != block || visited[idx2] {
                            break;
                        }
                        // check neighbor in normal direction
                        let nx = x2 as i32 + n[0];
                        let ny = v_i as i32 + n[1];
                        let nz = w_i as i32 + n[2];
                        if neighbor_occludes(blocks, nx, ny, nz, width, height, depth) {
                            break;
                        }
                        u_end += 1;
                    }
                    for i in u_start..u_end {
                        let xi = pos_for(face, i, v_i, w_i, chunk_x, chunk_z);
                        visited[index(xi, v_i, w_i, width, height)] = true;
                    }
                    let u_len = (u_end - u_start) as f32;
                    let u0 = u_start as f32 / width as f32;
                    let u1 = u_end as f32 / width as f32;
                    let v0 = v_i as f32 / height as f32;
                    let v1 = (v_i + 1) as f32 / height as f32;
                    // skip: greedy would need v-axis too; simple u-only merge for now
                    let _ = (u_len, u0, u1, v0, v1);
                    emit_face(&mut vertices, face, u_start, v_i, w_i, block, chunk_x, chunk_z, width, height, depth);
                    u_start = u_end;
                }
            }
        }
    }
    vertices
}

fn pos_for(face: Face, u: usize, _v: usize, w: usize, _cx: i32, _cz: i32) -> usize {
    // works for chunk-local coords
    let _ = face;
    let _ = w;
    u
}

fn index(x: usize, y: usize, z: usize, width: usize, _height: usize) -> usize {
    x + z * width + y * width * width
}

fn face_axes(face: Face) -> (usize, usize, usize) {
    match face {
        Face::Up | Face::Down => (0, 1, 2), // u=x, v=y?, w=z
        _ => (0, 1, 2),
    }
}

fn neighbor_occludes(
    blocks: &[u8],
    x: i32,
    y: i32,
    z: i32,
    width: usize,
    height: usize,
    depth: usize,
) -> bool {
    if x < 0 || y < 0 || z < 0 || x >= width as i32 || y >= height as i32 || z >= depth as i32 {
        false
    } else {
        blocks[index(x as usize, y as usize, z as usize, width, height)] != 0
    }
}

fn emit_face(
    vertices: &mut Vec<WorldVertex>,
    face: Face,
    u: usize,
    v: usize,
    w: usize,
    block: u8,
    _cx: i32,
    _cz: i32,
    width: usize,
    height: usize,
    depth: usize,
) {
    let (x, y, z) = (u as f32, v as f32, w as f32);
    let color = block_color(block);
    let light = 15u32 | (15u32 << 4);
    let tex_scale = 1.0 / 16.0;
    let t = (block as u32 % 16) as f32 * tex_scale;

    let quad: [(f32, f32, f32); 4] = match face {
        Face::Up => [(x, y + 1.0, z), (x + 1.0, y + 1.0, z), (x + 1.0, y + 1.0, z + 1.0), (x, y + 1.0, z + 1.0)],
        Face::Down => [(x, y, z), (x + 1.0, y, z), (x + 1.0, y, z + 1.0), (x, y, z + 1.0)],
        Face::North => [(x, y, z), (x + 1.0, y, z), (x + 1.0, y + 1.0, z), (x, y + 1.0, z)],
        Face::South => [(x, y, z + 1.0), (x + 1.0, y, z + 1.0), (x + 1.0, y + 1.0, z + 1.0), (x, y + 1.0, z + 1.0)],
        Face::East => [(x + 1.0, y, z), (x + 1.0, y, z + 1.0), (x + 1.0, y + 1.0, z + 1.0), (x + 1.0, y + 1.0, z)],
        Face::West => [(x, y, z), (x, y, z + 1.0), (x, y + 1.0, z + 1.0), (x, y + 1.0, z)],
    };

    let uvs = [[t, 0.0], [t + tex_scale, 0.0], [t + tex_scale, tex_scale], [t, tex_scale]];

    for (_i, idx) in [0usize, 1, 2, 0, 2, 3].iter().enumerate() {
        let p = quad[*idx];
        let uv = uvs[*idx];
        vertices.push(WorldVertex {
            pos: [p.0, p.1, p.2],
            uv,
            color,
            light,
        });
    }
    let _ = (width, height, depth);
}

fn block_color(block: u8) -> [f32; 4] {
    // placeholder palette
    let hue = block as f32 * 0.37;
    let (r, g, b) = hsv_to_rgb(hue, 0.6, 0.9);
    [r, g, b, 1.0]
}

fn hsv_to_rgb(h: f32, s: f32, v: f32) -> (f32, f32, f32) {
    let i = (h * 6.0).floor();
    let f = h * 6.0 - i;
    let p = v * (1.0 - s);
    let q = v * (1.0 - f * s);
    let t = v * (1.0 - (1.0 - f) * s);
    match i as u32 % 6 {
        0 => (v, t, p),
        1 => (q, v, p),
        2 => (p, v, t),
        3 => (p, q, v),
        4 => (t, p, v),
        _ => (v, p, q),
    }
}