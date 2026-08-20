pub mod renderer;
pub mod vg;
pub mod vk;

use std::sync::{Arc, Mutex};

use jni::objects::{JClass, JString};
use jni::sys::{jfloat, jint, jlong, jstring};
use jni::JNIEnv;
use raw_window_handle::{
    DisplayHandle, HandleError, HasDisplayHandle, HasWindowHandle, RawDisplayHandle,
    RawWindowHandle, WaylandDisplayHandle, WaylandWindowHandle, Win32WindowHandle, XcbWindowHandle,
    XlibWindowHandle,
};

use crate::renderer::{Renderer, WindowSizeProvider};
use crate::vg::shape::Color;

/// Thread-safe handle: JNI calls only add vertices outside of render.
#[derive(Clone)]
pub struct RendererHandle(pub Arc<Mutex<Renderer>>);

#[no_mangle]
pub unsafe extern "system" fn Java_dev_wsarndr_Native_create(
    mut env: JNIEnv,
    _class: JClass,
    backend: jint,
    win_handle_type: jint,
    win_handle: jlong,
    display_handle: jlong,
) -> jlong {
    let result = create_renderer(backend, win_handle_type, win_handle, display_handle);
    match result {
        Ok(r) => Box::into_raw(Box::new(r)) as jlong,
        Err(e) => {
            let _ = env.throw_new("java/lang/IllegalStateException", format!("wsarndr: {}", e));
            0
        }
    }
}

fn create_renderer(
    _backend: i32,
    win_handle_type: i32,
    win_handle: i64,
    display_handle: i64,
) -> anyhow::Result<RendererHandle> {
    let window = PlatformWindow {
        win_type: win_handle_type,
        win_handle: win_handle as usize,
        display_handle: display_handle as usize,
    };
    let r = Renderer::new(&window, "wsarndr", true, None)?;
    Ok(RendererHandle(Arc::new(Mutex::new(r))))
}

#[no_mangle]
pub unsafe extern "system" fn Java_dev_wsarndr_Native_destroy(
    _env: JNIEnv,
    _class: JClass,
    ptr: jlong,
) {
    if ptr == 0 {
        return;
    }
    drop(Box::from_raw(ptr as *mut RendererHandle));
}

#[no_mangle]
pub unsafe extern "system" fn Java_dev_wsarndr_Native_beginFrame(
    mut env: JNIEnv,
    _class: JClass,
    ptr: jlong,
) {
    let rh = match handle(&mut env, ptr) {
        Some(h) => h,
        None => return,
    };
    rh.0.lock().unwrap().clear_queue();
}

/// Get mutable handle; throw Java exception on error.
fn handle<'a>(env: &mut JNIEnv, ptr: jlong) -> Option<&'a RendererHandle> {
    if ptr == 0 {
        let _ = env.throw_new("java/lang/NullPointerException", "wsarndr handle is null");
        return None;
    }
    Some(unsafe { &*(ptr as *const RendererHandle) })
}

#[no_mangle]
pub unsafe extern "system" fn Java_dev_wsarndr_Native_endFrame(
    mut env: JNIEnv,
    _class: JClass,
    ptr: jlong,
) {
    let rh = match handle(&mut env, ptr) {
        Some(h) => h,
        None => return,
    };
    let mut r = match rh.0.lock() {
        Ok(g) => g,
        Err(e) => {
            let _ = env.throw_new("java/lang/IllegalStateException", format!("wsarndr: {}", e));
            return;
        }
    };
    match r.render(|_, _, _| {}) {
        Ok(_) => {}
        Err(e) => {
            let _ = env.throw_new("java/lang/IllegalStateException", format!("wsarndr render: {}", e));
        }
    }
}

#[no_mangle]
pub unsafe extern "system" fn Java_dev_wsarndr_Native_rect(
    mut env: JNIEnv,
    _class: JClass,
    ptr: jlong,
    x: jfloat,
    y: jfloat,
    w: jfloat,
    h: jfloat,
    argb: jint,
) {
    let rh = match handle(&mut env, ptr) {
        Some(h) => h,
        None => return,
    };
    rh.0.lock().unwrap().queue_rect(x, y, w, h, Color::argb(argb as u32));
}

#[no_mangle]
pub unsafe extern "system" fn Java_dev_wsarndr_Native_roundedRect(
    mut env: JNIEnv,
    _class: JClass,
    ptr: jlong,
    x: jfloat,
    y: jfloat,
    w: jfloat,
    h: jfloat,
    r: jfloat,
    argb: jint,
) {
    let rh = match handle(&mut env, ptr) {
        Some(h) => h,
        None => return,
    };
    rh.0.lock().unwrap().queue_rounded_rect(x, y, w, h, r, Color::argb(argb as u32));
}

#[no_mangle]
pub unsafe extern "system" fn Java_dev_wsarndr_Native_rectStroke(
    mut env: JNIEnv,
    _class: JClass,
    ptr: jlong,
    x: jfloat,
    y: jfloat,
    w: jfloat,
    h: jfloat,
    stroke: jfloat,
    argb: jint,
) {
    let rh = match handle(&mut env, ptr) {
        Some(h) => h,
        None => return,
    };
    rh.0.lock().unwrap().queue_rect_stroke(x, y, w, h, stroke, Color::argb(argb as u32));
}

#[no_mangle]
pub unsafe extern "system" fn Java_dev_wsarndr_Native_circle(
    mut env: JNIEnv,
    _class: JClass,
    ptr: jlong,
    cx: jfloat,
    cy: jfloat,
    r: jfloat,
    argb: jint,
) {
    let rh = match handle(&mut env, ptr) {
        Some(h) => h,
        None => return,
    };
    rh.0.lock().unwrap().queue_circle(cx, cy, r, Color::argb(argb as u32));
}

