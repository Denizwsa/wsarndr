
use ash::vk;
use fontdue::Font;

use crate::vk::{Device, SharedDevice};

pub const ATLAS_COLS: usize = 32;
pub const ATLAS_ROWS: usize = 8;
pub const GLYPH_PX: usize = 16;
pub const ATLAS_W: usize = ATLAS_COLS * GLYPH_PX;
pub const ATLAS_H: usize = ATLAS_ROWS * GLYPH_PX;

pub struct FontAtlas {
    pub device: SharedDevice,
    pub image: vk::Image,
    pub image_mem: vk::DeviceMemory,
    pub image_view: vk::ImageView,
    pub sampler: vk::Sampler,
    pub font: Font,
    pub font_size_px: f32,
    glyph_w: usize,
    glyph_h: usize,
}

impl FontAtlas {
    pub fn load(device: SharedDevice, ttf: &[u8], font_size_px: f32) -> anyhow::Result<Self> {
        let font = fontdue::Font::from_bytes(ttf, fontdue::FontSettings::default())
            .map_err(|e| anyhow::anyhow!("font parse error: {:?}", e))?;

        // Rasterize ASCII printable range (32..=126)
        let metrics = font.metrics('W', font_size_px);
        let glyph_w = (metrics.advance_width.ceil() as usize + 4).max(GLYPH_PX);
        let glyph_h = (font_size_px.ceil() as usize + 4).max(GLYPH_PX);

        let atlas_w = (ATLAS_COLS * glyph_w) as u32;
        let atlas_h = (ATLAS_ROWS * glyph_h) as u32;
        let mut pixels = vec![0u8; atlas_w as usize * atlas_h as usize];

        for (i, code) in (32u32..=126).chain(0x011E..=0x011F).enumerate() {
            let col = i % ATLAS_COLS;
            let row = i / ATLAS_COLS;
            let ch = char::from_u32(code).unwrap_or('?');
            let (m, coverage) = font.rasterize(ch, font_size_px);
            let cw = m.width;
            let ch_px = m.height;
            let ox = col * glyph_w + (glyph_w - cw) / 2;
            let oy = row * glyph_h + (glyph_h - ch_px) / 2;
            for (y, line) in coverage.chunks(cw.max(1)).enumerate() {
                for (x, &a) in line.iter().enumerate() {
                    if a > 0 && ox + x < atlas_w as usize && oy + y < atlas_h as usize {
                        pixels[(oy + y) * atlas_w as usize + (ox + x)] = a;
                    }
                }
            }
        }

        let atlas = Self::create_gpu_atlas(&device, &pixels, atlas_w, atlas_h)?;

        {
            use std::io::Write;
            let mut f = std::fs::File::create("/tmp/wsarndr_atlas.pgm").unwrap();
            write!(f, "P5\n{} {}\n255\n", atlas_w, atlas_h).unwrap();
            f.write_all(&pixels).unwrap();
            log::info!("atlas dumped: {}x{} glyph_w={} glyph_h={}", atlas_w, atlas_h, glyph_w, glyph_h);
        }

        Ok(Self {
            device,
            image: atlas.0,
            image_mem: atlas.1,
            image_view: atlas.2,
            sampler: atlas.3,
            font,
            font_size_px,
            glyph_w,
            glyph_h,
        })
    }

