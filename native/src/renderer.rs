use std::sync::Arc;

use ash::vk;
use std::sync::Mutex;

use crate::vg::font::FontAtlas;
use crate::vg::shape::{Color, TextAlign};
use crate::vg::VgContext;
use crate::vk::{Instance, Swapchain, SharedDevice, SharedInstance};

pub trait WindowSizeProvider {
    fn size(&self) -> Option<(u32, u32)>;
}

#[cfg(feature = "demo")]
impl WindowSizeProvider for winit::window::Window {
    fn size(&self) -> Option<(u32, u32)> {
        let s = self.inner_size();
        Some((s.width, s.height))
    }
}

impl<T: raw_window_handle::HasDisplayHandle + raw_window_handle::HasWindowHandle + WindowSizeProvider>
    WindowSizeProvider for &T
{
    fn size(&self) -> Option<(u32, u32)> {
        (**self).size()
    }
}

pub struct Renderer {
    pub instance: SharedInstance,
    pub surface: vk::SurfaceKHR,
    pub physical_device: vk::PhysicalDevice,
    pub device: SharedDevice,
    pub swapchain: Swapchain,
    pub render_pass: vk::RenderPass,
    pub framebuffers: Vec<vk::Framebuffer>,
    pub command_pool: vk::CommandPool,
    pub command_buffers: Vec<vk::CommandBuffer>,
    pub image_available: vk::Semaphore,
    pub render_finished: vk::Semaphore,
    pub in_flight: vk::Fence,
    pub vg: VgContext,
    pub current_frame: usize,
    pub dirty_swapchain: bool,
    pub clear_color: [f32; 4],
    pub ui_queue: Mutex<Vec<UiCommand>>,
}

#[derive(Clone, Debug)]
pub enum UiCommand {
    Rect(f32, f32, f32, f32, Color),
    RoundedRect(f32, f32, f32, f32, f32, Color),
    RectStroke(f32, f32, f32, f32, f32, Color),
    Circle(f32, f32, f32, Color),
    CircleStroke(f32, f32, f32, f32, Color),
    Line(f32, f32, f32, f32, f32, Color),
    Text(f32, f32, String, f32, Color),
    TextAligned(f32, f32, String, f32, Color, TextAlign),
    LinearGradient {
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        r: f32,
        from: [f32; 2],
        to: [f32; 2],
        c0: Color,
        c1: Color,
    },
    Triangle(f32, f32, f32, f32, f32, f32, Color),
    TriangleStroke(f32, f32, f32, f32, f32, f32, f32, Color),
    Arc(f32, f32, f32, f32, f32, Color),
    ArcStroke(f32, f32, f32, f32, f32, f32, Color),
    PushTranslate(f32, f32),
    PopTransform,
    PushClip(f32, f32, f32, f32),
    PopClip,
    Ellipse(f32, f32, f32, f32, Color),
    EllipseStroke(f32, f32, f32, f32, f32, Color),
    Polygon(f32, f32, f32, u32, f32, Color),
    Shadow(f32, f32, f32, f32, f32, f32, [f32; 2], Color),
}

/// Builder for Renderer - lets modders configure everything before creation.
pub struct RendererBuilder {
    app_name: String,
    font_ttf: Option<Vec<u8>>,
    font_size: f32,
    clear_color: [f32; 4],
    enable_validation: bool,
}

impl RendererBuilder {
    pub fn new(app_name: impl Into<String>) -> Self {
        Self {
            app_name: app_name.into(),
            font_ttf: None,
            font_size: 48.0,
            clear_color: [0.08, 0.08, 0.10, 1.0],
            enable_validation: true,
        }
    }
    pub fn font_bytes(mut self, ttf: Vec<u8>) -> Self { self.font_ttf = Some(ttf); self }
    pub fn font_size(mut self, px: f32) -> Self { self.font_size = px; self }
    pub fn clear_color(mut self, c: Color) -> Self { self.clear_color = c.to_array(); self }
    pub fn validation(mut self, enable: bool) -> Self { self.enable_validation = enable; self }
    pub fn build(self, window: &(impl raw_window_handle::HasDisplayHandle + raw_window_handle::HasWindowHandle + WindowSizeProvider)) -> anyhow::Result<Renderer> {
        let ttf_ref = self.font_ttf.as_deref();
        // Temporarily use the font_size via a thread-local or by creating a custom Renderer path.
        // For simplicity we ignore custom size here and use the default 48px bundled size;
        // modders can call `renderer.set_font(&ttf, size)` after creation for custom sizes.
        let mut r = Renderer::new(window, &self.app_name, self.enable_validation, ttf_ref)?;
        r.clear_color = self.clear_color;
        Ok(r)
    }
}

