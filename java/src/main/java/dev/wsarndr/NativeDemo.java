package dev.wsarndr;

import org.lwjgl.glfw.GLFW;
import org.lwjgl.opengl.GL;
import org.lwjgl.system.MemoryUtil;

import static org.lwjgl.glfw.GLFW.*;
import static org.lwjgl.opengl.GL11.*;

/**
 * End-to-end JNI bridge demo:
 * Opens a GLFW window, passes X11/Wayland handles to native,
 * queues 2D commands each frame and renders via endFrame.
 */
public class NativeDemo {

    public static void main(String[] args) throws Exception {
        Native.load(System.getProperty("wsarndr.lib", "/home/deniz/Projects/wsarndr/target/debug/libwsarndr.so"));

        if (!glfwInit()) {
            throw new IllegalStateException("glfwInit failed");
        }

        glfwWindowHint(GLFW_VISIBLE, GLFW_FALSE);
        glfwWindowHint(GLFW_RESIZABLE, GLFW_TRUE);
        glfwWindowHint(GLFW_CONTEXT_VERSION_MAJOR, 3);
        glfwWindowHint(GLFW_CONTEXT_VERSION_MINOR, 2);
        glfwWindowHint(GLFW_OPENGL_PROFILE, GLFW_OPENGL_CORE_PROFILE);
        glfwWindowHint(GLFW_OPENGL_FORWARD_COMPAT, GLFW_TRUE);

        long win = glfwCreateWindow(960, 540, "wsarndr JNI demo", 0, 0);
        if (win == 0) {
            throw new IllegalStateException("glfwCreateWindow failed");
        }
        glfwShowWindow(win);
        glfwMakeContextCurrent(win);
        GL.createCapabilities();

        // --- detect platform handles ---
        int winType;
        long winHandle;
        long dispHandle;
        String platform = System.getProperty("wsarndr.platform", "").toLowerCase();
        if (platform.contains("wayland")) {
            winType = Native.WIN_WAYLAND;
            winHandle = org.lwjgl.glfw.GLFWNativeWayland.glfwGetWaylandWindow(win);
            dispHandle = org.lwjgl.glfw.GLFWNativeWayland.glfwGetWaylandDisplay();
        } else {
            winType = Native.WIN_XLIB;
            winHandle = org.lwjgl.glfw.GLFWNativeX11.glfwGetX11Window(win);
            dispHandle = org.lwjgl.glfw.GLFWNativeX11.glfwGetX11Display();
        }
        System.out.println("window: type=" + winType + " win=0x" + Long.toHexString(winHandle)
                + " disp=0x" + Long.toHexString(dispHandle));

        long ptr = Native.create(Native.BACKEND_VULKAN, winType, winHandle, dispHandle);
        if (ptr == 0) {
            throw new IllegalStateException("wsarndr create failed");
        }
        System.out.println("wsarndr created: ptr=0x" + Long.toHexString(ptr));

        glfwSwapInterval(0);
        long start = System.nanoTime();
        int frames = 0;

        while (!glfwWindowShouldClose(win)) {
            int[] w = new int[1], h = new int[1];
            glfwGetFramebufferSize(win, w, h);
            glViewport(0, 0, w[0], h[0]);
            glClearColor(0.1f, 0.1f, 0.1f, 1f);
            glClear(GL_COLOR_BUFFER_BIT);
            glfwSwapBuffers(win);

            Native.beginFrame(ptr);
            drawUi(ptr, w[0], h[0]);
            Native.endFrame(ptr);
            glfwPollEvents();
            frames++;
            if (System.nanoTime() - start > 5_000_000_000L) {
                System.out.println("fps=" + (frames / 5));
                frames = 0;
                start = System.nanoTime();
            }
        }

        Native.destroy(ptr);
        glfwDestroyWindow(win);
        glfwTerminate();
    }

    private static void drawUi(long p, int w, int h) {
        int dark = 0xFF141414;
        int panel = 0xFF1E1E28;
        int accent = 0xFF4F8CFF;
        int white = 0xFFFFFFFF;
        int gray = 0xFFAAAAAA;

        // ClickGUI panel
        Native.roundedRect(p, 20, 20, 300, 380, 8, panel);
        Native.rectStroke(p, 20, 20, 300, 380, 2, accent);
        Native.text(p, 34, 34, "wsarndr ClickGUI", 18, white);
        Native.line(p, 24, 62, 316, 62, 1, 0xFF3A3A4A);
        Native.text(p, 34, 74, "Module A  [ON]", 14, accent);
        Native.text(p, 34, 96, "Module B  [OFF]", 14, gray);
        Native.text(p, 34, 118, "Module C  [ON]", 14, accent);
        Native.roundedRect(p, 34, 140, 160, 4, 2, accent);

        // ArrayList
        Native.roundedRect(p, w - 210, 20, 190, 110, 6, dark);
        Native.text(p, w - 198, 28, "ArrayList", 14, white);
        Native.text(p, w - 198, 50, "KillAura", 12, accent);
        Native.text(p, w - 198, 70, "Speed", 12, gray);
        Native.text(p, w - 198, 90, "AutoClicker", 12, accent);

        // TargetHUD
        Native.circle(p, w / 2f - 190, h - 150, 34, 0xFF2E2E3E);
        Native.circle(p, w / 2f - 190, h - 150, 30, 0xFF4F8CFF);
        Native.text(p, w / 2f - 140, h - 170, "Steve", 18, white);
        Native.roundedRect(p, w / 2f - 140, h - 148, 120, 6, 3, 0xFF3A3A4A);
        Native.roundedRect(p, w / 2f - 140, h - 148, 72, 6, 3, 0xFF3FBF4F);

        // gradient bar
        Native.linearGradient(p, 0, h - 4, w, 4, 0, 0, h - 4, w, h - 4, 0xFF4F8CFF, 0xFFBF3F8C);
    }
}
