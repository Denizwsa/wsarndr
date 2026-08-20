pub mod font;
pub mod shape;


use ash::vk;
use bytemuck::{Pod, Zeroable};

use crate::vk::{Device, Instance, SharedDevice, SharedInstance};
use crate::vg::font::FontAtlas;
use crate::vg::shape::{Color, GradMode, Shape, ShapeKind, TextAlign};

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable, Debug)]
pub struct VgVertex {
    pub pos: [f32; 2],
    pub uv: [f32; 2],
    pub color: [f32; 4],
    pub param: [f32; 4],
    pub bounds: [f32; 4],
    pub grad_color: [f32; 4],
    pub grad_from: [f32; 2],
    pub grad_to: [f32; 2],
    pub grad_params: [f32; 4],
}

const FLAG_TEXTURE: f32 = 1.0;
const FLAG_STROKE: f32 = 2.0;
const FLAG_LINE: f32 = 4.0;

#[repr(C)]
#[derive(Pod, Zeroable, Copy, Clone)]
pub struct VgPushConstants {
    pub viewport: [f32; 2],
    pub scale: f32,
    pub _pad: f32,
}

pub struct VgContext {
    pub device: SharedDevice,
    pub instance: SharedInstance,
    pub render_pass: vk::RenderPass,
    pub pipeline: vk::Pipeline,
    pub pipeline_layout: vk::PipelineLayout,
    pub descriptor_set_layout: vk::DescriptorSetLayout,
    pub descriptor_pool: vk::DescriptorPool,
    pub font_descriptor_set: vk::DescriptorSet,
    pub font_atlas: FontAtlas,
    pub vertex_buffer: vk::Buffer,
    pub vertex_buffer_mem: vk::DeviceMemory,
    pub vertex_capacity: usize,
    pub vertices: Vec<VgVertex>,
    pub viewport_size: [f32; 2],
    pub scale: f32,
    pub needs_upload: bool,
    pub translate: [f32; 2],
    pub transform_stack: Vec<[f32; 2]>,
}

impl VgContext {
    pub fn new(
        device: SharedDevice,
        instance: SharedInstance,
        render_pass: vk::RenderPass,
        font_atlas: FontAtlas,
    ) -> anyhow::Result<Self> {
        let _d = &device.device;

        let (pipeline, layout, dsl, pool) =
            unsafe { create_pipeline_and_layout(device.as_ref(), instance.as_ref(), render_pass)? };

        let font_descriptor_set = unsafe {
            device.device.allocate_descriptor_sets(
                &vk::DescriptorSetAllocateInfo::default()
                    .descriptor_pool(pool)
                    .set_layouts(std::slice::from_ref(&dsl)),
            )?[0]
        };
        let image_info = vk::DescriptorImageInfo::default()
            .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
            .image_view(font_atlas.image_view)
            .sampler(font_atlas.sampler);
        let write = vk::WriteDescriptorSet::default()
            .dst_set(font_descriptor_set)
            .dst_binding(0)
            .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
            .image_info(std::slice::from_ref(&image_info));
        unsafe { device.device.update_descriptor_sets(std::slice::from_ref(&write), &[]) };

        let (vertex_buffer, vertex_buffer_mem) =
            unsafe { create_vertex_buffer(device.as_ref(), 1 << 16)? };
        let vertex_capacity = 1 << 16;

        Ok(Self {
            device,
            instance,
            render_pass,
            pipeline,
            pipeline_layout: layout,
            descriptor_set_layout: dsl,
            descriptor_pool: pool,
            font_descriptor_set,
            font_atlas,
            vertex_buffer,
            vertex_buffer_mem,
            vertex_capacity,
            vertices: Vec::new(),
            viewport_size: [0.0, 0.0],
            scale: 1.0,
            needs_upload: false,
            translate: [0.0, 0.0],
            transform_stack: Vec::new(),
        })
    }

    pub fn begin_frame(&mut self, width: f32, height: f32, scale: f32) {
        self.viewport_size = [width, height];
        self.scale = scale;
        self.vertices.clear();
        self.needs_upload = false;
        self.translate = [0.0, 0.0];
        self.transform_stack.clear();
    }

