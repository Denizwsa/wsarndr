#version 450

layout(location = 0) in vec2 inPos;
layout(location = 1) in vec2 inUV;
layout(location = 2) in vec4 inColor;
layout(location = 3) in vec4 inParam;   // x = stroke width, y = feather, z = corner radius, w = flags
layout(location = 4) in vec4 inBounds;  // x,y = min, z,w = max
layout(location = 5) in vec4 inGradColor;
layout(location = 6) in vec2 inGradFrom;
layout(location = 7) in vec2 inGradTo;
layout(location = 8) in vec4 inGradParams; // x = mode (0 solid,1 linear,2 radial), y = inner radius, z,w unused

layout(push_constant) uniform Push {
    vec2 viewport;
    float scale;
    float _pad;
} pc;

layout(location = 0) out vec2 fragUV;
layout(location = 1) out vec4 fragColor;
layout(location = 2) out vec4 fragParam;
layout(location = 3) out vec4 fragBounds;
layout(location = 4) out vec4 fragGradColor;
layout(location = 5) out vec2 fragGradFrom;
layout(location = 6) out vec2 fragGradTo;
layout(location = 7) out vec4 fragGradParams;

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
}