#[no_mangle]
pub unsafe extern "system" fn Java_dev_wsarndr_Native_line(
    mut env: JNIEnv,
    _class: JClass,
    ptr: jlong,
    x0: jfloat,
    y0: jfloat,
    x1: jfloat,
    y1: jfloat,
    width: jfloat,
    argb: jint,
) {
    let rh = match handle(&mut env, ptr) {
        Some(h) => h,
        None => return,
    };
    rh.0.lock().unwrap().queue_line(x0, y0, x1, y1, width, Color::argb(argb as u32));
}

#[no_mangle]
pub unsafe extern "system" fn Java_dev_wsarndr_Native_text(
    mut env: JNIEnv,
    _class: JClass,
    ptr: jlong,
    x: jfloat,
    y: jfloat,
    text: JString,
    size: jfloat,
    argb: jint,
) {
    let h = match handle(&mut env, ptr) {
        Some(h) => h,
        None => return,
    };
    let s: String = env
        .get_string(&text)
        .map(|j| j.into())
        .unwrap_or_default();
    h.0.lock().unwrap().queue_text(x, y, &s, size, Color::argb(argb as u32));
}

#[no_mangle]
pub unsafe extern "system" fn Java_dev_wsarndr_Native_linearGradient(
    mut env: JNIEnv,
    _class: JClass,
    ptr: jlong,
    x: jfloat,
    y: jfloat,
    w: jfloat,
    h: jfloat,
    r: jfloat,
    from_x: jfloat,
    from_y: jfloat,
    to_x: jfloat,
    to_y: jfloat,
    c0: jint,
    c1: jint,
) {
    let rh = match handle(&mut env, ptr) {
        Some(h) => h,
        None => return,
    };
    rh.0.lock().unwrap().queue_linear_gradient(
        x, y, w, h, r,
        [from_x, from_y],
        [to_x, to_y],
        Color::argb(c0 as u32),
        Color::argb(c1 as u32),
    );
}

#[no_mangle]
pub unsafe extern "system" fn Java_dev_wsarndr_Native_clearQueue(
    mut _env: JNIEnv,
    _class: JClass,
    ptr: jlong,
) {
    let rh = match handle(&mut _env, ptr) {
        Some(h) => h,
        None => return,
    };
    rh.0.lock().unwrap().clear_queue();
}

#[no_mangle]
pub unsafe extern "system" fn Java_dev_wsarndr_Native_resize(
    mut env: JNIEnv,
    _class: JClass,
    ptr: jlong,
    width: jint,
    height: jint,
) {
    let rh = match handle(&mut env, ptr) {
        Some(h) => h,
        None => return,
    };
    if let Ok(mut r) = rh.0.lock() {
        if let Err(e) = r.resize(width as u32, height as u32) {
            let _ = env.throw_new("java/lang/IllegalStateException", format!("wsarndr resize: {}", e));
        }
    }
}

// ---- Platform window handle bridge ----

pub struct PlatformWindow {
    pub win_type: i32,
    pub win_handle: usize,
    pub display_handle: usize,
}

unsafe impl Send for PlatformWindow {}
unsafe impl Sync for PlatformWindow {}

impl WindowSizeProvider for PlatformWindow {
    fn size(&self) -> Option<(u32, u32)> {
        None
    }
}

impl HasWindowHandle for PlatformWindow {
    fn window_handle(&self) -> Result<raw_window_handle::WindowHandle<'_>, HandleError> {
        use std::ffi::c_void;
        let raw = match self.win_type {
            0 => RawWindowHandle::Xlib(XlibWindowHandle::new(self.win_handle as _)),
            1 => RawWindowHandle::Wayland(WaylandWindowHandle::new(
                std::ptr::NonNull::new(self.win_handle as *mut c_void)
                    .ok_or(HandleError::Unavailable)?,
            )),
            2 => RawWindowHandle::Xcb(XcbWindowHandle::new(
                std::num::NonZeroU32::new(self.win_handle as u32)
                    .ok_or(HandleError::Unavailable)?,
            )),
            3 => RawWindowHandle::Win32(Win32WindowHandle::new(
                std::num::NonZero::new(self.win_handle as isize)
                    .ok_or(HandleError::Unavailable)?,
            )),
            _ => return Err(HandleError::Unavailable),
        };
        Ok(unsafe { raw_window_handle::WindowHandle::borrow_raw(raw) })
    }
}

impl HasDisplayHandle for PlatformWindow {
    fn display_handle(&self) -> Result<DisplayHandle<'_>, HandleError> {
        use std::ffi::c_void;
        let raw = match self.win_type {
            0 | 2 => RawDisplayHandle::Xlib(raw_window_handle::XlibDisplayHandle::new(
                std::ptr::NonNull::new(self.display_handle as *mut c_void),
                0,
            )),
            1 => RawDisplayHandle::Wayland(WaylandDisplayHandle::new(
                std::ptr::NonNull::new(self.display_handle as *mut c_void)
                    .ok_or(HandleError::Unavailable)?,
            )),
            3 => RawDisplayHandle::Windows(raw_window_handle::WindowsDisplayHandle::new()),
            _ => return Err(HandleError::Unavailable),
        };
        Ok(unsafe { DisplayHandle::borrow_raw(raw) })
    }
}

#[allow(dead_code)]
fn _unused_jni_signatures(
    _env: JNIEnv,
    _class: JClass,
    _ptr: jlong,
    _text: jstring,
    _n: jint,
) {
}