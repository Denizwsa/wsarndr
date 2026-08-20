use ash::vk;
use fontdue::Font;

use crate::vk::{Device, SharedDevice};

pub const ATLAS_COLS: usize = 32;
pub const ATLAS_ROWS: usize = 8;
pub const GLYPH_PX: usize = 16;

#[derive(Clone, Copy, Debug)]
pub struct GlyphInfo {
    pub u0: f32,
    pub v0: f32,
    pub u1: f32,
    pub v1: f32,
    pub width: f32,
    pub height: f32,
    pub x_offset: f32,
    pub y_offset: f32,
    pub advance: f32,
    pub cell_w: f32,
    pub cell_h: f32,
}

pub struct FontAtlas {
    pub device: SharedDevice,
    pub image: vk::Image,
    pub image_mem: vk::DeviceMemory,
    pub image_view: vk::ImageView,
    pub sampler: vk::Sampler,
    pub font: Font,
    pub font_size_px: f32,
    glyphs: Vec<GlyphInfo>,
}

impl FontAtlas {
    pub fn load(device: SharedDevice, ttf: &[u8], font_size_px: f32) -> anyhow::Result<Self> {
        let font = fontdue::Font::from_bytes(ttf, fontdue::FontSettings::default())
            .map_err(|e| anyhow::anyhow!("font parse error: {:?}", e))?;

        let cell_w = (font_size_px.ceil() as usize + 4).max(GLYPH_PX);
        let cell_h = (font_size_px.ceil() as usize + 4).max(GLYPH_PX);

        let atlas_w = (ATLAS_COLS * cell_w) as u32;
        let atlas_h = (ATLAS_ROWS * cell_h) as u32;
        let mut pixels = vec![0u8; atlas_w as usize * atlas_h as usize];

        let mut glyphs = Vec::with_capacity(97);

        for (i, code) in (32u32..=126).chain(0x011E..=0x011F).enumerate() {
            let col = i % ATLAS_COLS;
            let row = i / ATLAS_COLS;
            let ch = char::from_u32(code).unwrap_or('?');
            let (m, coverage) = font.rasterize(ch, font_size_px);
            let cw = m.width;
            let ch_px = m.height;
            let ox = col * cell_w + (cell_w - cw) / 2;
            let oy = row * cell_h + (cell_h - ch_px) / 2;
            for (py, line) in coverage.chunks(cw.max(1)).enumerate() {
                for (px, &a) in line.iter().enumerate() {
                    if a > 0 && ox + px < atlas_w as usize && oy + py < atlas_h as usize {
                        pixels[(oy + py) * atlas_w as usize + (ox + px)] = a;
                    }
                }
            }
            // Sub-rect UV covering only the actual glyph pixels within the cell
            let u0 = ox as f32 / atlas_w as f32;
            let v0 = oy as f32 / atlas_h as f32;
            let u1 = (ox + cw) as f32 / atlas_w as f32;
            let v1 = (oy + ch_px) as f32 / atlas_h as f32;
            glyphs.push(GlyphInfo {
                u0,
                v0,
                u1,
                v1,
                width: cw as f32,
                height: ch_px as f32,
                x_offset: m.xmin as f32,
                y_offset: m.ymin as f32,
                advance: m.advance_width,
                cell_w: cell_w as f32,
                cell_h: cell_h as f32,
            });
        }

        let atlas = Self::create_gpu_atlas(&device, &pixels, atlas_w, atlas_h)?;

        Ok(Self {
            device,
            image: atlas.0,
            image_mem: atlas.1,
            image_view: atlas.2,
            sampler: atlas.3,
            font,
            font_size_px,
            glyphs,
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
                    .mag_filter(vk::Filter::LINEAR)
                    .min_filter(vk::Filter::LINEAR)
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

    pub fn glyph_info(&self, code: u32) -> Option<GlyphInfo> {
        let idx = match code {
            32..=126 => (code - 32) as usize,
            0x011E..=0x011F => (code - 0x011E + 95) as usize,
            _ => return None,
        };
        self.glyphs.get(idx).copied()
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