    pub fn upload(&mut self, device: &Device) -> anyhow::Result<()> {
        let size = (self.vertices.len() * std::mem::size_of::<VgVertex>()) as vk::DeviceSize;
        if size == 0 {
            return Ok(());
        }
        // Recreate vertex buffer if capacity is exceeded (prevents overflow)
        if (size as usize) > self.vertex_capacity {
            let new_cap = (size as usize).next_power_of_two().max(1 << 16);
            unsafe {
                device
                    .device
                    .destroy_buffer(self.vertex_buffer, None);
                device.device.free_memory(self.vertex_buffer_mem, None);
            }
            let (nbuf, nmem) = unsafe { create_vertex_buffer(device, new_cap)? };
            self.vertex_buffer = nbuf;
            self.vertex_buffer_mem = nmem;
            self.vertex_capacity = new_cap;
            log::warn!("vg vertex buffer grew to {}", new_cap);
        }
        unsafe {
            let ptr = device
                .device
                .map_memory(self.vertex_buffer_mem, 0, size, vk::MemoryMapFlags::empty())?;
            std::ptr::copy_nonoverlapping(
                self.vertices.as_ptr() as *const u8,
                ptr as *mut u8,
                size as usize,
            );
            device.device.unmap_memory(self.vertex_buffer_mem);
        }
        self.needs_upload = true;
        Ok(())
    }

    pub fn draw(&self, cmd: vk::CommandBuffer, extent: vk::Extent2D) {
        if self.vertices.is_empty() {
            return;
        }
        unsafe {
            self.device
                .device
                .cmd_bind_pipeline(cmd, vk::PipelineBindPoint::GRAPHICS, self.pipeline);
            self.device
                .device
                .cmd_bind_vertex_buffers(cmd, 0, &[self.vertex_buffer], &[0]);
            self.device.device.cmd_bind_descriptor_sets(
                cmd,
                vk::PipelineBindPoint::GRAPHICS,
                self.pipeline_layout,
                0,
                &[self.font_descriptor_set],
                &[],
            );

            let viewport = vk::Viewport {
                x: 0.0,
                y: 0.0,
                width: extent.width as f32,
                height: extent.height as f32,
                min_depth: 0.0,
                max_depth: 1.0,
            };
            self.device.device.cmd_set_viewport(cmd, 0, &[viewport]);
            let scissor = vk::Rect2D {
                offset: vk::Offset2D { x: 0, y: 0 },
                extent,
            };
            self.device.device.cmd_set_scissor(cmd, 0, &[scissor]);

            let push = VgPushConstants {
                viewport: [extent.width as f32, extent.height as f32],
                scale: self.scale,
                _pad: 0.0,
            };
            self.device.device.cmd_push_constants(
                cmd,
                self.pipeline_layout,
                vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT,
                0,
                bytemuck::bytes_of(&push),
            );

            self.device
                .device
                .cmd_draw(cmd, self.vertices.len() as u32, 1, 0, 0);
        }
    }

