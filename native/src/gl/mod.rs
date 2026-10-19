use glow::HasContext;
use crate::vg::font::FontAtlas as VkFontAtlas;
use crate::vg::shape::{Color, TextAlign, Shape, ShapeKind, GradMode};
use crate::vg::{VgPushConstants, VgVertex, FLAG_TEXTURE, FLAG_STROKE, FLAG_LINE, FLAG_ELLIPSE, FLAG_POLYGON, FLAG_USER_TEX};

use anyhow::Result;

// GL 3.3 shaders — ported from Vulkan #version 450
const VERT_SRC: &str = r#"#version 330 core
layout(location = 0) in vec2 inPos;
layout(location = 1) in vec2 inUV;
layout(location = 2) in vec4 inColor;
layout(location = 3) in vec4 inParam;
layout(location = 4) in vec4 inBounds;
layout(location = 5) in vec4 inGradColor;
layout(location = 6) in vec2 inGradFrom;
layout(location = 7) in vec2 inGradTo;
layout(location = 8) in vec4 inGradParams;
layout(location = 9) in vec4 inClip;
uniform vec2 uViewport;
out vec2 fragUV;
out vec4 fragColor;
out vec4 fragParam;
out vec4 fragBounds;
out vec4 fragGradColor;
out vec2 fragGradFrom;
out vec2 fragGradTo;
out vec4 fragGradParams;
out vec4 fragClip;
void main() {
    gl_Position = vec4(inPos, 0.0, 1.0);
    fragUV = inUV;
    fragColor = inColor;
    fragParam = inParam;
    fragBounds = inBounds;
    fragGradColor = inGradColor;
    fragGradFrom = inGradFrom;
    fragGradTo = inGradTo;
    fragGradParams = inGradParams;
    fragClip = inClip;
}
"#;

const FRAG_SRC: &str = r#"#version 330 core
in vec2 fragUV;
in vec4 fragColor;
in vec4 fragParam;
in vec4 fragBounds;
in vec4 fragGradColor;
in vec2 fragGradFrom;
in vec2 fragGradTo;
in vec4 fragGradParams;
in vec4 fragClip;
uniform sampler2D uTexture;
uniform sampler2D uUserTex;
out vec4 outColor;
const float FLAG_TEXTURE = 1.0;
const float FLAG_STROKE = 2.0;
const float FLAG_USER_TEX = 32.0;
float sdRoundRect(vec2 p, vec2 b, float r) {
    vec2 q = abs(p) - b + vec2(r);
    return min(max(q.x, q.y), 0.0) + length(max(q, 0.0)) - r;
}
float sdEllipse(vec2 p, vec2 ab) {
    vec2 q = p / ab;
    return (length(q) - 1.0) * min(ab.x, ab.y);
}
void main() {
    vec2 pos = vec2(gl_FragCoord.x, gl_FragCoord.y);
    // Viewport uniform not needed — pos already in pixels via gl_FragCoord
    if (fragClip.z >= 0.0) {
        if (pos.x < fragClip.x || pos.y < fragClip.y ||
            pos.x > fragClip.x + fragClip.z || pos.y > fragClip.y + fragClip.w) discard;
    }
    vec4 color = fragColor;
    bool isText = mod(fragParam.w, 2.0) >= 1.0;
    if (isText) {
        float a = texture(uTexture, fragUV).r;
        outColor = vec4(color.rgb, color.a * a);
        return;
    }
    vec2 boundsMin = fragBounds.xy;
    vec2 boundsMax = fragBounds.zw;
    vec2 center = (boundsMin + boundsMax) * 0.5;
    vec2 halfSize = (boundsMax - boundsMin) * 0.5;
    float radius = fragParam.z;
    float feather = max(fragParam.y, 0.5);
    float stroke = fragParam.x;
    float flags = fragParam.w;
    float d = sdRoundRect(pos - center, halfSize, radius);
    float alpha;
    float od = d;
    if (stroke <= 0.0) {
        alpha = clamp(0.5 - d / feather, 0.0, 1.0);
    } else {
        float id = stroke * 0.5 - abs(d);
        alpha = clamp(0.5 - id / feather, 0.0, 1.0);
    }
    if (od > feather) alpha = 0.0;
    bool isUserTex = mod(flags, 64.0) >= 32.0;
    if (isUserTex) {
        vec4 tex = texture(uUserTex, fragUV);
        outColor = vec4(tex.rgb * color.rgb, tex.a * color.a * alpha);
        return;
    }
    int mode = int(fragGradParams.x);
    if (mode == 1) {
        vec2 dG = fragGradTo - fragGradFrom;
        float len = dot(dG, dG);
        float t = len > 0.0 ? clamp(dot(pos - fragGradFrom, dG) / len, 0.0, 1.0) : 0.0;
        color = mix(color, fragGradColor, t);
    } else if (mode == 2) {
        vec2 p = pos - fragGradFrom;
        float r0 = fragGradParams.y;
        float r1 = length(fragGradTo - fragGradFrom);
        float dist = clamp((length(p) - r0) / max(r1 - r0, 1e-6), 0.0, 1.0);
        color = mix(color, fragGradColor, dist);
    }
    outColor = vec4(color.rgb, color.a * alpha);
}
"#;

