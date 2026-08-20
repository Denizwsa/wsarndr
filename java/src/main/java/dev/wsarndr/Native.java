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
}