    pub fn draw_shape(&mut self, shape: &Shape) {
        let (bounds, half) = shape.bounds();
        let (corner_radius, stroke, flags) = match shape.kind {
            ShapeKind::RoundedRect { radius } => (
                radius,
                shape.stroke_width,
                if shape.stroke_width > 0.0 { FLAG_STROKE } else { 0.0 },
            ),
            ShapeKind::Rect => (
                0.0,
                shape.stroke_width,
                if shape.stroke_width > 0.0 { FLAG_STROKE } else { 0.0 },
            ),
            ShapeKind::Circle => (
                half.max(0.001),
                shape.stroke_width,
                if shape.stroke_width > 0.0 { FLAG_STROKE } else { 0.0 },
            ),
            ShapeKind::Triangle { .. } => (
                0.0,
                shape.stroke_width,
                if shape.stroke_width > 0.0 { FLAG_STROKE } else { 0.0 },
            ),
            ShapeKind::Arc { .. } => (
                half.max(0.001),
                shape.stroke_width,
                if shape.stroke_width > 0.0 { FLAG_STROKE } else { 0.0 },
            ),
            ShapeKind::Line { .. } => (
                shape.stroke_width * 0.5 + 0.5,
                shape.stroke_width,
                FLAG_LINE + FLAG_STROKE,
            ),
            ShapeKind::Text => (0.0, 0.0, FLAG_TEXTURE),
        };

        let corners = [
            [bounds[0], bounds[1]],
            [bounds[2], bounds[1]],
            [bounds[2], bounds[3]],
            [bounds[0], bounds[3]],
        ];
        let tri_indices: [[usize; 3]; 2] = [[0, 1, 2], [0, 2, 3]];

        let feather = (1.0 / self.scale).max(0.5);

        let default_uv = self
            .font_atlas
            .glyph_info(b'?' as u32)
            .map_or([0.0, 0.0], |gi| [gi.u0, gi.v0]);

        for tri in tri_indices {
            for &i in &tri {
                let c = corners[i];
                let px = c[0];
                let py = c[1];
                let (vw, vh) = (self.viewport_size[0].max(1.0), self.viewport_size[1].max(1.0));
                let clip_x = px / vw * 2.0 - 1.0;
                let clip_y = 1.0 - py / vh * 2.0;
                let uv = if let Some(uv4) = shape.uv_override {
                    let cx = if i == 0 || i == 3 { uv4[0] } else { uv4[2] };
                    let cy = if i < 2 { uv4[3] } else { uv4[1] };
                    [cx, cy]
                } else {
                    default_uv
                };
                let v = VgVertex {
                    pos: [clip_x, clip_y],
                    uv,
                    color: shape.fill_color.to_array(),
                    param: [stroke, feather, corner_radius, flags],
                    bounds,
                    grad_color: shape.grad_color.to_array(),
                    grad_from: shape.grad_from,
                    grad_to: shape.grad_to,
                    grad_params: [
                        match shape.grad_mode {
                            GradMode::None => 0.0,
                            GradMode::Linear => 1.0,
                            GradMode::Radial => 2.0,
                        },
                        shape.grad_inner_radius,
                        0.0,
                        0.0,
                    ],
                };
                self.vertices.push(v);
            }
        }
    }

    // ---- Immediate helpers ----

    pub fn rounded_rect_fill(
        &mut self,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        r: f32,
        color: impl Into<Color>,
    ) {
        let (x, y) = self.apply(x, y);
        self.draw_shape(&Shape {
            kind: ShapeKind::RoundedRect { radius: r },
            x,
            y,
            w,
            h,
            fill_color: color.into(),
            stroke_width: 0.0,
            ..Default::default()
        });
    }

    pub fn rounded_rect_stroke(
        &mut self,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        r: f32,
        width: f32,
        color: impl Into<Color>,
    ) {
        let (x, y) = self.apply(x, y);
        self.draw_shape(&Shape {
            kind: ShapeKind::RoundedRect { radius: r },
            x,
            y,
            w,
            h,
            fill_color: color.into(),
            stroke_width: width,
            ..Default::default()
        });
    }

    pub fn rect_fill(&mut self, x: f32, y: f32, w: f32, h: f32, color: impl Into<Color>) {
        let (x, y) = self.apply(x, y);
        self.draw_shape(&Shape {
            kind: ShapeKind::Rect,
            x,
            y,
            w,
            h,
            fill_color: color.into(),
            stroke_width: 0.0,
            ..Default::default()
        });
    }

    pub fn rect_stroke(
        &mut self,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        width: f32,
        color: impl Into<Color>,
    ) {
        let (x, y) = self.apply(x, y);
        self.draw_shape(&Shape {
            kind: ShapeKind::Rect,
            x,
            y,
            w,
            h,
            fill_color: color.into(),
            stroke_width: width,
            ..Default::default()
        });
    }

    pub fn circle_fill(&mut self, cx: f32, cy: f32, r: f32, color: impl Into<Color>) {
        let (cx, cy) = self.apply(cx, cy);
        self.draw_shape(&Shape {
            kind: ShapeKind::Circle,
            x: cx - r,
            y: cy - r,
            w: r * 2.0,
            h: r * 2.0,
            fill_color: color.into(),
            stroke_width: 0.0,
            ..Default::default()
        });
    }