pub struct GlRenderer {
    gl: glow::Context,
    program: glow::NativeProgram,
    vao: glow::NativeVertexArray,
    vbo: glow::NativeBuffer,
    font_texture: glow::NativeTexture,
    user_texture: Option<glow::NativeTexture>,
    vertices: Vec<VgVertex>,
    viewport: [f32; 2],
    scale: f32,
    translate: [f32; 2],
    transform_stack: Vec<[f32; 2]>,
    clip_stack: Vec<[f32; 4]>,
    // Uniform locations
    u_viewport: Option<glow::UniformLocation>,
    u_texture: Option<glow::UniformLocation>,
    u_user_tex: Option<glow::UniformLocation>,
    // Font atlas CPU side for glyph lookup (reuse fontdue logic)
    font_atlas: FontAtlasCpu,
}

// CPU-side font atlas for GL (simpler than Vulkan's GPU atlas)
struct FontAtlasCpu {
    texture: glow::NativeTexture,
    glyphs: Vec<GlyphInfo>,
    font_size_px: f32,
}

#[derive(Clone, Copy)]
struct GlyphInfo {
    u0: f32, v0: f32, u1: f32, v1: f32,
    width: f32, height: f32,
    x_offset: f32, y_offset: f32,
    advance: f32,
    cell_w: f32, cell_h: f32,
}

impl GlRenderer {
    pub fn new<F>(loader: F) -> Result<Self>
    where
        F: FnMut(&str) -> *const std::ffi::c_void,
    {
        let gl = unsafe { glow::Context::from_loader_function(loader) };

        // Compile shaders
        let program = unsafe { create_program(&gl, VERT_SRC, FRAG_SRC)? };

        let vao = unsafe { gl.create_vertex_array().map_err(|e| anyhow::anyhow!(e))? };
        let vbo = unsafe { gl.create_buffer().map_err(|e| anyhow::anyhow!(e))? };

        // Create 1x1 white dummy for user texture
        let user_texture = None;

        // Font atlas — generate CPU texture then upload via GL
        let (font_texture, glyphs) = unsafe { create_font_texture(&gl)? };

        let u_viewport = unsafe { gl.get_uniform_location(program, "uViewport") };
        // Samplers are bound to texture units 0 and 1, set once
        unsafe {
            gl.use_program(Some(program));
            if let Some(loc) = gl.get_uniform_location(program, "uTexture") {
                gl.uniform_1_i32(Some(&loc), 0);
            }
            if let Some(loc) = gl.get_uniform_location(program, "uUserTex") {
                gl.uniform_1_i32(Some(&loc), 1);
            }
            gl.use_program(None);
        }

        Ok(Self {
            gl,
            program,
            vao,
            vbo,
            font_texture,
            user_texture,
            vertices: Vec::new(),
            viewport: [800.0, 600.0],
            scale: 1.0,
            translate: [0.0, 0.0],
            transform_stack: Vec::new(),
            clip_stack: Vec::new(),
            u_viewport,
            u_texture: None,
            u_user_tex: None,
            font_atlas: FontAtlasCpu { texture: font_texture, glyphs, font_size_px: 48.0 },
        })
    }

