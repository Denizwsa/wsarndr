package dev.wsarndr;

import java.awt.Font;
import java.io.IOException;
import java.io.InputStream;
import java.nio.file.Files;
import java.nio.file.Path;

/**
 * High-level wsarndr wrapper with NanoVG-like API, custom font support and animation helpers.
 *
 * <pre>
 * WsArndr r = new WsArndr(Native.WIN_XLIB, win, display);
 * r.beginFrame();
 * r.roundedRect(10, 10, 200, 40, 8, 0xFF1E1E2E);
 * r.text(20, 28, "Hello wsarndr", 16, 0xFFCDD6F4);
 * r.endFrame();
 * r.close();
 * </pre>
 */
public final class WsArndr implements AutoCloseable {
    private long ptr;
    private float globalAlpha = 1f;

    public WsArndr(int winType, long winHandle, long displayHandle) {
        this.ptr = Native.create(Native.BACKEND_VULKAN, winType, winHandle, displayHandle);
        if (this.ptr == 0) throw new IllegalStateException("wsarndr: create failed");
    }

    public long handle() { return ptr; }

    public void beginFrame() { Native.beginFrame(ptr); }
    public void endFrame() { Native.endFrame(ptr); }
    public void resize(int w, int h) { Native.resize(ptr, w, h); }

    // ---- State ----
    public void save() { Native.save(ptr); }
    public void restore() { Native.restore(ptr); }
    public void translate(float x, float y) { Native.translate(ptr, x, y); }
    public void rotate(float deg) { Native.rotate(ptr, deg); }
    public void scale(float sx, float sy) { Native.scale(ptr, sx, sy); }
    public void globalAlpha(float a) { globalAlpha = Math.max(0, Math.min(1, a)); Native.globalAlpha(ptr, globalAlpha); }
    public void pushTranslate(float x, float y) { Native.pushTranslate(ptr, x, y); }
    public void popTransform() { Native.popTransform(ptr); }
    public void pushClip(float x, float y, float w, float h) { Native.pushClip(ptr, x, y, w, h); }
    public void popClip() { Native.popClip(ptr); }
    public void scissor(float x, float y, float w, float h) { pushClip(x, y, w, h); }
    public void resetScissor() { Native.popClip(ptr); }

    // ---- Primitives ----
    public void rect(float x, float y, float w, float h, int argb) { Native.rect(ptr, x, y, w, h, argb); }
    public void roundedRect(float x, float y, float w, float h, float r, int argb) { Native.roundedRect(ptr, x, y, w, h, r, argb); }
    public void rectStroke(float x, float y, float w, float h, float s, int argb) { Native.rectStroke(ptr, x, y, w, h, s, argb); }
    public void circle(float cx, float cy, float r, int argb) { Native.circle(ptr, cx, cy, r, argb); }
    public void circleStroke(float cx, float cy, float r, float w, int argb) { Native.circleStroke(ptr, cx, cy, r, w, argb); }
    public void ellipse(float cx, float cy, float rx, float ry, int argb) { Native.ellipse(ptr, cx, cy, rx, ry, argb); }
    public void ellipseStroke(float cx, float cy, float rx, float ry, float w, int argb) { Native.ellipseStroke(ptr, cx, cy, rx, ry, w, argb); }
    public void polygon(float cx, float cy, float r, int sides, float rot, int argb) { Native.polygon(ptr, cx, cy, r, sides, rot, argb); }
    public void triangle(float x1, float y1, float x2, float y2, float x3, float y3, int argb) { Native.triangle(ptr, x1, y1, x2, y2, x3, y3, argb); }
    public void line(float x0, float y0, float x1, float y1, float w, int argb) { Native.line(ptr, x0, y0, x1, y1, w, argb); }
    public void arc(float cx, float cy, float r, float a0, float a1, int argb) { Native.arc(ptr, cx, cy, r, a0, a1, argb); }

    // ---- Path (NanoVG compat) ----
    public void beginPath() { Native.beginPath(ptr); }
    public void moveTo(float x, float y) { Native.moveTo(ptr, x, y); }
    public void lineTo(float x, float y) { Native.lineTo(ptr, x, y); }
    public void bezierTo(float c1x, float c1y, float c2x, float c2y, float x, float y) { Native.bezierTo(ptr, c1x, c1y, c2x, c2y, x, y); }
    public void quadTo(float cx, float cy, float x, float y) { Native.quadTo(ptr, cx, cy, x, y); }
    public void closePath() { Native.closePath(ptr); }
    public void fill() { Native.fill(ptr); }
    public void stroke() { Native.stroke(ptr); }
    public void pathWinding(int dir) { /* dir 1=CCW, 2=CW */ }
    public void roundedRectVarying(float x, float y, float w, float h, float r1, float r2, float r3, float r4, int argb) {
        // Fallback to uniform radius
        float r = (r1 + r2 + r3 + r4) * 0.25f;
        roundedRect(x, y, w, h, r, argb);
    }