    pub fn circle_stroke(
        &mut self,
        cx: f32,
        cy: f32,
        r: f32,
        width: f32,
        color: impl Into<Color>,
    ) {
        let (cx, cy) = self.apply(cx, cy);
        self.draw_shape(&Shape {
            kind: ShapeKind::Circle,
            x: cx - r,
            y: cy - r,
            w: r * 2.0,
            h: r * 2.0,
            fill_color: color.into(),
            stroke_width: width,
            ..Default::default()
        });
    }

    pub fn line(&mut self, x0: f32, y0: f32, x1: f32, y1: f32, width: f32, color: impl Into<Color>) {
        let (x0, y0) = self.apply(x0, y0);
        let (x1, y1) = self.apply(x1, y1);
        let d = ((x1 - x0).powi(2) + (y1 - y0).powi(2)).sqrt();
        let pad = width * 0.5 + 2.0;
        let nx = if d > 0.0 { -(y1 - y0) / d } else { 1.0 };
        let ny = if d > 0.0 { (x1 - x0) / d } else { 0.0 };
        self.draw_shape(&Shape {
            kind: ShapeKind::Line { x1, y1 },
            x: x0.min(x1) - pad,
            y: y0.min(y1) - pad,
            w: (x0 - x1).abs() + pad * 2.0,
            h: (y0 - y1).abs() + pad * 2.0,
            fill_color: color.into(),
            stroke_width: width,
            _line_n: [nx, ny],
            ..Default::default()
        });
    }

    pub fn text(&mut self, x: f32, y: f32, text: &str, size: f32, color: impl Into<Color>) {
        self.text_aligned(x, y, text, size, color, TextAlign::Left);
    }

    pub fn text_aligned(
        &mut self,
        x: f32,
        y: f32,
        text: &str,
        size: f32,
        color: impl Into<Color>,
        align: TextAlign,
    ) {
        let color = color.into();
        let scale = size / self.font_atlas.font_size_px;
        let (x, y) = self.apply(x, y);
        let mut pen_x = match align {
            TextAlign::Left => x,
            TextAlign::Center => x - self.text_width(text, size) * 0.5,
            TextAlign::Right => x - self.text_width(text, size),
        };
        for ch in text.chars() {
            if let Some(gi) = self.font_atlas.glyph_info(ch as u32) {
                let gw = gi.width * scale;
                let gh = gi.height * scale;
                let uv = [gi.u0, gi.v0, gi.u1, gi.v1];
                // y is top of the text box; baseline is top + ascent.
                let baseline = y + self.font_atlas.ascent * scale;
                let gx = pen_x + gi.x_offset * scale;
                let gy = baseline - gi.y_offset * scale - gh;
                if gw > 0.0 && gh > 0.0 {
                    let shape = Shape {
                        kind: ShapeKind::Text,
                        x: gx,
                        y: gy,
                        w: gw,
                        h: gh,
                        fill_color: color,
                        stroke_width: 0.0,
                        uv_override: Some(uv),
                        ..Default::default()
                    };
                    self.draw_shape(&shape);
                }
                pen_x += gi.advance * scale;
            }
        }
    }

    pub fn text_width(&self, text: &str, size: f32) -> f32 {
        let scale = size / self.font_atlas.font_size_px;
        let mut w = 0.0f32;
        for ch in text.chars() {
            if let Some(gi) = self.font_atlas.glyph_info(ch as u32) {
                w += gi.advance * scale;
            }
        }
        w
    }

    /// Returns (width, height) for the given text at the specified size.
    pub fn measure_text(&self, text: &str, size: f32) -> (f32, f32) {
        let scale = size / self.font_atlas.font_size_px;
        let cell_h = self.font_atlas.glyph_info(b'X' as u32)
            .map_or(size, |gi| gi.cell_h * scale);
        (self.text_width(text, size), cell_h)
    }

    // ---- Coordinate transform stack ----

    /// Push a translate transform. All subsequent draws are offset by (tx, ty).
    pub fn push_translate(&mut self, tx: f32, ty: f32) {
        self.transform_stack.push(self.translate);
        self.translate[0] += tx;
        self.translate[1] += ty;
    }

    /// Pop the last transform.
    pub fn pop_transform(&mut self) {
        if let Some(prev) = self.transform_stack.pop() {
            self.translate = prev;
        }
    }

