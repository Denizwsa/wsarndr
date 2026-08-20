# wsarndr

Standalone, embeddable 2D render system with a Vulkan backend and Rust native core. Designed for Minecraft client UI (ClickGUI, ArrayList, TargetHUD, etc.) but usable for any 2D overlay or HUD.

## Features

- Vulkan rendering via `ash`
- 2D immediate-mode API: rounded rects, circles, lines, text, gradients
- SDF-based shape rendering with anti-aliasing
- Font atlas with system font fallback (JetBrains Mono, Liberation, Noto, DejaVu)
- JNI bridge for Java/Kotlin integration (Minecraft mod friendly)
- Winit demo for standalone testing
- Window resize with proper swapchain recreation
- Auto-scaling UI elements

## Project Structure

```
wsarndr/
├── native/             # Rust core library
│   ├── src/
│   │   ├── lib.rs          # JNI exports
│   │   ├── renderer.rs     # Renderer, command queue, swapchain management
│   │   ├── vk/             # Vulkan instance, device, swapchain
│   │   ├── vg/             # 2D vector graphics context
│   │   │   ├── mod.rs      # VgContext, draw shapes, vertex buffer
│   │   │   ├── font.rs     # Font atlas rasterization
│   │   │   └── shape.rs    # Shape types, Color
│   │   └── world/          # 3D module (disabled, kept for reference)
│   └── shaders/
│       ├── vg.vert         # Vertex shader (pixel → clip space)
│       └── vg.frag         # Fragment shader (SDF rendering, gradients, text)
├── java/               # Java JNI wrapper + demo
│   └── src/main/java/dev/wsarndr/
│       ├── Native.java     # JNI method declarations
│       └── NativeDemo.java # GLFW-based demo
├── examples/
│   └── demo/           # Winit demo (Rust)
└── Cargo.toml          # Workspace root
```

## Requirements

- Rust 1.70+
- Vulkan 1.2+ GPU and driver
- `glslc` (shader compiler) in PATH
- Java 17+ (for JNI usage)

## Build

```bash
# Rust demo
cargo build -p wsarndr-demo

# Run
cargo run -p wsarndr-demo
```

## Java / JNI Integration

### 1. Build the native library

```bash
cargo build
# Output: target/debug/libwsarndr.so (Linux)
```

### 2. Use from Java

```java
// Load native library
Native.load("/path/to/libwsarndr.so");

// Create renderer (X11 example)
long ptr = Native.create(Native.BACKEND_VULKAN, Native.WIN_XLIB, winHandle, displayHandle);

// Each frame
Native.beginFrame(ptr);
Native.roundedRect(ptr, x, y, w, h, radius, argbColor);
Native.circle(ptr, cx, cy, r, argbColor);
Native.text(ptr, x, y, "Hello", 16.0f, argbColor);
Native.endFrame(ptr);

// On window resize
Native.resize(ptr, newWidth, newHeight);

// Cleanup
Native.destroy(ptr);
```

### Window handles (from GLFW)

| Platform  | `winHandleType`       | `winHandle`                        | `displayHandle`                    |
|-----------|-----------------------|------------------------------------|------------------------------------|
| X11       | `WIN_XLIB` (0)        | `glfwGetX11Window(win)`            | `glfwGetX11Display()`              |
| Wayland   | `WIN_WAYLAND` (1)     | `glfwGetWaylandWindow(win)`        | `glfwGetWaylandDisplay()`          |
| XCB       | `WIN_XCB` (2)         | `glfwGetXCBWindow(win)`            | `glfwGetXCBConnection()`           |
| Win32     | `WIN_WIN32` (3)       | `glfwGetWin32Window(win)`          | `0`                                |

## API Reference

### VgContext (Rust immediate mode)

```rust
vg.rect_fill(x, y, w, h, color);
vg.rounded_rect_fill(x, y, w, h, radius, color);
vg.rounded_rect_stroke(x, y, w, h, radius, stroke_width, color);
vg.rect_stroke(x, y, w, h, stroke_width, color);
vg.circle_fill(cx, cy, r, color);
vg.circle_stroke(cx, cy, r, stroke_width, color);
vg.line(x0, y0, x1, y1, width, color);
vg.text(x, y, "text", font_size, color);
vg.linear_gradient_rounded_rect(x, y, w, h, r, from, to, color0, color1);
vg.radial_gradient_circle(cx, cy, r, color0, color1);
```

### Color

Colors are ARGB packed as `u32`:

```rust
Color::argb(0xFF_FF0000) // opaque red
Color::rgb(1.0, 0.0, 0.0) // same
```

### Renderer

```rust
let renderer = Renderer::new(&window, "App", true, None)?;
renderer.resize(width, height)?;
renderer.render(|vg, w, h| {
    // draw with vg using w, h as viewport dimensions
});
```

## License

MIT
