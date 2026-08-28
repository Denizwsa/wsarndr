pub mod font;
pub mod layout;
pub mod shape;


use std::collections::HashMap;

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
    pub clip: [f32; 4],
}

pub const FLAG_TEXTURE: f32 = 1.0;
pub const FLAG_STROKE: f32 = 2.0;
pub const FLAG_LINE: f32 = 4.0;
pub const FLAG_ELLIPSE: f32 = 8.0;
pub const FLAG_POLYGON: f32 = 16.0;
pub const FLAG_USER_TEX: f32 = 32.0;

#[repr(C)]
#[derive(Pod, Zeroable, Copy, Clone)]
pub struct VgPushConstants {
    pub viewport: [f32; 2],
    pub scale: f32,
    pub _pad: f32,
}

struct UserTexture {
    image: vk::Image,
    memory: vk::DeviceMemory,
    view: vk::ImageView,
    sampler: vk::Sampler,
    width: u32,
    height: u32,
}

#[derive(Clone)]
struct VgState {
    translate: [f32; 2],
    clip: Option<[f32; 4]>,
    global_alpha: f32,
    fill_color: Color,
    stroke_color: Color,
    stroke_width: f32,
    transform: [f32; 6],
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
    pub transform: [f32; 6],
    pub transform_stack: Vec<[f32; 2]>,
    pub state_stack: Vec<VgState>,
    pub clip_stack: Vec<[f32; 4]>,
    pub global_alpha: f32,
    pub fill_color: Color,
    pub stroke_color: Color,
    pub stroke_width: f32,
    pub path: Vec<[f32; 2]>,
    pub path_winding: i32,
    pub font_size: f32,
    pub text_align: TextAlign,
    pub line_cap: i32,
    pub line_join: i32,
    pub fonts: HashMap<String, FontAtlas>,
    pub current_font: String,
    user_texture: Option<UserTexture>,
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
        // Create 1x1 white dummy for user texture binding 1.
        let (u_img, u_mem, u_view, u_samp) = unsafe { create_dummy_texture(device.as_ref())? };
        let user_texture = UserTexture { image: u_img, memory: u_mem, view: u_view, sampler: u_samp, width: 1, height: 1 };
        let font_info = vk::DescriptorImageInfo::default()
            .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
            .image_view(font_atlas.image_view)
            .sampler(font_atlas.sampler);
        let user_info = vk::DescriptorImageInfo::default()
            .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
            .image_view(user_texture.view)
            .sampler(user_texture.sampler);
        let writes = [
            vk::WriteDescriptorSet::default()
                .dst_set(font_descriptor_set)
                .dst_binding(0)
                .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                .image_info(std::slice::from_ref(&font_info)),
            vk::WriteDescriptorSet::default()
                .dst_set(font_descriptor_set)
                .dst_binding(1)
                .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                .image_info(std::slice::from_ref(&user_info)),
        ];
        unsafe { device.device.update_descriptor_sets(&writes, &[]) };

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
            transform: [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
            transform_stack: Vec::new(),
            state_stack: Vec::new(),
            clip_stack: Vec::new(),
            global_alpha: 1.0,
            fill_color: Color::WHITE,
            stroke_color: Color::BLACK,
            stroke_width: 1.0,
            path: Vec::new(),
            path_winding: 1,
            font_size: 16.0,
            text_align: TextAlign::Left,
            line_cap: 0,
            line_join: 0,
            fonts: HashMap::new(),
            current_font: "default".to_string(),
            user_texture: Some(user_texture),
        })
    }

    pub fn begin_frame(&mut self, width: f32, height: f32, scale: f32) {
        self.viewport_size = [width, height];
        self.scale = scale;
        self.vertices.clear();
        self.needs_upload = false;
        self.translate = [0.0, 0.0];
        self.transform = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];
        self.transform_stack.clear();
        self.state_stack.clear();
        self.clip_stack.clear();
        self.path.clear();
        self.global_alpha = 1.0;
    }

    // ---- NanoVG compat: state stack ----
    pub fn save(&mut self) {
        self.state_stack.push(VgState {
            translate: self.translate,
            clip: self.clip_stack.last().copied(),
            global_alpha: self.global_alpha,
            fill_color: self.fill_color,
            stroke_color: self.stroke_color,
            stroke_width: self.stroke_width,
            transform: self.transform,
        });
        self.transform_stack.push(self.translate);
    }
    pub fn restore(&mut self) {
        if let Some(st) = self.state_stack.pop() {
            self.translate = st.translate;
            self.global_alpha = st.global_alpha;
            self.fill_color = st.fill_color;
            self.stroke_color = st.stroke_color;
            self.stroke_width = st.stroke_width;
            self.transform = st.transform;
            if let Some(c) = st.clip { if self.clip_stack.is_empty() || self.clip_stack.last() != Some(&c) { self.clip_stack.push(c); } }
        }
        if let Some(t) = self.transform_stack.pop() { self.translate = t; }
    }
    pub fn reset(&mut self) {
        self.transform = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];
        self.translate = [0.0, 0.0];
        self.global_alpha = 1.0;
        self.clip_stack.clear();
        self.path.clear();
        self.state_stack.clear();
        self.transform_stack.clear();
    }
    // ---- Transforms ----
    pub fn translate(&mut self, tx: f32, ty: f32) { self.transform[4] += tx; self.transform[5] += ty; }
    pub fn rotate(&mut self, angle_deg: f32) {
        let rad = angle_deg.to_radians();
        let (s, c) = rad.sin_cos();
        let [a,b,cc,d,tx,ty] = self.transform;
        self.transform = [a*c + cc*s, b*c + d*s, a*(-s) + cc*c, b*(-s) + d*c, tx, ty];
    }
    pub fn scale(&mut self, sx: f32, sy: f32) {
        self.transform[0] *= sx; self.transform[1] *= sx; self.transform[2] *= sy; self.transform[3] *= sy;
    }
    pub fn skew_x(&mut self, angle_deg: f32) { let t = angle_deg.to_radians().tan(); self.transform[2] += self.transform[0] * t; self.transform[3] += self.transform[1] * t; }
    pub fn skew_y(&mut self, angle_deg: f32) { let t = angle_deg.to_radians().tan(); self.transform[0] += self.transform[2] * t; self.transform[1] += self.transform[3] * t; }
    pub fn set_transform(&mut self, a: f32, b: f32, c: f32, d: f32, e: f32, f: f32) { self.transform = [a,b,c,d,e,f]; }
    pub fn reset_transform(&mut self) { self.transform = [1.0,0.0,0.0,1.0,0.0,0.0]; self.translate = [0.0,0.0]; }
    pub fn global_alpha(&mut self, a: f32) { self.global_alpha = a.clamp(0.0,1.0); }
    pub fn stroke_width(&mut self, w: f32) { self.stroke_width = w; }
    pub fn stroke_color(&mut self, c: impl Into<Color>) { self.stroke_color = c.into(); }
    pub fn fill_color(&mut self, c: impl Into<Color>) { self.fill_color = c.into(); }
    pub fn line_cap(&mut self, cap: i32) { self.line_cap = cap; }
    pub fn line_join(&mut self, join: i32) { self.line_join = join; }
    // ---- Scissor ----
    pub fn scissor(&mut self, x: f32, y: f32, w: f32, h: f32) { self.push_clip(x,y,w,h); }
    pub fn intersect_scissor(&mut self, x: f32, y: f32, w: f32, h: f32) { self.push_clip(x,y,w,h); }
    pub fn reset_scissor(&mut self) { self.clip_stack.clear(); }
    // ---- Path API (NanoVG compat) ----
    pub fn begin_path(&mut self) { self.path.clear(); }
    pub fn move_to(&mut self, x: f32, y: f32) { let (x,y)=self.apply(x,y); self.path.push([x,y]); }
    pub fn line_to(&mut self, x: f32, y: f32) { let (x,y)=self.apply(x,y); self.path.push([x,y]); }
    pub fn bezier_to(&mut self, c1x: f32, c1y: f32, c2x: f32, c2y: f32, x: f32, y: f32) {
        if self.path.is_empty() { return; }
        let p0 = *self.path.last().unwrap();
        let (c1x,c1y)=self.apply(c1x,c1y); let (c2x,c2y)=self.apply(c2x,c2y); let (x,y)=self.apply(x,y);
        for i in 1..=10 { let t=i as f32/10.0; let u=1.0-t; let bx = u*u*u*p0[0] + 3.0*u*u*t*c1x + 3.0*u*t*t*c2x + t*t*t*x; let by = u*u*u*p0[1] + 3.0*u*u*t*c1y + 3.0*u*t*t*c2y + t*t*t*y; self.path.push([bx,by]); }
    }
    pub fn quad_to(&mut self, cx: f32, cy: f32, x: f32, y: f32) {
        if self.path.is_empty() { return; }
        let p0 = *self.path.last().unwrap();
        let (cx,cy)=self.apply(cx,cy); let (x,y)=self.apply(x,y);
        for i in 1..=8 { let t=i as f32/8.0; let u=1.0-t; let bx = u*u*p0[0] + 2.0*u*t*cx + t*t*x; let by = u*u*p0[1] + 2.0*u*t*cy + t*t*y; self.path.push([bx,by]); }
    }
    pub fn arc(&mut self, cx: f32, cy: f32, r: f32, a0_deg: f32, a1_deg: f32) {
        let (cx,cy)=self.apply(cx,cy);
        let a0=a0_deg.to_radians(); let a1=a1_deg.to_radians(); let sweep=a1-a0; let segs=((sweep.abs()/15.0).ceil() as usize).max(8);
        for i in 0..=segs { let a=a0 + sweep*(i as f32/segs as f32); self.path.push([cx + r*a.cos(), cy + r*a.sin()]); }
    }
    pub fn arc_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, r: f32) { self.line_to(x1,y1); let _=r; self.line_to(x2,y2); }
    pub fn close_path(&mut self) { if let Some(&first)=self.path.first() { if self.path.last()!=Some(&first) { self.path.push(first); } } }
    pub fn path_winding(&mut self, dir: i32) { self.path_winding = dir; }
    pub fn fill(&mut self) {
        if self.path.len() < 3 { return; }
        let col = self.fill_color.with_alpha(self.fill_color.a * self.global_alpha);
        // Triangulate via fan from first point (convex approx; for concave use ear clipping but fan is fast)
        let p0 = self.path[0];
        for i in 1..self.path.len()-1 {
            let p1=self.path[i]; let p2=self.path[i+1];
            self.draw_shape(&crate::vg::shape::Shape { kind: crate::vg::shape::ShapeKind::Triangle { x2: p1[0], y2: p1[1], x3: p2[0], y3: p2[1] }, x: p0[0], y: p0[1], w: (p1[0]-p0[0]).abs().max(1.0), h: (p1[1]-p0[1]).abs().max(1.0), fill_color: col, stroke_width: 0.0, _line_n: [p0[0],p0[1]], ..Default::default() });
            // Fallback: also draw triangle via direct vertices for concave safety
            let _ = (p0,p1,p2);
        }
        // For proper fill, also add as polygon via existing path triangulation would be better, but fan covers most UI rects
        // Instead, push as series of triangles using helper
        let n=self.path.len();
        if n>=3 {
            let cx = self.path.iter().map(|p| p[0]).sum::<f32>()/n as f32;
            let cy = self.path.iter().map(|p| p[1]).sum::<f32>()/n as f32;
            for i in 0..n { let a=self.path[i]; let b=self.path[(i+1)%n]; self.triangle_fill(a[0],a[1],b[0],b[1],cx,cy,col); }
        }
        self.path.clear();
    }
    pub fn stroke(&mut self) {
        if self.path.len() < 2 { return; }
        let col = self.stroke_color.with_alpha(self.stroke_color.a * self.global_alpha);
        let w = self.stroke_width.max(1.0);
        for i in 0..self.path.len()-1 { let a=self.path[i]; let b=self.path[i+1]; self.line(a[0],a[1],b[0],b[1],w,col); }
        self.path.clear();
    }
    pub fn rounded_rect_varying(&mut self, x: f32, y: f32, w: f32, h: f32, r1: f32, r2: f32, r3: f32, r4: f32, color: impl Into<Color>) {
        let r=(r1+r2+r3+r4)/4.0; self.rounded_rect_fill(x,y,w,h,r,color);
    }
    pub fn box_gradient(&mut self, x: f32, y: f32, w: f32, h: f32, r: f32, f: f32, c0: Color, c1: Color) {
        self.shadow_rounded_rect(x,y,w,h,r,f,[0.0,0.0],c0);
        let _=c1;
    }
    pub fn image_pattern(&mut self, x: f32, y: f32, w: f32, h: f32, angle: f32, alpha: f32) { let _=(x,y,w,h,angle,alpha); }
    // ---- Font ----
    pub fn font_face(&mut self, name: &str) {
        if self.fonts.contains_key(name) {
            let new_atlas = self.fonts.remove(name).unwrap();
            let old_name = self.current_font.clone();
            let old_atlas = std::mem::replace(&mut self.font_atlas, new_atlas);
            self.fonts.insert(old_name, old_atlas);
            self.current_font = name.to_string();
        }
    }
    pub fn font_size(&mut self, size: f32) { self.font_size = size; }
    pub fn text_bounds(&self, x: f32, y: f32, text: &str, size: f32) -> [f32;4] {
        let w=self.text_width(text,size); let h=size; [x, y-h, x+w, y]
    }
    pub fn text_metrics(&self, size: f32) -> (f32,f32,f32) {
        let asc=self.font_atlas.ascent * (size/self.font_atlas.font_size_px);
        let desc=size - asc;
        (asc,desc,size)
    }
    pub fn create_font(&mut self, name: &str, ttf: &[u8]) -> i32 {
        match FontAtlas::load(self.device.clone(), ttf, self.font_size) {
            Ok(atlas) => { self.fonts.insert(name.to_string(), atlas); 0 },
            Err(_) => -1,
        }
    }
    pub fn create_font_mem(&mut self, name: &str, ttf: &[u8]) -> i32 { self.create_font(name, ttf) }
    fn glyph_info_fallback(&self, code: u32) -> Option<crate::vg::font::GlyphInfo> {
        if let Some(gi) = self.font_atlas.glyph_info(code) { return Some(gi); }
        for atlas in self.fonts.values() {
            if let Some(gi) = atlas.glyph_info(code) { return Some(gi); }
        }
        None
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
            ShapeKind::Ellipse => (
                half.max(0.001),
                shape.stroke_width,
                FLAG_ELLIPSE + if shape.stroke_width > 0.0 { FLAG_STROKE } else { 0.0 },
            ),
            ShapeKind::Polygon { sides } => (
                sides as f32,
                shape.stroke_width,
                FLAG_POLYGON + if shape.stroke_width > 0.0 { FLAG_STROKE } else { 0.0 },
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
                    clip: self.current_clip(),
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
        let is_small = size < 13.0;
        for ch in text.chars() {
            if let Some(gi) = self.glyph_info_fallback(ch as u32) {
                let gw = gi.width * scale;
                let gh = gi.height * scale;
                let uv = [gi.u0, gi.v0, gi.u1, gi.v1];
                // Uniform cell-centered glyphs: off_x/off_y are centering offsets within cell.
                let gx = (pen_x + gi.x_offset * scale).round();
                let gy = (y + gi.y_offset * scale).round();
                if gw > 0.0 && gh > 0.0 {
                    // For very small text, faux-bold by drawing a second copy offset by 0.5px
                    if is_small {
                        let bold = Shape {
                            kind: ShapeKind::Text,
                            x: gx + 0.7,
                            y: gy,
                            w: gw,
                            h: gh,
                            fill_color: color.with_alpha(color.a * 0.55),
                            stroke_width: 0.0,
                            uv_override: Some(uv),
                            ..Default::default()
                        };
                        self.draw_shape(&bold);
                    }
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
            if let Some(gi) = self.glyph_info_fallback(ch as u32) {
                w += gi.advance * scale;
            }
        }
        w
    }

    /// Returns (width, height) for the given text at the specified size.
    pub fn measure_text(&self, text: &str, size: f32) -> (f32, f32) {
        let scale = size / self.font_atlas.font_size_px;
        let cell_h = self.glyph_info_fallback(b'X' as u32)
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

    /// Apply current transform to coordinates (affine + translate).
    fn apply(&self, x: f32, y: f32) -> (f32, f32) {
        let a = self.transform[0]; let b = self.transform[1]; let c = self.transform[2]; let d = self.transform[3]; let tx = self.transform[4]; let ty = self.transform[5];
        let x1 = a * x + c * y + tx;
        let y1 = b * x + d * y + ty;
        (x1 + self.translate[0], y1 + self.translate[1])
    }

    // ---- Clip stack ----

    /// Push a clip rectangle. All subsequent draws are clipped to this rect (intersected with previous clip).
    pub fn push_clip(&mut self, x: f32, y: f32, w: f32, h: f32) {
        let (x, y) = self.apply(x, y);
        let rect = if let Some(&cur) = self.clip_stack.last() {
            if cur[2] < 0.0 {
                [x, y, w, h]
            } else {
                let x0 = x.max(cur[0]);
                let y0 = y.max(cur[1]);
                let x1 = (x + w).min(cur[0] + cur[2]);
                let y1 = (y + h).min(cur[1] + cur[3]);
                [x0, y0, (x1 - x0).max(0.0), (y1 - y0).max(0.0)]
            }
        } else {
            [x, y, w, h]
        };
        self.clip_stack.push(rect);
    }

    pub fn pop_clip(&mut self) {
        self.clip_stack.pop();
    }

    fn current_clip(&self) -> [f32; 4] {
        self.clip_stack.last().copied().unwrap_or([-1.0, -1.0, -1.0, -1.0])
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

    // ---- Ellipse ----

    pub fn ellipse_fill(&mut self, cx: f32, cy: f32, rx: f32, ry: f32, color: impl Into<Color>) {
        let (cx, cy) = self.apply(cx, cy);
        self.draw_shape(&Shape {
            kind: ShapeKind::Ellipse,
            x: cx - rx,
            y: cy - ry,
            w: rx * 2.0,
            h: ry * 2.0,
            fill_color: color.into(),
            stroke_width: 0.0,
            ..Default::default()
        });
    }

    pub fn ellipse_stroke(&mut self, cx: f32, cy: f32, rx: f32, ry: f32, width: f32, color: impl Into<Color>) {
        let (cx, cy) = self.apply(cx, cy);
        self.draw_shape(&Shape {
            kind: ShapeKind::Ellipse,
            x: cx - rx,
            y: cy - ry,
            w: rx * 2.0,
            h: ry * 2.0,
            fill_color: color.into(),
            stroke_width: width,
            ..Default::default()
        });
    }

    // ---- Regular polygon ----

    pub fn polygon_fill(&mut self, cx: f32, cy: f32, radius: f32, sides: u32, rotation_deg: f32, color: impl Into<Color>) {
        let (cx, cy) = self.apply(cx, cy);
        self.draw_shape(&Shape {
            kind: ShapeKind::Polygon { sides: sides.max(3) },
            x: cx - radius,
            y: cy - radius,
            w: radius * 2.0,
            h: radius * 2.0,
            fill_color: color.into(),
            stroke_width: 0.0,
            grad_inner_radius: rotation_deg.to_radians(),
            ..Default::default()
        });
    }

    // ---- Shadow helper ----

    /// Draw a soft shadow for a rounded rect (offset + blur).
    pub fn shadow_rounded_rect(&mut self, x: f32, y: f32, w: f32, h: f32, r: f32, blur: f32, offset: [f32; 2], color: impl Into<Color>) {
        let base: Color = color.into();
        let c = base.with_alpha(base.a * 0.35);
        let b = blur;
        self.rounded_rect_fill(x + offset[0] - b, y + offset[1] - b, w + b * 2.0, h + b * 2.0, r + b, c);
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

    /// Reload font from TTF bytes. Allows modders to use custom fonts at runtime.
    pub fn set_font(&mut self, ttf: &[u8], size_px: f32) -> anyhow::Result<()> {
        let new_atlas = FontAtlas::load(self.device.clone(), ttf, size_px)?;
        unsafe { self.device.device.device_wait_idle()? };
        let image_info = vk::DescriptorImageInfo::default()
            .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
            .image_view(new_atlas.image_view)
            .sampler(new_atlas.sampler);
        let write = vk::WriteDescriptorSet::default()
            .dst_set(self.font_descriptor_set)
            .dst_binding(0)
            .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
            .image_info(std::slice::from_ref(&image_info));
        unsafe { self.device.device.update_descriptor_sets(std::slice::from_ref(&write), &[]) };
        self.font_atlas = new_atlas;
        Ok(())
    }

    /// Load a user image (PNG/JPEG bytes) into the GPU texture slot.
    /// Modders can call this once and then use `draw_image` / `draw_rounded_image`.
    pub fn set_image_from_bytes(&mut self, bytes: &[u8]) -> anyhow::Result<()> {
        let img = image::load_from_memory(bytes).map_err(|e| anyhow::anyhow!("image decode: {}", e))?;
        let rgba = img.to_rgba8();
        let (w, h) = (rgba.width(), rgba.height());
        let (nimg, nmem, nview, nsamp) = unsafe { create_texture_from_rgba(self.device.as_ref(), w, h, &rgba)? };
        unsafe { self.device.device.device_wait_idle()? };
        // Destroy old user texture if exists
        if let Some(old) = self.user_texture.take() {
            unsafe {
                self.device.device.destroy_image_view(old.view, None);
                self.device.device.destroy_sampler(old.sampler, None);
                self.device.device.destroy_image(old.image, None);
                self.device.device.free_memory(old.memory, None);
            }
        }
        let info = vk::DescriptorImageInfo::default()
            .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
            .image_view(nview)
            .sampler(nsamp);
        let write = vk::WriteDescriptorSet::default()
            .dst_set(self.font_descriptor_set)
            .dst_binding(1)
            .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
            .image_info(std::slice::from_ref(&info));
        unsafe { self.device.device.update_descriptor_sets(std::slice::from_ref(&write), &[]) };
        self.user_texture = Some(UserTexture { image: nimg, memory: nmem, view: nview, sampler: nsamp, width: w, height: h });
        Ok(())
    }

    /// Draw the currently bound user image.
    pub fn draw_image(&mut self, x: f32, y: f32, w: f32, h: f32, tint: impl Into<Color>) {
        let (x, y) = self.apply(x, y);
        self.draw_shape(&Shape {
            kind: ShapeKind::Rect,
            x,
            y,
            w,
            h,
            fill_color: tint.into(),
            stroke_width: 0.0,
            uv_override: Some([0.0, 0.0, 1.0, 1.0]),
            ..Default::default()
        });
        // Patch last 6 vertices to use user texture flag
        let n = self.vertices.len();
        for v in &mut self.vertices[n.saturating_sub(6)..] {
            v.param[3] += FLAG_USER_TEX;
        }
    }

    pub fn draw_rounded_image(&mut self, x: f32, y: f32, w: f32, h: f32, r: f32, tint: impl Into<Color>) {
        let (x, y) = self.apply(x, y);
        self.draw_shape(&Shape {
            kind: ShapeKind::RoundedRect { radius: r },
            x,
            y,
            w,
            h,
            fill_color: tint.into(),
            stroke_width: 0.0,
            uv_override: Some([0.0, 0.0, 1.0, 1.0]),
            ..Default::default()
        });
        let n = self.vertices.len();
        for v in &mut self.vertices[n.saturating_sub(6)..] {
            v.param[3] += FLAG_USER_TEX;
        }
    }

    // ---- Per-corner rounded rect (extensible) ----
    pub fn rounded_rect_corners(&mut self, x: f32, y: f32, w: f32, h: f32, radii: [f32; 4], color: impl Into<Color>) {
        // For now uniform fallback using max radius; shader can be extended to handle 4 radii via grad params.
        let r = radii[0].max(radii[1]).max(radii[2]).max(radii[3]);
        self.rounded_rect_fill(x, y, w, h, r, color);
    }

    // ---- Text effects ----
    pub fn text_with_shadow(&mut self, x: f32, y: f32, text: &str, size: f32, color: impl Into<Color>, shadow: impl Into<Color>, offset: [f32; 2]) {
        let c: Color = color.into();
        let s: Color = shadow.into();
        self.text(x + offset[0], y + offset[1], text, size, s);
        self.text(x, y, text, size, c);
    }

    pub fn text_with_outline(&mut self, x: f32, y: f32, text: &str, size: f32, color: impl Into<Color>, outline: impl Into<Color>, width: f32) {
        let c: Color = color.into();
        let o: Color = outline.into();
        // Simple outline by drawing 4 offsets
        for (dx, dy) in [(-width, 0.0), (width, 0.0), (0.0, -width), (0.0, width)] {
            self.text(x + dx, y + dy, text, size, o);
        }
        self.text(x, y, text, size, c);
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

unsafe fn create_dummy_texture(device: &Device) -> anyhow::Result<(vk::Image, vk::DeviceMemory, vk::ImageView, vk::Sampler)> {
    create_texture_from_rgba(device, 1, 1, &[255, 255, 255, 255])
}

unsafe fn create_texture_from_rgba(device: &Device, w: u32, h: u32, rgba: &[u8]) -> anyhow::Result<(vk::Image, vk::DeviceMemory, vk::ImageView, vk::Sampler)> {
    let pool = device.device.create_command_pool(
        &vk::CommandPoolCreateInfo::default().queue_family_index(device.queue_family_index),
        None,
    )?;
    let (staging, staging_mem) = device.create_buffer(
        (w * h * 4) as vk::DeviceSize,
        vk::BufferUsageFlags::TRANSFER_SRC,
        vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT,
    )?;
    {
        let ptr = device.device.map_memory(staging_mem, 0, (w * h * 4) as vk::DeviceSize, vk::MemoryMapFlags::empty())?;
        std::ptr::copy_nonoverlapping(rgba.as_ptr(), ptr as *mut u8, (w * h * 4) as usize);
        device.device.unmap_memory(staging_mem);
    }
    let (image, image_mem) = device.create_image(
        w,
        h,
        vk::Format::R8G8B8A8_SRGB,
        vk::ImageUsageFlags::TRANSFER_DST | vk::ImageUsageFlags::SAMPLED,
        vk::MemoryPropertyFlags::DEVICE_LOCAL,
    )?;
    device.single_time_command(pool, |cmd| {
        device.transition_image_layout(cmd, image, vk::ImageLayout::UNDEFINED, vk::ImageLayout::TRANSFER_DST_OPTIMAL).ok();
        device.copy_buffer_to_image(cmd, staging, image, w, h);
        device.transition_image_layout(cmd, image, vk::ImageLayout::TRANSFER_DST_OPTIMAL, vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL).ok();
    })?;
    let view = device.device.create_image_view(
        &vk::ImageViewCreateInfo::default()
            .image(image)
            .view_type(vk::ImageViewType::TYPE_2D)
            .format(vk::Format::R8G8B8A8_SRGB)
            .subresource_range(vk::ImageSubresourceRange::default().aspect_mask(vk::ImageAspectFlags::COLOR).level_count(1).layer_count(1)),
        None,
    )?;
    let sampler = device.device.create_sampler(
        &vk::SamplerCreateInfo::default()
            .mag_filter(vk::Filter::LINEAR)
            .min_filter(vk::Filter::LINEAR)
            .mipmap_mode(vk::SamplerMipmapMode::NEAREST)
            .address_mode_u(vk::SamplerAddressMode::CLAMP_TO_EDGE)
            .address_mode_v(vk::SamplerAddressMode::CLAMP_TO_EDGE)
            .address_mode_w(vk::SamplerAddressMode::CLAMP_TO_EDGE),
        None,
    )?;
    device.device.destroy_command_pool(pool, None);
    device.device.destroy_buffer(staging, None);
    device.device.free_memory(staging_mem, None);
    Ok((image, image_mem, view, sampler))
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
        vertex_attr(9, vk::Format::R32G32B32A32_SFLOAT, 112),
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
        &vk::DescriptorSetLayoutCreateInfo::default().bindings(&[
            vk::DescriptorSetLayoutBinding::default()
                .binding(0)
                .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::FRAGMENT),
            vk::DescriptorSetLayoutBinding::default()
                .binding(1)
                .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::FRAGMENT),
        ]),
        None,
    )?;

    let pool = d.create_descriptor_pool(
        &vk::DescriptorPoolCreateInfo::default()
            .max_sets(1)
            .pool_sizes(&[vk::DescriptorPoolSize {
                ty: vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
                descriptor_count: 2,
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
            if let Some(ut) = self.user_texture.take() {
                self.device.device.destroy_image_view(ut.view, None);
                self.device.device.destroy_sampler(ut.sampler, None);
                self.device.device.destroy_image(ut.image, None);
                self.device.device.free_memory(ut.memory, None);
            }
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