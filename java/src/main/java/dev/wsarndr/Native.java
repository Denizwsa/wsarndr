package dev.wsarndr;

/**
 * wsarndr JNI bridge.
 *
 * Native library loading: {@code System.loadLibrary("wsarndr")} or
 * specify path via {@link #load(String)}.
 *
 * Window handles are obtained from GLFW:
 * <pre>
 *   X11:    display = glfwGetX11Display(),  window = glfwGetX11Window(handle)
 *   Wayland: display = glfwGetWaylandDisplay(), window = glfwGetWaylandWindow(handle)
 *   Win32:  display = 0,                     window = glfwGetWin32Window(handle)
 * </pre>
 */
public final class Native {

    /** Render backend selection (Vulkan only for now). */
    public static final int BACKEND_VULKAN = 0;

    /** Window handle types. */
    public static final int WIN_XLIB = 0;
    public static final int WIN_WAYLAND = 1;
    public static final int WIN_XCB = 2;
    public static final int WIN_WIN32 = 3;

    static {
        // If user called load(path) first, skip System.loadLibrary here.
        if (System.getProperty("wsarndr.skipLoadLibrary") == null) {
            System.loadLibrary("wsarndr");
        }
    }

    private Native() {
    }

    /** Loads the native library from an absolute file path. */
    public static void load(String path) {
        System.load(path);
    }

    /**
     * Creates a renderer.
     *
     * @param backend        {@link #BACKEND_VULKAN}
     * @param winHandleType  {@link #WIN_XLIB}, {@link #WIN_WAYLAND}, {@link #WIN_XCB}, or {@link #WIN_WIN32}
     * @param winHandle      window handle (from GLFW)
     * @param displayHandle  display handle (from GLFW; 0 on Win32)
     * @return native renderer handle (pointer), or 0 on error
     */
    public static native long create(int backend, int winHandleType, long winHandle, long displayHandle);

    /** Destroys the renderer. */
    public static native void destroy(long ptr);

    /** Begins a new frame and clears the command queue. */
    public static native void beginFrame(long ptr);

    /** Renders the queue and presents to screen. */
    public static native void endFrame(long ptr);

    /** Clears the queue (also done in beginFrame). */
    public static native void clearQueue(long ptr);

    /** Called when window size changes; recreates the swapchain. */
    public static native void resize(long ptr, int width, int height);

    /* ---- 2D commands ---- */

    public static native void rect(long ptr, float x, float y, float w, float h, int argb);

    public static native void roundedRect(long ptr, float x, float y, float w, float h, float radius, int argb);

    public static native void rectStroke(long ptr, float x, float y, float w, float h, float stroke, int argb);

    public static native void circle(long ptr, float cx, float cy, float r, int argb);

    public static native void line(long ptr, float x0, float y0, float x1, float y1, float width, int argb);

    public static native void text(long ptr, float x, float y, String text, float size, int argb);

    public static native void linearGradient(long ptr, float x, float y, float w, float h, float radius,
                                             float fromX, float fromY, float toX, float toY,
                                             int c0, int c1);

    /* ---- Extended API for modders ---- */

    /** Text alignment constants. */
    public static final int ALIGN_LEFT = 0;
    public static final int ALIGN_CENTER = 1;
    public static final int ALIGN_RIGHT = 2;

    /** Draw text with alignment (0=left, 1=center, 2=right). */
    public static native void textAligned(long ptr, float x, float y, String text, float size, int argb, int align);

    /** Draw a circle outline. */
    public static native void circleStroke(long ptr, float cx, float cy, float r, float width, int argb);

    /** Draw a filled triangle. */
    public static native void triangle(long ptr, float x1, float y1, float x2, float y2, float x3, float y3, int argb);

    /** Draw a triangle outline. */
    public static native void triangleStroke(long ptr, float x1, float y1, float x2, float y2, float x3, float y3, float width, int argb);