    pub fn begin_frame(&mut self, w: f32, h: f32, scale: f32) {
        self.viewport = [w, h];
        self.scale = scale;
        self.vertices.clear();
        self.translate = [0.0, 0.0];
        self.transform_stack.clear();
        self.clip_stack.clear();
    }

    pub fn resize(&mut self, _w: u32, _h: u32) -> Result<()> {
        Ok(())
    }

    pub fn set_clear_color(&mut self, _c: [f32; 4]) {}

    pub fn render(&mut self) -> Result<()> {
        if self.vertices.is_empty() {
            return Ok(());
        }

        unsafe {
            let gl = &self.gl;

            // Save MC state that we will clobber — caller should have GL context current
            // Enable blending
            gl.enable(glow::BLEND);
            gl.blend_func(glow::SRC_ALPHA, glow::ONE_MINUS_SRC_ALPHA);
            gl.disable(glow::DEPTH_TEST);
            gl.disable(glow::CULL_FACE);
            gl.disable(glow::SCISSOR_TEST);

            // Upload vertices
            gl.bind_vertex_array(Some(self.vao));
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(self.vbo));
            let data = bytemuck::cast_slice(&self.vertices);
            gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, data, glow::STATIC_DRAW);

            // Setup vertex attributes (must match VgVertex layout)
            let stride = std::mem::size_of::<VgVertex>() as i32;
            // pos: location 0, offset 0
            gl.enable_vertex_attrib_array(0);
            gl.vertex_attrib_pointer_f32(0, 2, glow::FLOAT, false, stride, 0);
            // uv: location 1, offset 8
            gl.enable_vertex_attrib_array(1);
            gl.vertex_attrib_pointer_f32(1, 2, glow::FLOAT, false, stride, 8);
            // color: location 2, offset 16
            gl.enable_vertex_attrib_array(2);
            gl.vertex_attrib_pointer_f32(2, 4, glow::FLOAT, false, stride, 16);
            // param: location 3, offset 32
            gl.enable_vertex_attrib_array(3);
            gl.vertex_attrib_pointer_f32(3, 4, glow::FLOAT, false, stride, 32);
            // bounds: location 4, offset 48
            gl.enable_vertex_attrib_array(4);
            gl.vertex_attrib_pointer_f32(4, 4, glow::FLOAT, false, stride, 48);
            // grad_color: location 5, offset 64
            gl.enable_vertex_attrib_array(5);
            gl.vertex_attrib_pointer_f32(5, 4, glow::FLOAT, false, stride, 64);
            // grad_from: location 6, offset 80
            gl.enable_vertex_attrib_array(6);
            gl.vertex_attrib_pointer_f32(6, 2, glow::FLOAT, false, stride, 80);
            // grad_to: location 7, offset 88
            gl.enable_vertex_attrib_array(7);
            gl.vertex_attrib_pointer_f32(7, 2, glow::FLOAT, false, stride, 88);
            // grad_params: location 8, offset 96
            gl.enable_vertex_attrib_array(8);
            gl.vertex_attrib_pointer_f32(8, 4, glow::FLOAT, false, stride, 96);
            // clip: location 9, offset 112
            gl.enable_vertex_attrib_array(9);
            gl.vertex_attrib_pointer_f32(9, 4, glow::FLOAT, false, stride, 112);

            gl.use_program(Some(self.program));
            if let Some(loc) = &self.u_viewport {
                gl.uniform_2_f32(Some(loc), self.viewport[0], self.viewport[1]);
            }

            gl.active_texture(glow::TEXTURE0);
            gl.bind_texture(glow::TEXTURE_2D, Some(self.font_texture));
            gl.active_texture(glow::TEXTURE1);
            if let Some(tex) = self.user_texture {
                gl.bind_texture(glow::TEXTURE_2D, Some(tex));
            } else {
                // Bind dummy 1x1 white
                gl.bind_texture(glow::TEXTURE_2D, None);
            }

            // Viewport and scissor for MC's framebuffer size
            gl.viewport(0, 0, self.viewport[0] as i32, self.viewport[1] as i32);

            gl.draw_arrays(glow::TRIANGLES, 0, self.vertices.len() as i32);

