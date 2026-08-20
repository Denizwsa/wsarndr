use std::time::Instant;

use winit::application::ApplicationHandler;
use winit::dpi::PhysicalSize;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Window, WindowId};

use wsarndr::renderer::Renderer;
use wsarndr::vg::shape::{Color, TextAlign};

struct App {
    window: Option<Window>,
    renderer: Option<Renderer>,
    start: Instant,
    last_resize: Option<PhysicalSize<u32>>,
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let attrs = Window::default_attributes()
            .with_title("wsarndr demo")
            .with_inner_size(PhysicalSize::new(1280, 720));
        match event_loop.create_window(attrs) {
            Ok(window) => {
                self.window = Some(window);
            }
            Err(e) => {
                log::error!("window create failed: {}", e);
                event_loop.exit();
            }
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => self.last_resize = Some(size),
            WindowEvent::RedrawRequested => {
                if self.renderer.is_none() {
                    let size = self.window.as_ref().unwrap().inner_size();
                    if size.width == 0 || size.height == 0 {
                        self.window.as_ref().unwrap().request_redraw();
                        return;
                    }
                    self.ensure_renderer(event_loop);
                }

                let renderer = match self.renderer.as_mut() {
                    Some(r) => r,
                    None => return,
                };
                let window = match self.window.as_ref() {
                    Some(w) => w,
                    None => return,
                };

                if let Some(size) = self.last_resize.take() {
                    if size.width > 0 && size.height > 0 {
                        renderer.resize(size.width, size.height).ok();
                    }
                }

                let _t = self.start.elapsed().as_secs_f32();

                let result = renderer.render(|vg, w, h| {
                    let s = (w / 1280.0).min(h / 720.0);

                    // ClickGUI panel
                    let pw = 300.0 * s;
                    let ph = 200.0 * s;
                    let px = w * 0.25;
                    let py = h * 0.2;
                    vg.rounded_rect_fill(px, py, pw, ph, 8.0 * s, Color::argb(0xCC1E1E2E));
                    let header_h = 24.0 * s;
                    vg.rect_fill(px, py, pw, header_h, Color::argb(0xFF89B4FA));
                    vg.text(px + 8.0 * s, py + 5.0 * s, "wsarndr ClickGUI", 14.0 * s, Color::argb(0xFF11111B));

                    // Module list
                    let mut y = py + header_h + 12.0 * s;
                    for m in ["KillAura", "Scaffold", "Speed", "AntiCheat"].iter() {
                        let row_h = 18.0 * s;
                        let width = vg.text_width(m, 13.0 * s) + 20.0 * s;
                        let color = Color::argb(0x882A2A3C);
                        vg.rounded_rect_fill(px + 8.0 * s, y, width, row_h, 4.0 * s, color);
                        vg.text(px + 14.0 * s, y + 3.0 * s, m, 13.0 * s, Color::argb(0xFFCDD6F4));
                        y += row_h + 4.0 * s;
                    }

                    // TargetHUD
                    let thw = 160.0 * s;
                    let thh = 60.0 * s;
                    let thx = w * 0.7;
                    let thy = h * 0.3;
                    vg.rounded_rect_fill(thx, thy, thw, thh, 6.0 * s, Color::argb(0xDDA6ADC8));
                    vg.circle_fill(thx + 30.0 * s, thy + 30.0 * s, 18.0 * s, Color::argb(0xFFF9E2AF));
                    vg.text(thx + 56.0 * s, thy + 8.0 * s, "Steve", 14.0 * s, Color::argb(0xFF11111B));
                    vg.text(thx + 56.0 * s, thy + 28.0 * s, "20.0 HP", 12.0 * s, Color::argb(0xFF40A02B));

                    // Gradient + line + circle
                    let gw = 220.0 * s;
                    let gh = 40.0 * s;
                    vg.linear_gradient_rounded_rect(
                        w * 0.1, h * 0.8, gw, gh, 6.0 * s,
                        [w * 0.1, h * 0.8], [w * 0.1 + gw, h * 0.8],
                        Color::argb(0xFF89B4FA), Color::argb(0xFFF9E2AF),
                    );
                    vg.line(w * 0.1, h * 0.75, w * 0.9, h * 0.75, 2.0 * s, Color::argb(0xFFA6E3A1));

                    // Title text
                    let txt = "wsarndr - Vulkan 2D render system";
                    let tw = vg.text_width(txt, 20.0 * s);
                    vg.rect_fill(w * 0.5 - tw / 2.0 - 6.0 * s, h * 0.6 - 2.0 * s, tw + 12.0 * s, 26.0 * s, Color::argb(0xAA45475A));
                    vg.text(w * 0.5 - tw / 2.0, h * 0.6, txt, 20.0 * s, Color::argb(0xFFA6ADC8));

                    // New features demo: triangle, arc, transform stack
                    // Triangle (arrow indicator)
                    let tri_x = w * 0.6;
                    let tri_y = h * 0.15;
                    vg.triangle_fill(
                        tri_x, tri_y,
                        tri_x + 40.0 * s, tri_y + 20.0 * s,
                        tri_x, tri_y + 40.0 * s,
                        Color::argb(0xFFA6E3A1),
                    );

                    // Arc (health indicator)
                    vg.arc_fill(w * 0.85, h * 0.6, 30.0 * s, 0.0, 270.0, Color::argb(0xFFF9E2AF));
                    vg.arc_stroke(w * 0.85, h * 0.6, 30.0 * s, 0.0, 270.0, 2.0 * s, Color::argb(0xFF89B4FA));

                    // Transform stack demo
                    vg.push_translate(w * 0.15, h * 0.5);
                    vg.rounded_rect_fill(0.0, 0.0, 100.0 * s, 30.0 * s, 4.0 * s, Color::argb(0x882A2A3C));
                    vg.text_aligned(50.0 * s, 6.0 * s, "Translated", 12.0 * s, Color::argb(0xFFCDD6F4), TextAlign::Center);
                    vg.pop_transform();

                    // Text alignment demo
                    vg.text_aligned(w * 0.5, h * 0.92, "Centered text", 14.0 * s, Color::argb(0xFF89B4FA), TextAlign::Center);
                    vg.text_aligned(w * 0.85, h * 0.88, "Right", 14.0 * s, Color::argb(0xFFA6E3A1), TextAlign::Right);
                });

                if result.is_ok() {
                    window.request_redraw();
                }
            }
            _ => {}
        }
    }
}

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    let event_loop = EventLoop::new().expect("event loop");
    event_loop.set_control_flow(ControlFlow::Poll);

    let mut app = App {
        window: None,
        renderer: None,
        start: Instant::now(),
        last_resize: None,
    };
    event_loop.run_app(&mut app).expect("run app");
}

impl App {
    fn ensure_renderer(&mut self, event_loop: &ActiveEventLoop) {
        if self.renderer.is_some() {
            return;
        }
        let window = match self.window.as_ref() {
            Some(w) => w,
            None => return,
        };
        match Renderer::new(window, "wsarndr-demo", true, None) {
            Ok(renderer) => {
                self.renderer = Some(renderer);
                window.request_redraw();
            }
            Err(e) => {
                log::error!("renderer init failed: {}", e);
                event_loop.exit();
            }
        }
    }
}