    /// Apply current transform to coordinates.
    fn apply(&self, x: f32, y: f32) -> (f32, f32) {
        (x + self.translate[0], y + self.translate[1])
    }

    // ---- Triangle ----

    pub fn triangle_fill(
        &mut self,
        x1: f32, y1: f32,
        x2: f32, y2: f32,
        x3: f32, y3: f32,
        color: impl Into<Color>,
    ) {
        let (x1, y1) = self.apply(x1, y1);
        let (x2, y2) = self.apply(x2, y2);
        let (x3, y3) = self.apply(x3, y3);
        let min_x = x1.min(x2).min(x3);
        let min_y = y1.min(y2).min(y3);
        let max_x = x1.max(x2).max(x3);
        let max_y = y1.max(y2).max(y3);
        self.draw_shape(&Shape {
            kind: ShapeKind::Triangle { x2, y2, x3, y3 },
            x: min_x,
            y: min_y,
            w: (max_x - min_x).max(0.1),
            h: (max_y - min_y).max(0.1),
            fill_color: color.into(),
            stroke_width: 0.0,
            _line_n: [x1, y1],
            ..Default::default()
        });
    }

    pub fn triangle_stroke(
        &mut self,
        x1: f32, y1: f32,
        x2: f32, y2: f32,
        x3: f32, y3: f32,
        width: f32,
        color: impl Into<Color>,
    ) {
        let (x1, y1) = self.apply(x1, y1);
        let (x2, y2) = self.apply(x2, y2);
        let (x3, y3) = self.apply(x3, y3);
        let min_x = x1.min(x2).min(x3);
        let min_y = y1.min(y2).min(y3);
        let max_x = x1.max(x2).max(x3);
        let max_y = y1.max(y2).max(y3);
        self.draw_shape(&Shape {
            kind: ShapeKind::Triangle { x2, y2, x3, y3 },
            x: min_x,
            y: min_y,
            w: (max_x - min_x).max(0.1),
            h: (max_y - min_y).max(0.1),
            fill_color: color.into(),
            stroke_width: width,
            _line_n: [x1, y1],
            ..Default::default()
        });
    }

    // ---- Arc (partial circle / pie slice) ----

    pub fn arc_fill(
        &mut self,
        cx: f32, cy: f32,
        r: f32,
        start_deg: f32,
        sweep_deg: f32,
        color: impl Into<Color>,
    ) {
        let (cx, cy) = self.apply(cx, cy);
        self.draw_shape(&Shape {
            kind: ShapeKind::Arc { start: start_deg.to_radians(), sweep: sweep_deg.to_radians() },
            x: cx - r,
            y: cy - r,
            w: r * 2.0,
            h: r * 2.0,
            fill_color: color.into(),
            stroke_width: 0.0,
            _line_n: [cx, cy],
            ..Default::default()
        });
    }

    pub fn arc_stroke(
        &mut self,
        cx: f32, cy: f32,
        r: f32,
        start_deg: f32,
        sweep_deg: f32,
        width: f32,
        color: impl Into<Color>,
    ) {
        let (cx, cy) = self.apply(cx, cy);
        self.draw_shape(&Shape {
            kind: ShapeKind::Arc { start: start_deg.to_radians(), sweep: sweep_deg.to_radians() },
            x: cx - r,
            y: cy - r,
            w: r * 2.0,
            h: r * 2.0,
            fill_color: color.into(),
            stroke_width: width,
            _line_n: [cx, cy],
            ..Default::default()
        });
    }

    pub fn linear_gradient_rounded_rect(
        &mut self,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        r: f32,
        from: [f32; 2],
        to: [f32; 2],
        c0: Color,
        c1: Color,
    ) {
        let (x, y) = self.apply(x, y);
        let (fx, fy) = self.apply(from[0], from[1]);
        let (tx, ty) = self.apply(to[0], to[1]);
        self.draw_shape(&Shape {
            kind: ShapeKind::RoundedRect { radius: r },
            x,
            y,
            w,
            h,
            fill_color: c0,
            grad_color: c1,
            grad_mode: GradMode::Linear,
            grad_from: [fx, fy],
            grad_to: [tx, ty],
            stroke_width: 0.0,
            ..Default::default()
        });
    }

