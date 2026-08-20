#version 450

layout(location = 0) in vec2 fragUV;
layout(location = 1) in vec4 fragColor;
layout(location = 2) flat in uint fragLight;

layout(set = 1, binding = 0) uniform sampler2D uAtlas;

layout(location = 0) out vec4 outColor;

void main() {
    float sky = float((fragLight >> 4) & 0xF) / 15.0;
    float block = float(fragLight & 0xF) / 15.0;
    float light = mix(sky, block, 0.5);
    outColor = vec4(fragColor.rgb * light, fragColor.a);
}