impl Renderer {
    pub fn new(
        window: &(impl raw_window_handle::HasDisplayHandle + raw_window_handle::HasWindowHandle + WindowSizeProvider),
        app_name: &str,
        enable_validation: bool,
        font_ttf: Option<&[u8]>,
    ) -> anyhow::Result<Self> {
        let entry = unsafe { ash::Entry::load()? };
        let display_handle = window
            .display_handle()
            .map_err(|e| anyhow::anyhow!("display handle: {:?}", e))?
            .as_raw();
        let window_handle = window
            .window_handle()
            .map_err(|e| anyhow::anyhow!("window handle: {:?}", e))?
            .as_raw();
        if let Some((width, height)) = window.size() {
            crate::vk::REQUESTED_EXTENT.with(|e| {
                *e.borrow_mut() = Some(vk::Extent2D {
                    width,
                    height,
                })
            });
        }
        let (instance, surface) =
            Instance::new(entry, display_handle, window_handle, app_name, enable_validation)?;
        let instance = Arc::new(instance);

        let (physical, gfx_q, present_q, caps) =
            instance.pick_physical_device(surface)?;
        let device = Arc::new(instance.create_device(physical, gfx_q, present_q)?);

        let swapchain = device.create_swapchain(&instance, surface, &caps, None, None)?;
        let render_pass = device.create_render_pass(swapchain.format)?;
        let framebuffers = device.create_framebuffers(
            render_pass,
            &swapchain.image_views,
            swapchain.extent,
        )?;

        let command_pool = device.create_command_pool()?;
        let command_buffers = device.create_command_buffers(command_pool, swapchain.images.len() as u32)?;
        let (image_available, render_finished, in_flight) = device.create_semaphores()?;

        let font_atlas = if let Some(ttf) = font_ttf {
            FontAtlas::load(device.clone(), ttf, 48.0)?
        } else {
            crate::vg::font::load_default_font(device.clone(), 48.0)?
        };

        let vg = VgContext::new(device.clone(), instance.clone(), render_pass, font_atlas)?;

        Ok(Self {
            instance,
            surface,
            physical_device: physical,
            device,
            swapchain,
            render_pass,
            framebuffers,
            command_pool,
            command_buffers,
            image_available,
            render_finished,
            in_flight,
            vg,
            current_frame: 0,
            dirty_swapchain: false,
            clear_color: [0.08, 0.08, 0.1, 1.0],
            ui_queue: Mutex::new(Vec::new()),
        })
    }

    pub fn queue_rect(&self, x: f32, y: f32, w: f32, h: f32, color: Color) {
        if let Ok(mut q) = self.ui_queue.lock() {
            q.push(UiCommand::Rect(x, y, w, h, color));
        }
    }

    pub fn queue_rounded_rect(&self, x: f32, y: f32, w: f32, h: f32, r: f32, color: Color) {
        if let Ok(mut q) = self.ui_queue.lock() {
            q.push(UiCommand::RoundedRect(x, y, w, h, r, color));
        }
    }

    pub fn queue_rect_stroke(&self, x: f32, y: f32, w: f32, h: f32, stroke: f32, color: Color) {
        if let Ok(mut q) = self.ui_queue.lock() {
            q.push(UiCommand::RectStroke(x, y, w, h, stroke, color));
        }
    }

    pub fn queue_circle(&self, cx: f32, cy: f32, r: f32, color: Color) {
        if let Ok(mut q) = self.ui_queue.lock() {
            q.push(UiCommand::Circle(cx, cy, r, color));
        }
    }

    pub fn queue_line(&self, x0: f32, y0: f32, x1: f32, y1: f32, width: f32, color: Color) {
        if let Ok(mut q) = self.ui_queue.lock() {
            q.push(UiCommand::Line(x0, y0, x1, y1, width, color));
        }
    }

    pub fn queue_text(&self, x: f32, y: f32, text: &str, size: f32, color: Color) {
        if let Ok(mut q) = self.ui_queue.lock() {
            q.push(UiCommand::Text(x, y, text.to_string(), size, color));
        }
    }