    // ---- Gradients / Shadows / Images ----
    public void linearGradient(float x, float y, float w, float h, float r, float fx, float fy, float tx, float ty, int c0, int c1) {
        Native.linearGradient(ptr, x, y, w, h, r, fx, fy, tx, ty, c0, c1);
    }
    public void boxGradient(float x, float y, float w, float h, float r, float f, int c0, int c1) {
        // Map to shadow + linear gradient for now
        shadowRoundedRect(x, y, w, h, r, f, 0, 0, c0);
    }
    public void shadowRoundedRect(float x, float y, float w, float h, float r, float blur, float offX, float offY, int argb) {
        Native.shadowRoundedRect(ptr, x, y, w, h, r, blur, offX, offY, argb);
    }
    public void setImage(byte[] pngBytes) { Native.setImage(ptr, pngBytes); }
    public void drawImage(float x, float y, float w, float h, int tint) { Native.drawImage(ptr, x, y, w, h, tint); }
    public void drawImage(float x, float y, float w, float h) { drawImage(x, y, w, h, 0xFFFFFFFF); }
    public void drawRoundedImage(float x, float y, float w, float h, float r, int tint) { Native.drawRoundedImage(ptr, x, y, w, h, r, tint); }

    // ---- Text / Custom Font ----
    public void text(float x, float y, String s, float size, int argb) { Native.text(ptr, x, y, s, size, argb); }
    public void textAligned(float x, float y, String s, float size, int argb, int align) { Native.textAligned(ptr, x, y, s, size, argb, align); }
    public void fontFace(String name) { Native.fontFace(ptr, name); }
    public void fontSize(float size) { Native.fontSize(ptr, size); }
    public int createFont(String name, byte[] ttf) { return Native.createFont(ptr, name, ttf); }
    public float[] textBounds(float x, float y, String text, float size) { return Native.textBounds(ptr, x, y, text, size); }

    /** Load a TTF font from bytes (e.g. resources). Size is atlas base size, e.g. 48. */
    public void setFont(byte[] ttf, float size) { Native.setFont(ptr, ttf, size); }

    /** Load a TTF font from a file path. */
    public void loadFont(Path ttfPath, float size) throws IOException {
        byte[] data = Files.readAllBytes(ttfPath);
        setFont(data, size);
    }

    /** Load a Java AWT Font as TTF bytes (fallback). */
    public void loadFont(Font awtFont, float size) {
        try (InputStream in = getClass().getResourceAsStream("/fonts/" + awtFont.getName() + ".ttf")) {
            if (in != null) setFont(in.readAllBytes(), size);
        } catch (Exception ignored) {}
    }

    // ---- Animation helpers (pure Java, no native needed) ----
    public static float lerp(float a, float b, float t) { t = Math.max(0, Math.min(1, t)); return a + (b - a) * t; }
    public static float clamp(float v, float lo, float hi) { return Math.max(lo, Math.min(hi, v)); }
    public static float easeOutCubic(float t) { t = clamp(t, 0, 1); float p = t - 1; return p * p * p + 1; }
    public static float easeInOutCubic(float t) {
        t = clamp(t, 0, 1);
        return t < 0.5 ? 4 * t * t * t : 0.5f * (float)Math.pow(2 * t - 2, 3) + 1;
    }
    public static float easeOutElastic(float t) {
        if (t == 0 || t == 1) return t;
        float p = 0.3f;
        return (float)(Math.pow(2, -10 * t) * Math.sin((t * 10 - 0.75) * (2 * Math.PI / p)) + 1);
    }
    /** Exponential smoothing: current -> target with speed (0..1) and dt seconds. */
    public static float animate(float cur, float target, float speed, float dt) {
        return cur + (target - cur) * (1 - (float)Math.exp(-speed * dt));
    }

    /** Simple spring animator for UI values. */
    public static final class Animator {
        public float value, target, velocity;
        public float stiffness = 300, damping = 30, mass = 1;
        public Animator(float initial) { value = target = initial; }
        public void setTarget(float t) { target = t; }
        public void set(float v) { value = target = v; velocity = 0; }
        public float update(float dt) {
            float f = -stiffness * (value - target);
            float d = -damping * velocity;
            float a = (f + d) / mass;
            velocity += a * dt;
            value += velocity * dt;
            if (Math.abs(value - target) < 0.01 && Math.abs(velocity) < 0.01) { value = target; velocity = 0; }
            return value;
        }
        public boolean isAtRest() { return value == target && velocity == 0; }
    }

    /** Color animation helper. */
    public static final class ColorAnimator {
        public int value, target;
        public ColorAnimator(int initial) { value = target = initial; }
        public void setTarget(int c) { target = c; }
        public void update(float t) { value = Native.lerpColor(value, target, t); }
    }

    @Override public void close() { if (ptr != 0) { Native.destroy(ptr); ptr = 0; } }
}