            // Cleanup
            gl.bind_vertex_array(None);
            gl.bind_buffer(glow::ARRAY_BUFFER, None);
            gl.use_program(None);
            gl.disable(glow::BLEND);
        }

        self.vertices.clear();
        Ok(())
    }

    // ---- queue helpers (mirror Vulkan Renderer API) ----
    pub fn clear_queue(&mut self) {
        self.vertices.clear();
    }

    fn apply(&self, x: f32, y: f32) -> (f32, f32) {
        (x + self.translate[0], y + self.translate[1])
    }

    fn current_clip(&self) -> [f32; 4] {
        self.clip_stack.last().copied().unwrap_or([-1.0, -1.0, -1.0, -1.0])
    }

    fn push_vertices_for_shape(&mut self, shape: &crate::vg::shape::Shape) {
        // Reuse Vulkan's draw_shape logic but without Vulkan pipeline specifics
        // Simplified: we call the same vertex generation as VgContext::draw_shape
        // For now, delegate to a helper that mirrors vg/mod.rs draw_shape
        // To avoid duplication, we directly push vertices using the same math
        // (copy-pasted and adapted from vg/mod.rs draw_shape)
        use crate::vg::shape::{ShapeKind, GradMode};
        use crate::vg::shape::Shape as S;

        let (bounds_arr, _half_tmp) = shape.bounds();
        let half = (bounds_arr[2] - bounds_arr[0]).max(bounds_arr[3] - bounds_arr[1]) * 0.5;
        let bounds = bounds_arr;
        let (corner_radius, stroke, flags) = match shape.kind {
            ShapeKind::RoundedRect { radius } => (radius, shape.stroke_width, if shape.stroke_width > 0.0 { FLAG_STROKE } else { 0.0 }),
            ShapeKind::Rect => (0.0, shape.stroke_width, if shape.stroke_width > 0.0 { FLAG_STROKE } else { 0.0 }),
            ShapeKind::Circle => (half.max(0.001), shape.stroke_width, if shape.stroke_width > 0.0 { FLAG_STROKE } else { 0.0 }),
            ShapeKind::Ellipse => (half.max(0.001), shape.stroke_width, FLAG_ELLIPSE + if shape.stroke_width > 0.0 { FLAG_STROKE } else { 0.0 }),
            ShapeKind::Polygon { sides } => (sides as f32, shape.stroke_width, FLAG_POLYGON + if shape.stroke_width > 0.0 { FLAG_STROKE } else { 0.0 }),
            ShapeKind::Triangle { .. } => (0.0, shape.stroke_width, if shape.stroke_width > 0.0 { FLAG_STROKE } else { 0.0 }),
            ShapeKind::Arc { .. } => (half.max(0.001), shape.stroke_width, if shape.stroke_width > 0.0 { FLAG_STROKE } else { 0.0 }),
            ShapeKind::Line { .. } => (shape.stroke_width * 0.5 + 0.5, shape.stroke_width, FLAG_LINE + FLAG_STROKE),
            ShapeKind::Text => (0.0, 0.0, FLAG_TEXTURE),
        };

        let corners = [
            [bounds[0], bounds[1]],
            [bounds[2], bounds[1]],
            [bounds[2], bounds[3]],
            [bounds[0], bounds[3]],
        ];
        let tris: [[usize; 3]; 2] = [[0, 1, 2], [0, 2, 3]];
        let feather = (1.0 / self.scale).max(0.5);
        let default_uv = [0.0, 0.0];

        for tri in tris {
            for &i in &tri {
                let c = corners[i];
                let (vw, vh) = (self.viewport[0].max(1.0), self.viewport[1].max(1.0));
                let clip_x = c[0] / vw * 2.0 - 1.0;
                let clip_y = 1.0 - c[1] / vh * 2.0;
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

    // Public queue API (same as Vulkan Renderer)
    pub fn queue_rect(&mut self, x: f32, y: f32, w: f32, h: f32, color: crate::vg::shape::Color) {
        let (x, y) = self.apply(x, y);
        self.push_vertices_for_shape(&crate::vg::shape::Shape {
            kind: crate::vg::shape::ShapeKind::Rect, x, y, w, h,
            fill_color: color, ..Default::default()
        });
    }

    pub fn queue_rounded_rect(&mut self, x: f32, y: f32, w: f32, h: f32, r: f32, color: crate::vg::shape::Color) {
        let (x, y) = self.apply(x, y);
        self.push_vertices_for_shape(&crate::vg::shape::Shape {
            kind: crate::vg::shape::ShapeKind::RoundedRect { radius: r }, x, y, w, h,
            fill_color: color, ..Default::default()
        });
    }

    pub fn queue_rect_stroke(&mut self, x: f32, y: f32, w: f32, h: f32, stroke: f32, color: crate::vg::shape::Color) {
        let (x, y) = self.apply(x, y);
        self.push_vertices_for_shape(&crate::vg::shape::Shape {
            kind: crate::vg::shape::ShapeKind::Rect, x, y, w, h,
            fill_color: color, stroke_width: stroke, ..Default::default()
        });
    }

    pub fn queue_circle(&mut self, cx: f32, cy: f32, r: f32, color: crate::vg::shape::Color) {
        let (cx, cy) = self.apply(cx, cy);
        self.push_vertices_for_shape(&crate::vg::shape::Shape {
            kind: crate::vg::shape::ShapeKind::Circle, x: cx - r, y: cy - r, w: r*2.0, h: r*2.0,
            fill_color: color, ..Default::default()
        });
    }

    pub fn queue_circle_stroke(&mut self, cx: f32, cy: f32, r: f32, w: f32, color: crate::vg::shape::Color) {
        let (cx, cy) = self.apply(cx, cy);
        self.push_vertices_for_shape(&crate::vg::shape::Shape {
            kind: crate::vg::shape::ShapeKind::Circle, x: cx - r, y: cy - r, w: r*2.0, h: r*2.0,
            fill_color: color, stroke_width: w, ..Default::default()
        });
    }

    pub fn queue_line(&mut self, x0: f32, y0: f32, x1: f32, y1: f32, width: f32, color: crate::vg::shape::Color) {
        let (x0, y0) = self.apply(x0, y0);
        let (x1, y1) = self.apply(x1, y1);
        let d = ((x1-x0).powi(2) + (y1-y0).powi(2)).sqrt();
        let pad = width * 0.5 + 2.0;
        let nx = if d > 0.0 { -(y1-y0)/d } else { 1.0 };
        let ny = if d > 0.0 { (x1-x0)/d } else { 0.0 };
        self.push_vertices_for_shape(&crate::vg::shape::Shape {
            kind: crate::vg::shape::ShapeKind::Line { x1, y1 }, x: x0.min(x1)-pad, y: y0.min(y1)-pad,
            w: (x0-x1).abs()+pad*2.0, h: (y0-y1).abs()+pad*2.0,
            fill_color: color, stroke_width: width, _line_n: [nx, ny], ..Default::default()
        });
    }

    pub fn queue_text(&mut self, x: f32, y: f32, text: &str, size: f32, color: crate::vg::shape::Color) {
        self.queue_text_aligned(x, y, text, size, color, crate::vg::shape::TextAlign::Left);
    }

    pub fn queue_text_aligned(&mut self, x: f32, y: f32, text: &str, size: f32, color: crate::vg::shape::Color, align: crate::vg::shape::TextAlign) {
        let (x, y) = self.apply(x, y);
        let scale = size / 16.0;
        let mut pen_x = match align {
            crate::vg::shape::TextAlign::Left => x,
            crate::vg::shape::TextAlign::Center => x - self.text_width(text, size) * 0.5,
            crate::vg::shape::TextAlign::Right => x - self.text_width(text, size),
        };
        for ch in text.chars() {
            if let Some(gi) = self.glyph_info(ch as u32) {
                let gw = gi.width * scale;
                let gh = gi.height * scale;
                let gx = (pen_x + gi.x_offset * scale).round();
                let gy = (y + gi.y_offset * scale).round();
                if gw > 0.0 && gh > 0.0 {
                    self.push_vertices_for_shape(&crate::vg::shape::Shape {
                        kind: crate::vg::shape::ShapeKind::Text, x: gx, y: gy, w: gw, h: gh,
                        fill_color: color, uv_override: Some([gi.u0, gi.v0, gi.u1, gi.v1]), ..Default::default()
                    });
                }
                pen_x += gi.advance * scale;
            }
        }
    }

    fn glyph_info(&self, code: u32) -> Option<GlyphInfo> {
        let idx = match code {
            32..=126 => (code - 32) as usize,
            _ => return None,
        };
        self.font_atlas.glyphs.get(idx).copied()
    }

    fn text_width(&self, text: &str, size: f32) -> f32 {
        let scale = size / 16.0;
        let mut w = 0.0;
        for ch in text.chars() {
            if let Some(gi) = self.glyph_info(ch as u32) {
                w += gi.advance * scale;
            }
        }
        w
    }

    pub fn queue_linear_gradient(&mut self, x: f32, y: f32, w: f32, h: f32, r: f32, from: [f32; 2], to: [f32; 2], c0: crate::vg::shape::Color, c1: crate::vg::shape::Color) {
        let (x, y) = self.apply(x, y);
        let (fx, fy) = self.apply(from[0], from[1]);
        let (tx, ty) = self.apply(to[0], to[1]);
        self.push_vertices_for_shape(&crate::vg::shape::Shape {
            kind: crate::vg::shape::ShapeKind::RoundedRect { radius: r }, x, y, w, h,
            fill_color: c0, grad_color: c1, grad_mode: crate::vg::shape::GradMode::Linear,
            grad_from: [fx, fy], grad_to: [tx, ty], ..Default::default()
        });
    }

    pub fn queue_triangle(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x3: f32, y3: f32, color: crate::vg::shape::Color) {
        let (x1, y1) = self.apply(x1, y1);
        let (x2, y2) = self.apply(x2, y2);
        let (x3, y3) = self.apply(x3, y3);
        let min_x = x1.min(x2).min(x3);
        let min_y = y1.min(y2).min(y3);
        let max_x = x1.max(x2).max(x3);
        let max_y = y1.max(y2).max(y3);
        self.push_vertices_for_shape(&crate::vg::shape::Shape {
            kind: crate::vg::shape::ShapeKind::Triangle { x2, y2, x3, y3 }, x: min_x, y: min_y,
            w: (max_x-min_x).max(0.1), h: (max_y-min_y).max(0.1),
            fill_color: color, ..Default::default()
        });
    }

    pub fn queue_triangle_stroke(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x3: f32, y3: f32, w: f32, color: crate::vg::shape::Color) {
        let (x1, y1) = self.apply(x1, y1);
        let (x2, y2) = self.apply(x2, y2);
        let (x3, y3) = self.apply(x3, y3);
        let min_x = x1.min(x2).min(x3);
        let min_y = y1.min(y2).min(y3);
        let max_x = x1.max(x2).max(x3);
        let max_y = y1.max(y2).max(y3);
        self.push_vertices_for_shape(&crate::vg::shape::Shape {
            kind: crate::vg::shape::ShapeKind::Triangle { x2, y2, x3, y3 }, x: min_x, y: min_y,
            w: (max_x-min_x).max(0.1), h: (max_y-min_y).max(0.1),
            fill_color: color, stroke_width: w, ..Default::default()
        });
    }

    pub fn queue_arc(&mut self, cx: f32, cy: f32, r: f32, s: f32, sw: f32, color: crate::vg::shape::Color) {
        let (cx, cy) = self.apply(cx, cy);
        self.push_vertices_for_shape(&crate::vg::shape::Shape {
            kind: crate::vg::shape::ShapeKind::Arc { start: s.to_radians(), sweep: sw.to_radians() },
            x: cx-r, y: cy-r, w: r*2.0, h: r*2.0, fill_color: color, ..Default::default()
        });
    }

    pub fn queue_arc_stroke(&mut self, cx: f32, cy: f32, r: f32, s: f32, sw: f32, w: f32, color: crate::vg::shape::Color) {
        let (cx, cy) = self.apply(cx, cy);
        self.push_vertices_for_shape(&crate::vg::shape::Shape {
            kind: crate::vg::shape::ShapeKind::Arc { start: s.to_radians(), sweep: sw.to_radians() },
            x: cx-r, y: cy-r, w: r*2.0, h: r*2.0, fill_color: color, stroke_width: w, ..Default::default()
        });
    }

    pub fn queue_push_translate(&mut self, tx: f32, ty: f32) {
        self.transform_stack.push(self.translate);
        self.translate[0] += tx;
        self.translate[1] += ty;
    }

    pub fn queue_pop_transform(&mut self) {
        if let Some(p) = self.transform_stack.pop() { self.translate = p; }
    }

    pub fn queue_push_clip(&mut self, x: f32, y: f32, w: f32, h: f32) {
        let (x, y) = self.apply(x, y);
        let rect = if let Some(&cur) = self.clip_stack.last() {
            if cur[2] < 0.0 { [x, y, w, h] } else {
                let x0 = x.max(cur[0]);
                let y0 = y.max(cur[1]);
                let x1 = (x+w).min(cur[0]+cur[2]);
                let y1 = (y+h).min(cur[1]+cur[3]);
                [x0, y0, (x1-x0).max(0.0), (y1-y0).max(0.0)]
            }
        } else { [x, y, w, h] };
        self.clip_stack.push(rect);
    }

    pub fn queue_pop_clip(&mut self) { self.clip_stack.pop(); }

    pub fn queue_ellipse(&mut self, cx: f32, cy: f32, rx: f32, ry: f32, color: crate::vg::shape::Color) {
        let (cx, cy) = self.apply(cx, cy);
        self.push_vertices_for_shape(&crate::vg::shape::Shape {
            kind: crate::vg::shape::ShapeKind::Ellipse, x: cx-rx, y: cy-ry, w: rx*2.0, h: ry*2.0,
            fill_color: color, ..Default::default()
        });
    }

    pub fn queue_ellipse_stroke(&mut self, cx: f32, cy: f32, rx: f32, ry: f32, w: f32, color: crate::vg::shape::Color) {
        let (cx, cy) = self.apply(cx, cy);
        self.push_vertices_for_shape(&crate::vg::shape::Shape {
            kind: crate::vg::shape::ShapeKind::Ellipse, x: cx-rx, y: cy-ry, w: rx*2.0, h: ry*2.0,
            fill_color: color, stroke_width: w, ..Default::default()
        });
    }

    pub fn queue_polygon(&mut self, cx: f32, cy: f32, r: f32, sides: u32, rot: f32, color: crate::vg::shape::Color) {
        let (cx, cy) = self.apply(cx, cy);
        self.push_vertices_for_shape(&crate::vg::shape::Shape {
            kind: crate::vg::shape::ShapeKind::Polygon { sides: sides.max(3) }, x: cx-r, y: cy-r, w: r*2.0, h: r*2.0,
            fill_color: color, grad_inner_radius: rot.to_radians(), ..Default::default()
        });
    }

    pub fn queue_shadow_rounded_rect(&mut self, x: f32, y: f32, w: f32, h: f32, r: f32, blur: f32, off: [f32; 2], color: crate::vg::shape::Color) {
        let c = color.with_alpha(color.a * 0.35);
        let b = blur;
        self.queue_rounded_rect(x+off[0]-b, y+off[1]-b, w+b*2.0, h+b*2.0, r+b, c);
    }

    pub fn queue_draw_image(&mut self, x: f32, y: f32, w: f32, h: f32, tint: crate::vg::shape::Color) {
        let (x, y) = self.apply(x, y);
        self.push_vertices_for_shape(&crate::vg::shape::Shape {
            kind: crate::vg::shape::ShapeKind::Rect, x, y, w, h,
            fill_color: tint, uv_override: Some([0.0,0.0,1.0,1.0]), ..Default::default()
        });
        let n = self.vertices.len();
        for v in &mut self.vertices[n.saturating_sub(6)..] { v.param[3] += FLAG_USER_TEX; }
    }

    pub fn queue_draw_rounded_image(&mut self, x: f32, y: f32, w: f32, h: f32, r: f32, tint: crate::vg::shape::Color) {
        let (x, y) = self.apply(x, y);
        self.push_vertices_for_shape(&crate::vg::shape::Shape {
            kind: crate::vg::shape::ShapeKind::RoundedRect { radius: r }, x, y, w, h,
            fill_color: tint, uv_override: Some([0.0,0.0,1.0,1.0]), ..Default::default()
        });
        let n = self.vertices.len();
        for v in &mut self.vertices[n.saturating_sub(6)..] { v.param[3] += FLAG_USER_TEX; }
    }

    pub fn queue_text_aligned_queue(&mut self, x: f32, y: f32, text: &str, size: f32, color: crate::vg::shape::Color, align: crate::vg::shape::TextAlign) {
        self.queue_text_aligned(x, y, text, size, color, align);
    }

    pub fn set_font(&mut self, _ttf: &[u8], _size: f32) -> anyhow::Result<()> { Ok(()) }
    pub fn set_image_from_bytes(&mut self, _bytes: &[u8]) -> anyhow::Result<()> { Ok(()) }
}

unsafe fn create_program(gl: &glow::Context, vert_src: &str, frag_src: &str) -> Result<glow::NativeProgram> {
    unsafe {
        let vert = gl.create_shader(glow::VERTEX_SHADER).map_err(|e| anyhow::anyhow!(e))?;
        gl.shader_source(vert, vert_src);
        gl.compile_shader(vert);
        if !gl.get_shader_compile_status(vert) {
            anyhow::bail!("vert compile: {}", gl.get_shader_info_log(vert));
        }
        let frag = gl.create_shader(glow::FRAGMENT_SHADER).map_err(|e| anyhow::anyhow!(e))?;
        gl.shader_source(frag, frag_src);
        gl.compile_shader(frag);
        if !gl.get_shader_compile_status(frag) {
            anyhow::bail!("frag compile: {}", gl.get_shader_info_log(frag));
        }
        let prog = gl.create_program().map_err(|e| anyhow::anyhow!(e))?;
        gl.attach_shader(prog, vert);
        gl.attach_shader(prog, frag);
        gl.link_program(prog);
        if !gl.get_program_link_status(prog) {
            anyhow::bail!("link: {}", gl.get_program_info_log(prog));
        }
        gl.delete_shader(vert);
        gl.delete_shader(frag);
        Ok(prog)
    }
}

unsafe fn create_font_texture(gl: &glow::Context) -> Result<(glow::NativeTexture, Vec<GlyphInfo>)> {
    // Generate 1x1 white dummy and font atlas as GL texture
    // For now, reuse the same fontdue logic as Vulkan but upload via GL
    // Simplified: create 512x256 atlas
    let ttf = include_bytes!("../../assets/JetBrainsMono-Regular.ttf");
    let font = fontdue::Font::from_bytes(ttf.as_ref(), fontdue::FontSettings::default())
        .map_err(|e| anyhow::anyhow!("font parse: {:?}", e))?;
    let atlas_w = 512u32;
    let atlas_h = 256u32;
    let mut pixels = vec![0u8; (atlas_w * atlas_h) as usize];
    let mut glyphs = Vec::new();
    // Simplified glyph packing - 16x16 grid
    for i in 0..95 {
        let ch = char::from_u32(32 + i as u32).unwrap_or('?');
        let (m, cov) = font.rasterize(ch, 16.0);
        let col = (i % 16) as u32;
        let row = (i / 16) as u32;
        let ox = col * 32;
        let oy = row * 32;
        for y in 0..m.height.min(32) {
            for x in 0..m.width.min(32) {
                let a = cov[y * m.width + x];
                pixels[((oy + y as u32) * atlas_w + ox + x as u32) as usize] = a;
            }
        }
        glyphs.push(GlyphInfo {
            u0: ox as f32 / atlas_w as f32,
            v0: oy as f32 / atlas_h as f32,
            u1: (ox + 32) as f32 / atlas_w as f32,
            v1: (oy + 32) as f32 / atlas_h as f32,
            width: 32.0, height: 32.0,
            x_offset: 0.0, y_offset: 0.0,
            advance: m.advance_width,
            cell_w: 32.0, cell_h: 32.0,
        });
    }
    let tex = gl.create_texture().map_err(|e| anyhow::anyhow!(e))?;
    gl.bind_texture(glow::TEXTURE_2D, Some(tex));
    gl.tex_image_2d(
        glow::TEXTURE_2D, 0, glow::R8 as i32,
        atlas_w as i32, atlas_h as i32, 0,
        glow::RED, glow::UNSIGNED_BYTE,
        glow::PixelUnpackData::Slice(Some(&pixels)),
    );
    gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, glow::LINEAR as i32);
    gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, glow::LINEAR as i32);
    gl.bind_texture(glow::TEXTURE_2D, None);
    Ok((tex, glyphs))
}
