#version 450

layout(location = 0) in vec3 inPos;
layout(location = 1) in vec2 inUV;
layout(location = 2) in vec4 inColor;
layout(location = 3) in uint inLight;

layout(set = 0, binding = 0) uniform ViewProj {
    mat4 viewProj;
    vec4 camPos;
} vp;

layout(push_constant) uniform Push {
    mat4 model;
    float alpha;
    float _pad0;
    float _pad1;
    float _pad2;
} pc;

layout(location = 0) out vec2 fragUV;
layout(location = 1) out vec4 fragColor;
layout(location = 2) flat out uint fragLight;

void main() {
    gl_Position = vp.viewProj * pc.model * vec4(inPos, 1.0);
    fragUV = inUV;
    fragColor = inColor * pc.alpha;
    fragLight = inLight;
}