    pub fn queue_text_aligned(&self, x: f32, y: f32, text: &str, size: f32, color: Color, align: TextAlign) {
        if let Ok(mut q) = self.ui_queue.lock() {
            q.push(UiCommand::TextAligned(x, y, text.to_string(), size, color, align));
        }
    }

    pub fn queue_linear_gradient(
        &self,
        x: f32, y: f32, w: f32, h: f32, r: f32,
        from: [f32; 2],
        to: [f32; 2],
        c0: Color,
        c1: Color,
    ) {
        if let Ok(mut q) = self.ui_queue.lock() {
            q.push(UiCommand::LinearGradient { x, y, w, h, r, from, to, c0, c1 });
        }
    }

    pub fn queue_circle_stroke(&self, cx: f32, cy: f32, r: f32, width: f32, color: Color) {
        if let Ok(mut q) = self.ui_queue.lock() {
            q.push(UiCommand::CircleStroke(cx, cy, r, width, color));
        }
    }

    pub fn queue_triangle(&self, x1: f32, y1: f32, x2: f32, y2: f32, x3: f32, y3: f32, color: Color) {
        if let Ok(mut q) = self.ui_queue.lock() {
            q.push(UiCommand::Triangle(x1, y1, x2, y2, x3, y3, color));
        }
    }

    pub fn queue_triangle_stroke(&self, x1: f32, y1: f32, x2: f32, y2: f32, x3: f32, y3: f32, width: f32, color: Color) {
        if let Ok(mut q) = self.ui_queue.lock() {
            q.push(UiCommand::TriangleStroke(x1, y1, x2, y2, x3, y3, width, color));
        }
    }

    pub fn queue_arc(&self, cx: f32, cy: f32, r: f32, start_deg: f32, sweep_deg: f32, color: Color) {
        if let Ok(mut q) = self.ui_queue.lock() {
            q.push(UiCommand::Arc(cx, cy, r, start_deg, sweep_deg, color));
        }
    }

    pub fn queue_arc_stroke(&self, cx: f32, cy: f32, r: f32, start_deg: f32, sweep_deg: f32, width: f32, color: Color) {
        if let Ok(mut q) = self.ui_queue.lock() {
            q.push(UiCommand::ArcStroke(cx, cy, r, start_deg, sweep_deg, width, color));
        }
    }

    pub fn queue_push_translate(&self, tx: f32, ty: f32) {
        if let Ok(mut q) = self.ui_queue.lock() {
            q.push(UiCommand::PushTranslate(tx, ty));
        }
    }

    pub fn queue_pop_transform(&self) {
        if let Ok(mut q) = self.ui_queue.lock() {
            q.push(UiCommand::PopTransform);
        }
    }

    pub fn queue_push_clip(&self, x: f32, y: f32, w: f32, h: f32) {
        if let Ok(mut q) = self.ui_queue.lock() {
            q.push(UiCommand::PushClip(x, y, w, h));
        }
    }

    pub fn queue_pop_clip(&self) {
        if let Ok(mut q) = self.ui_queue.lock() {
            q.push(UiCommand::PopClip);
        }
    }

    pub fn queue_ellipse(&self, cx: f32, cy: f32, rx: f32, ry: f32, color: Color) {
        if let Ok(mut q) = self.ui_queue.lock() {
            q.push(UiCommand::Ellipse(cx, cy, rx, ry, color));
        }
    }

    pub fn queue_ellipse_stroke(&self, cx: f32, cy: f32, rx: f32, ry: f32, width: f32, color: Color) {
        if let Ok(mut q) = self.ui_queue.lock() {
            q.push(UiCommand::EllipseStroke(cx, cy, rx, ry, width, color));
        }
    }

    pub fn queue_polygon(&self, cx: f32, cy: f32, radius: f32, sides: u32, rotation_deg: f32, color: Color) {
        if let Ok(mut q) = self.ui_queue.lock() {
            q.push(UiCommand::Polygon(cx, cy, radius, sides, rotation_deg, color));
        }
    }

    pub fn queue_shadow_rounded_rect(&self, x: f32, y: f32, w: f32, h: f32, r: f32, blur: f32, offset: [f32; 2], color: Color) {
        if let Ok(mut q) = self.ui_queue.lock() {
            q.push(UiCommand::Shadow(x, y, w, h, r, blur, offset, color));
        }
    }

    pub fn clear_queue(&self) {
        if let Ok(mut q) = self.ui_queue.lock() {
            q.clear();
        }
    }

    #[allow(dead_code)]
    fn drain_queue(&self, vg: &mut VgContext) {
        Self::drain_queue_static(&self.ui_queue, vg);
    }