    pub fn radial_gradient_circle(&mut self, cx: f32, cy: f32, r: f32, c0: Color, c1: Color) {
        let (cx, cy) = self.apply(cx, cy);
        self.draw_shape(&Shape {
            kind: ShapeKind::Circle,
            x: cx - r,
            y: cy - r,
            w: r * 2.0,
            h: r * 2.0,
            fill_color: c0,
            grad_color: c1,
            grad_mode: GradMode::Radial,
            grad_from: [cx, cy],
            grad_to: [cx + r, cy],
            grad_inner_radius: 0.0,
            stroke_width: 0.0,
            ..Default::default()
        });
    }

    pub fn clear() {}
}

unsafe fn create_vertex_buffer(
    device: &Device,
    size: usize,
) -> anyhow::Result<(vk::Buffer, vk::DeviceMemory)> {
    let (buffer, mem) = device.create_buffer(
        size as vk::DeviceSize,
        vk::BufferUsageFlags::VERTEX_BUFFER,
        vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT,
    )?;
    Ok((buffer, mem))
}

unsafe fn create_pipeline_and_layout(
    device: &Device,
    instance: &Instance,
    render_pass: vk::RenderPass,
) -> anyhow::Result<(
    vk::Pipeline,
    vk::PipelineLayout,
    vk::DescriptorSetLayout,
    vk::DescriptorPool,
)> {
    let d = &device.device;

    let vert_spv = include_spirv("vg_vert")?;
    let frag_spv = include_spirv("vg_frag")?;

    let vert_mod = d.create_shader_module(
        &vk::ShaderModuleCreateInfo::default().code(&vert_spv),
        None,
    )?;
    let frag_mod = d.create_shader_module(
        &vk::ShaderModuleCreateInfo::default().code(&frag_spv),
        None,
    )?;

    let main_cstr = std::ffi::CString::new("main").unwrap();
    let stage_info = [
        vk::PipelineShaderStageCreateInfo::default()
            .stage(vk::ShaderStageFlags::VERTEX)
            .module(vert_mod)
            .name(&main_cstr),
        vk::PipelineShaderStageCreateInfo::default()
            .stage(vk::ShaderStageFlags::FRAGMENT)
            .module(frag_mod)
            .name(&main_cstr),
    ];

    let binding = vk::VertexInputBindingDescription::default()
        .binding(0)
        .stride(std::mem::size_of::<VgVertex>() as u32)
        .input_rate(vk::VertexInputRate::VERTEX);

    let attrs = [
        vertex_attr(0, vk::Format::R32G32_SFLOAT, 0),
        vertex_attr(1, vk::Format::R32G32_SFLOAT, 8),
        vertex_attr(2, vk::Format::R32G32B32A32_SFLOAT, 16),
        vertex_attr(3, vk::Format::R32G32B32A32_SFLOAT, 32),
        vertex_attr(4, vk::Format::R32G32B32A32_SFLOAT, 48),
        vertex_attr(5, vk::Format::R32G32B32A32_SFLOAT, 64),
        vertex_attr(6, vk::Format::R32G32_SFLOAT, 80),
        vertex_attr(7, vk::Format::R32G32_SFLOAT, 88),
        vertex_attr(8, vk::Format::R32G32B32A32_SFLOAT, 96),
    ];

    let vertex_input = vk::PipelineVertexInputStateCreateInfo::default()
        .vertex_binding_descriptions(std::slice::from_ref(&binding))
        .vertex_attribute_descriptions(&attrs);

    let input_asm = vk::PipelineInputAssemblyStateCreateInfo::default()
        .topology(vk::PrimitiveTopology::TRIANGLE_LIST);

    let viewport_state = vk::PipelineViewportStateCreateInfo::default()
        .viewport_count(1)
        .scissor_count(1);

    let raster = vk::PipelineRasterizationStateCreateInfo::default()
        .polygon_mode(vk::PolygonMode::FILL)
        .line_width(1.0)
        .cull_mode(vk::CullModeFlags::NONE)
        .front_face(vk::FrontFace::COUNTER_CLOCKWISE);

    let multisample = vk::PipelineMultisampleStateCreateInfo::default()
        .rasterization_samples(vk::SampleCountFlags::TYPE_1);

    let depth_stencil = vk::PipelineDepthStencilStateCreateInfo::default()
        .depth_test_enable(false)
        .depth_write_enable(false);

    let color_attachment = vk::PipelineColorBlendAttachmentState::default()
        .blend_enable(true)
        .src_color_blend_factor(vk::BlendFactor::SRC_ALPHA)
        .dst_color_blend_factor(vk::BlendFactor::ONE_MINUS_SRC_ALPHA)
        .color_blend_op(vk::BlendOp::ADD)
        .src_alpha_blend_factor(vk::BlendFactor::ONE)
        .dst_alpha_blend_factor(vk::BlendFactor::ONE_MINUS_SRC_ALPHA)
        .alpha_blend_op(vk::BlendOp::ADD)
        .color_write_mask(vk::ColorComponentFlags::RGBA);

    let color_blend = vk::PipelineColorBlendStateCreateInfo::default()
        .attachments(std::slice::from_ref(&color_attachment));

    let dsl = d.create_descriptor_set_layout(
        &vk::DescriptorSetLayoutCreateInfo::default().bindings(&[vk::DescriptorSetLayoutBinding::default()
            .binding(0)
            .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
            .descriptor_count(1)
            .stage_flags(vk::ShaderStageFlags::FRAGMENT)]),
        None,
    )?;

    let pool = d.create_descriptor_pool(
        &vk::DescriptorPoolCreateInfo::default()
            .max_sets(1)
            .pool_sizes(&[vk::DescriptorPoolSize {
                ty: vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
                descriptor_count: 1,
            }]),
        None,
    )?;

    let pipeline_layout = d.create_pipeline_layout(
        &vk::PipelineLayoutCreateInfo::default()
            .set_layouts(std::slice::from_ref(&dsl))
            .push_constant_ranges(&[vk::PushConstantRange {
                stage_flags: vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT,
                offset: 0,
                size: std::mem::size_of::<VgPushConstants>() as u32,
            }]),
        None,
    )?;

    let pipeline = d
        .create_graphics_pipelines(
            vk::PipelineCache::null(),
            &[vk::GraphicsPipelineCreateInfo::default()
                .stages(&stage_info)
                .vertex_input_state(&vertex_input)
                .input_assembly_state(&input_asm)
                .viewport_state(&viewport_state)
                .rasterization_state(&raster)
                .multisample_state(&multisample)
                .depth_stencil_state(&depth_stencil)
                .color_blend_state(&color_blend)
                .layout(pipeline_layout)
                .render_pass(render_pass)
                .subpass(0)],
            None,
        )
        .map_err(|(_, e)| e)?
        .remove(0);

    d.destroy_shader_module(vert_mod, None);
    d.destroy_shader_module(frag_mod, None);

    let _ = instance;
    Ok((pipeline, pipeline_layout, dsl, pool))
}

fn vertex_attr(location: u32, format: vk::Format, offset: u32) -> vk::VertexInputAttributeDescription {
    vk::VertexInputAttributeDescription {
        location,
        binding: 0,
        format,
        offset,
    }
}

fn include_spirv(name: &str) -> anyhow::Result<Vec<u32>> {
    let dir = env!("WSARNDR_SHADERS");
    let path = std::path::Path::new(dir).join(format!("{}.spv", name));
    let bytes = std::fs::read(&path)?;
    let words: Vec<u32> = bytes
        .chunks_exact(4)
        .map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect();
    Ok(words)
}

impl Drop for VgContext {
    fn drop(&mut self) {
        unsafe {
            self.device.device.device_wait_idle().ok();
            self.device.device.destroy_buffer(self.vertex_buffer, None);
            self.device.device.free_memory(self.vertex_buffer_mem, None);
            self.device.device.destroy_descriptor_pool(self.descriptor_pool, None);
            self.device
                .device
                .destroy_descriptor_set_layout(self.descriptor_set_layout, None);
            self.device.device.destroy_pipeline(self.pipeline, None);
            self.device
                .device
                .destroy_pipeline_layout(self.pipeline_layout, None);
        }
    }
}