    /** Draw a filled arc (partial circle). Angles in degrees. */
    public static native void arc(long ptr, float cx, float cy, float r, float startDeg, float sweepDeg, int argb);

    /** Draw an arc outline. Angles in degrees. */
    public static native void arcStroke(long ptr, float cx, float cy, float r, float startDeg, float sweepDeg, float width, int argb);

    /** Push a translate transform. All subsequent draws are offset by (tx, ty). */
    public static native void pushTranslate(long ptr, float tx, float ty);

    /** Pop the last transform. */
    public static native void popTransform(long ptr);

    /** Push a clip rectangle (intersected with previous clip). */
    public static native void pushClip(long ptr, float x, float y, float w, float h);

    /** Pop the last clip. */
    public static native void popClip(long ptr);

    /** Load a custom font from TTF bytes. Size is atlas resolution (e.g. 48). */
    public static native void setFont(long ptr, byte[] ttfData, float sizePx);

    /** Draw a filled ellipse. */
    public static native void ellipse(long ptr, float cx, float cy, float rx, float ry, int argb);

    /** Draw an ellipse outline. */
    public static native void ellipseStroke(long ptr, float cx, float cy, float rx, float ry, float width, int argb);

    /** Draw a regular polygon (e.g. hexagon, pentagon). Rotation in degrees. */
    public static native void polygon(long ptr, float cx, float cy, float radius, int sides, float rotationDeg, int argb);

    /** Draw a soft shadow for a rounded rect. */
    public static native void shadowRoundedRect(long ptr, float x, float y, float w, float h, float radius, float blur, float offX, float offY, int argb);

    /** Set the user image from PNG/JPEG bytes for subsequent drawImage calls. */
    public static native void setImage(long ptr, byte[] pngBytes);

    /** Draw the currently bound user image. */
    public static native void drawImage(long ptr, float x, float y, float w, float h, int tintArgb);

    /** Draw the user image with rounded corners. */
    public static native void drawRoundedImage(long ptr, float x, float y, float w, float h, float radius, int tintArgb);

    // ---- Color helpers (pure Java) ----

    /** Linearly interpolate between two ARGB colors. t in 0..1. */
    public static int lerpColor(int c0, int c1, float t) {
        t = Math.max(0f, Math.min(1f, t));
        int a0 = (c0 >>> 24) & 0xFF, r0 = (c0 >> 16) & 0xFF, g0 = (c0 >> 8) & 0xFF, b0 = c0 & 0xFF;
        int a1 = (c1 >>> 24) & 0xFF, r1 = (c1 >> 16) & 0xFF, g1 = (c1 >> 8) & 0xFF, b1 = c1 & 0xFF;
        int a = (int)(a0 + (a1 - a0) * t), r = (int)(r0 + (r1 - r0) * t), g = (int)(g0 + (g1 - g0) * t), b = (int)(b0 + (b1 - b0) * t);
        return (a << 24) | (r << 16) | (g << 8) | b;
    }

    /** Darken a color by amount 0..1. */
    public static int darken(int argb, float amount) {
        float f = Math.max(0f, 1f - amount);
        int a = (argb >>> 24) & 0xFF, r = (argb >> 16) & 0xFF, g = (argb >> 8) & 0xFF, b = argb & 0xFF;
        return (a << 24) | ((int)(r * f) << 16) | ((int)(g * f) << 8) | (int)(b * f);
    }

    /** Rainbow color from hue 0..1. */
    public static int rainbow(float hue) {
        float h = (hue % 1f) * 6f;
        int i = (int)h;
        float f = h - i, q = 1f - f;
        float r=0,g=0,b=0;
        switch (i % 6) {
            case 0: r=1; g=f; break;
            case 1: r=q; g=1; break;
            case 2: g=1; b=f; break;
            case 3: g=q; b=1; break;
            case 4: r=f; b=1; break;
            default: r=1; b=q; break;
        }
        return (0xFF << 24) | ((int)(r*255) << 16) | ((int)(g*255) << 8) | (int)(b*255);
    }
}