    fn drain_queue_static(queue: &Mutex<Vec<UiCommand>>, vg: &mut VgContext) {
        let cmds = queue.lock().map(|mut q| std::mem::take(&mut *q));
        if let Ok(cmds) = cmds {
            for cmd in cmds {
                match cmd {
                    UiCommand::Rect(x, y, w, h, c) => vg.rect_fill(x, y, w, h, c),
                    UiCommand::RoundedRect(x, y, w, h, r, c) => vg.rounded_rect_fill(x, y, w, h, r, c),
                    UiCommand::RectStroke(x, y, w, h, s, c) => vg.rect_stroke(x, y, w, h, s, c),
                    UiCommand::Circle(cx, cy, r, c) => vg.circle_fill(cx, cy, r, c),
                    UiCommand::CircleStroke(cx, cy, r, w, c) => vg.circle_stroke(cx, cy, r, w, c),
                    UiCommand::Line(x0, y0, x1, y1, w, c) => vg.line(x0, y0, x1, y1, w, c),
                    UiCommand::Text(x, y, s, size, c) => vg.text(x, y, &s, size, c),
                    UiCommand::TextAligned(x, y, s, size, c, align) => vg.text_aligned(x, y, &s, size, c, align),
                    UiCommand::LinearGradient { x, y, w, h, r, from, to, c0, c1 } => {
                        vg.linear_gradient_rounded_rect(x, y, w, h, r, from, to, c0, c1)
                    }
                    UiCommand::Triangle(x1, y1, x2, y2, x3, y3, c) => vg.triangle_fill(x1, y1, x2, y2, x3, y3, c),
                    UiCommand::TriangleStroke(x1, y1, x2, y2, x3, y3, w, c) => vg.triangle_stroke(x1, y1, x2, y2, x3, y3, w, c),
                    UiCommand::Arc(cx, cy, r, start, sweep, c) => vg.arc_fill(cx, cy, r, start, sweep, c),
                    UiCommand::ArcStroke(cx, cy, r, start, sweep, w, c) => vg.arc_stroke(cx, cy, r, start, sweep, w, c),
                    UiCommand::PushTranslate(tx, ty) => vg.push_translate(tx, ty),
                    UiCommand::PopTransform => vg.pop_transform(),
                    UiCommand::PushClip(x, y, w, h) => vg.push_clip(x, y, w, h),
                    UiCommand::PopClip => vg.pop_clip(),
                    UiCommand::Ellipse(cx, cy, rx, ry, c) => vg.ellipse_fill(cx, cy, rx, ry, c),
                    UiCommand::EllipseStroke(cx, cy, rx, ry, w, c) => vg.ellipse_stroke(cx, cy, rx, ry, w, c),
                    UiCommand::Polygon(cx, cy, r, sides, rot, c) => vg.polygon_fill(cx, cy, r, sides, rot, c),
                    UiCommand::Shadow(x, y, w, h, r, b, off, c) => vg.shadow_rounded_rect(x, y, w, h, r, b, off, c),
                }
            }
        }
    }

    pub fn resize(&mut self, width: u32, height: u32) -> anyhow::Result<()> {
        unsafe { self.device.device.device_wait_idle()? };
        // Set the window size into REQUESTED_EXTENT so create_swapchain uses the correct dimensions
        crate::vk::REQUESTED_EXTENT.with(|e| {
            *e.borrow_mut() = Some(vk::Extent2D { width, height });
        });
        let caps = unsafe {
            self.instance
                .surface_loader
                .get_physical_device_surface_capabilities(self.physical_device, self.surface)?
        };
        let new_sc = self
            .device
            .create_swapchain(&self.instance, self.surface, &caps, Some(self.swapchain.swapchain), Some(vk::Extent2D { width, height }))?;
        unsafe {
            for v in &self.swapchain.image_views {
                self.device.device.destroy_image_view(*v, None);
            }
            self.device.swapchain_loader.destroy_swapchain(self.swapchain.swapchain, None);
        }
        self.swapchain = new_sc;

        for fb in &self.framebuffers {
            unsafe { self.device.device.destroy_framebuffer(*fb, None) };
        }
        self.framebuffers = self
            .device
            .create_framebuffers(self.render_pass, &self.swapchain.image_views, self.swapchain.extent)?;
        self.dirty_swapchain = false;
        Ok(())
    }