    fn create_gpu_atlas(
        device: &Device,
        pixels: &[u8],
        w: u32,
        h: u32,
    ) -> anyhow::Result<(
        vk::Image,
        vk::DeviceMemory,
        vk::ImageView,
        vk::Sampler,
    )> {
        let pool = unsafe { device.device.create_command_pool(
            &vk::CommandPoolCreateInfo::default()
                .queue_family_index(device.queue_family_index),
            None,
        )? };

        let (staging, staging_mem) = device.create_buffer(
            pixels.len() as vk::DeviceSize,
            vk::BufferUsageFlags::TRANSFER_SRC,
            vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT,
        )?;

        unsafe {
            let ptr = device
                .device
                .map_memory(staging_mem, 0, pixels.len() as vk::DeviceSize, vk::MemoryMapFlags::empty())?;
            std::ptr::copy_nonoverlapping(pixels.as_ptr(), ptr as *mut u8, pixels.len());
            device.device.unmap_memory(staging_mem);
        }

        let (image, image_mem) = device.create_image(
            w,
            h,
            vk::Format::R8_UNORM,
            vk::ImageUsageFlags::TRANSFER_DST | vk::ImageUsageFlags::SAMPLED,
            vk::MemoryPropertyFlags::DEVICE_LOCAL,
        )?;

        device.single_time_command(pool, |cmd| {
            device
                .transition_image_layout(
                    cmd,
                    image,
                    vk::ImageLayout::UNDEFINED,
                    vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                )
                .ok();
            device.copy_buffer_to_image(cmd, staging, image, w, h);
            device
                .transition_image_layout(
                    cmd,
                    image,
                    vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                    vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                )
                .ok();
        })?;

        let image_view = unsafe {
            device.device.create_image_view(
                &vk::ImageViewCreateInfo::default()
                    .image(image)
                    .view_type(vk::ImageViewType::TYPE_2D)
                    .format(vk::Format::R8_UNORM)
                    .subresource_range(
                        vk::ImageSubresourceRange::default()
                            .aspect_mask(vk::ImageAspectFlags::COLOR)
                            .level_count(1)
                            .layer_count(1),
                    ),
                None,
            )?
        };

        let sampler = unsafe {
            device.device.create_sampler(
                &vk::SamplerCreateInfo::default()
                    .mag_filter(vk::Filter::NEAREST)
                    .min_filter(vk::Filter::NEAREST)
                    .mipmap_mode(vk::SamplerMipmapMode::NEAREST)
                    .address_mode_u(vk::SamplerAddressMode::CLAMP_TO_EDGE)
                    .address_mode_v(vk::SamplerAddressMode::CLAMP_TO_EDGE)
                    .address_mode_w(vk::SamplerAddressMode::CLAMP_TO_EDGE),
                None,
            )?
        };

        unsafe {
            device.device.destroy_command_pool(pool, None);
            device.device.destroy_buffer(staging, None);
            device.device.free_memory(staging_mem, None);
        }

        Ok((image, image_mem, image_view, sampler))
    }

    pub fn glyph_dimensions(&self) -> (usize, usize) {
        (self.glyph_w, self.glyph_h)
    }

    pub fn uv_for_glyph(&self, code: u32) -> Option<[f32; 4]> {
        let idx = match code {
            32..=126 => (code - 32) as usize,
            0x011E..=0x011F => (code - 0x011E + 95) as usize,
            _ => return None,
        };
        let col = idx % ATLAS_COLS;
        let row = idx / ATLAS_COLS;
        let atlas_w = (ATLAS_COLS * self.glyph_w) as f32;
        let atlas_h = (ATLAS_ROWS * self.glyph_h) as f32;
        let u0 = (col * self.glyph_w) as f32 / atlas_w;
        let v0 = (row * self.glyph_h) as f32 / atlas_h;
        let u1 = ((col + 1) * self.glyph_w) as f32 / atlas_w;
        let v1 = ((row + 1) * self.glyph_h) as f32 / atlas_h;
        Some([u0, v0, u1, v1])
    }
}

impl Drop for FontAtlas {
    fn drop(&mut self) {
        unsafe {
            self.device.device.device_wait_idle().ok();
            self.device.device.destroy_sampler(self.sampler, None);
            self.device.device.destroy_image_view(self.image_view, None);
            self.device.device.destroy_image(self.image, None);
            self.device.device.free_memory(self.image_mem, None);
        }
    }
}

pub fn load_default_font(device: SharedDevice, size_px: f32) -> anyhow::Result<FontAtlas> {
    const CANDIDATES: &[&str] = &[
        "/usr/share/fonts/liberation/LiberationMono-Regular.ttf",
        "/usr/share/fonts/TTF/JetBrainsMonoNerdFont-Regular.ttf",
        "/usr/share/fonts/noto/NotoSansMono-Regular.ttf",
        "/usr/share/fonts/dejavu/DejaVuSansMono.ttf",
    ];
    for path in CANDIDATES {
        if let Ok(data) = std::fs::read(path) {
            log::info!("font: {}", path);
            return FontAtlas::load(device, &data, size_px);
        }
    }
    anyhow::bail!("no system font found (checked {:?})", CANDIDATES)
}