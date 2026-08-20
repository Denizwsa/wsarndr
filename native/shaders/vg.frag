#version 450

layout(location = 0) in vec2 fragUV;
layout(location = 1) in vec4 fragColor;
layout(location = 2) in vec4 fragParam;
layout(location = 3) in vec4 fragBounds;
layout(location = 4) in vec4 fragGradColor;
layout(location = 5) in vec2 fragGradFrom;
layout(location = 6) in vec2 fragGradTo;
layout(location = 7) in vec4 fragGradParams;
layout(location = 8) in vec4 fragClip;

layout(set = 0, binding = 0) uniform sampler2D uTexture;
layout(set = 0, binding = 1) uniform sampler2D uUserTex;

layout(push_constant) uniform Push {
    vec2 viewport;
    float scale;
    float _pad;
} pc;

layout(location = 0) out vec4 outColor;

const float FLAG_TEXTURE = 1.0;
const float FLAG_STROKE  = 2.0;
const float FLAG_LINE    = 4.0;
const float FLAG_ELLIPSE = 8.0;
const float FLAG_POLYGON = 16.0;
const float FLAG_USER_TEX = 32.0;

float sdRoundRect(vec2 p, vec2 b, float r) {
    vec2 q = abs(p) - b + vec2(r);
    return min(max(q.x, q.y), 0.0) + length(max(q, 0.0)) - r;
}

float sdEllipse(vec2 p, vec2 ab) {
    // Approximate ellipse SDF: scale to unit circle
    vec2 q = p / ab;
    float d = (length(q) - 1.0) * min(ab.x, ab.y);
    // Refine with one Newton iteration for better accuracy
    // (good enough for UI)
    return d;
}

float sdRegularPolygon(vec2 p, float r, float n, float rot) {
    float a = atan(p.y, p.x) + rot;
    float b = 6.28318530718 / n;
    return cos(floor(0.5 + a / b) * b - a) * length(p) - r;
}

void main() {
    vec2 pos = vec2(gl_FragCoord.x, pc.viewport.y - gl_FragCoord.y);

    // Clip rect discard (w < 0 means no clip)
    if (fragClip.z >= 0.0) {
        if (pos.x < fragClip.x || pos.y < fragClip.y ||
            pos.x > fragClip.x + fragClip.z || pos.y > fragClip.y + fragClip.w) {
            discard;
        }
    }

    vec4 color = fragColor;
    bool isText = mod(fragParam.w, 2.0) >= 1.0;

    if (isText) {
        float texAlpha = texture(uTexture, fragUV).r;
        outColor = vec4(color.rgb, color.a * texAlpha);
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

    float d;
    if (mod(flags, 32.0) >= 16.0) {
        // Polygon: radius = halfSize.x, sides = radius param, rot = grad y
        float n = max(radius, 3.0);
        float rot = fragGradParams.y;
        d = sdRegularPolygon(pos - center, halfSize.x, n, rot);
    } else if (mod(flags, 16.0) >= 8.0) {
        d = sdEllipse(pos - center, halfSize);
    } else {
        d = sdRoundRect(pos - center, halfSize, radius);
    }

    float alpha;
    float od = d;
    if (stroke <= 0.0) {
        alpha = clamp(0.5 - d / feather, 0.0, 1.0);
    } else {
        float strokeWidth = max(stroke, feather);
        float id = strokeWidth * 0.5 - abs(d);
        alpha = clamp(0.5 - id / feather, 0.0, 1.0);
    }
    if (od > feather) alpha = 0.0;

    bool isUserTex = mod(flags, 64.0) >= 32.0;
    if (isUserTex) {
        vec4 tex = texture(uUserTex, fragUV);
        // Tint with vertex color and apply rounded alpha
        vec3 rgb = tex.rgb * color.rgb;
        float a = tex.a * color.a * alpha;
        outColor = vec4(rgb, a);
        return;
    }

    int mode = int(fragGradParams.x);
    float t = 0.0;
    if (mode == 1) {
        vec2 dG = fragGradTo - fragGradFrom;
        float len = dot(dG, dG);
        t = len > 0.0 ? clamp(dot(pos - fragGradFrom, dG) / len, 0.0, 1.0) : 0.0;
        color = mix(color, fragGradColor, t);
    } else if (mode == 2) {
        vec2 p = pos - fragGradFrom;
        float r0 = fragGradParams.y;
        float r1 = length(fragGradTo - fragGradFrom);
        float dist = clamp((length(p) - r0) / max(r1 - r0, 1e-6), 0.0, 1.0);
        color = mix(color, fragGradColor, dist);
    }

    float a = alpha * color.a;
    outColor = vec4(color.rgb, a);
}