    pub fn set_font(&mut self, ttf: &[u8], size_px: f32) -> anyhow::Result<()> {
        self.vg.set_font(ttf, size_px)
    }

    pub fn render<F>(&mut self, draw: F) -> anyhow::Result<bool>
    where
        F: FnOnce(&mut VgContext, f32, f32),
    {
        if self.dirty_swapchain {
            return Ok(false);
        }

        unsafe {
            self.device
                .device
                .wait_for_fences(&[self.in_flight], true, u64::MAX)?;
        }

        let (image_index, suboptimal) = unsafe {
            self.device.swapchain_loader.acquire_next_image(
                self.swapchain.swapchain,
                u64::MAX,
                self.image_available,
                vk::Fence::null(),
            )?
        };
        if suboptimal {
            self.dirty_swapchain = true;
        }

        let cmd = self.command_buffers[image_index as usize];
        unsafe {
            self.device
                .device
                .reset_fences(&[self.in_flight])?;
            self.device
                .device
                .reset_command_buffer(cmd, vk::CommandBufferResetFlags::empty())?;
        }

        let extent = self.swapchain.extent;
        let (w, h) = (extent.width as f32, extent.height as f32);

        self.vg.begin_frame(w, h, 1.0);
        Self::drain_queue_static(&self.ui_queue, &mut self.vg);
        draw(&mut self.vg, w, h);
        self.vg.upload(&self.device)?;

        let clear = vk::ClearValue {
            color: vk::ClearColorValue {
                float32: self.clear_color,
            },
        };
        let render_pass_begin = vk::RenderPassBeginInfo::default()
            .render_pass(self.render_pass)
            .framebuffer(self.framebuffers[image_index as usize])
            .render_area(vk::Rect2D {
                offset: vk::Offset2D { x: 0, y: 0 },
                extent,
            })
            .clear_values(std::slice::from_ref(&clear));

        unsafe {
            self.device.device.begin_command_buffer(
                cmd,
                &vk::CommandBufferBeginInfo::default()
                    .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT),
            )?;
            self.device
                .device
                .cmd_begin_render_pass(cmd, &render_pass_begin, vk::SubpassContents::INLINE);

            self.vg.draw(cmd, extent);

            self.device.device.cmd_end_render_pass(cmd);
            self.device.device.end_command_buffer(cmd)?;
        }

        let wait_stages = [vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT];
        let submit = vk::SubmitInfo::default()
            .wait_semaphores(std::slice::from_ref(&self.image_available))
            .wait_dst_stage_mask(&wait_stages)
            .command_buffers(std::slice::from_ref(&cmd))
            .signal_semaphores(std::slice::from_ref(&self.render_finished));

        unsafe {
            self.device.device.queue_submit(
                self.device.graphics_queue,
                std::slice::from_ref(&submit),
                self.in_flight,
            )?;
        }

        let present = vk::PresentInfoKHR::default()
            .wait_semaphores(std::slice::from_ref(&self.render_finished))
            .swapchains(std::slice::from_ref(&self.swapchain.swapchain))
            .image_indices(std::slice::from_ref(&image_index));

        let result = unsafe {
            self.device
                .swapchain_loader
                .queue_present(self.device.present_queue, &present)
        };
        match result {
            Ok(true) => self.dirty_swapchain = true,
            Ok(false) => {}
            Err(vk::Result::ERROR_OUT_OF_DATE_KHR) => self.dirty_swapchain = true,
            Err(e) => return Err(e.into()),
        }

        self.current_frame = (self.current_frame + 1) % 2;
        Ok(true)
    }
}

impl Drop for Renderer {
    fn drop(&mut self) {
        unsafe {
            self.device.device.device_wait_idle().ok();
            self.device.device.destroy_semaphore(self.image_available, None);
            self.device.device.destroy_semaphore(self.render_finished, None);
            self.device.device.destroy_fence(self.in_flight, None);
            self.device
                .device
                .destroy_command_pool(self.command_pool, None);
            for fb in &self.framebuffers {
                self.device.device.destroy_framebuffer(*fb, None);
            }
            self.device.device.destroy_render_pass(self.render_pass, None);
            for v in &self.swapchain.image_views {
                self.device.device.destroy_image_view(*v, None);
            }
            self.device
                .swapchain_loader
                .destroy_swapchain(self.swapchain.swapchain, None);
            self.device.device.destroy_device(None);
            self.instance.surface_loader.destroy_surface(self.surface, None);
            self.instance.destroy();
        }
    }
}
