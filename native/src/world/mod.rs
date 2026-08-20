pub mod chunk;
pub mod frustum;
pub mod mesh;

pub use chunk::{build_chunk_mesh, ChunkMesh, Face};
pub use frustum::Frustum;

use ash::vk;
use ash::vk::Handle;
use bytemuck::{Pod, Zeroable};
use glam::Mat4;

use crate::vk::{Device, Instance, SharedDevice, SharedInstance};

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable, Debug)]
pub struct WorldVertex {
    pub pos: [f32; 3],
    pub uv: [f32; 2],
    pub color: [f32; 4],
    pub light: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable, Debug)]
pub struct WorldPushConstants {
    pub model: [[f32; 4]; 4],
    pub alpha: f32,
    pub _pad: [f32; 3],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable, Debug)]
pub struct ViewProjUniform {
    pub view_proj: [[f32; 4]; 4],
    pub cam_pos: [f32; 4],
}

pub struct WorldContext {
    pub device: SharedDevice,
    pub instance: SharedInstance,
    pub render_pass: vk::RenderPass,
    pub pipeline: vk::Pipeline,
    pub pipeline_layout: vk::PipelineLayout,
    pub descriptor_set_layout: vk::DescriptorSetLayout,
    pub descriptor_pool: vk::DescriptorPool,
    pub descriptor_set: vk::DescriptorSet,
    pub uniform_buffer: vk::Buffer,
    pub uniform_mem: vk::DeviceMemory,
    pub uniform: ViewProjUniform,
    pub chunk_meshes: Vec<ChunkMesh>,
    pub depth_format: vk::Format,
    pub depth_image: vk::Image,
    pub depth_mem: vk::DeviceMemory,
    pub depth_view: vk::ImageView,
}

impl WorldContext {
    pub fn new(
        device: SharedDevice,
        instance: SharedInstance,
        render_pass: vk::RenderPass,
    ) -> anyhow::Result<Self> {
        let (pipeline, layout, dsl, pool) =
            unsafe { create_pipeline(device.as_ref(), instance.as_ref(), render_pass)? };

        let (uniform_buffer, uniform_mem) = device.create_buffer(
            std::mem::size_of::<ViewProjUniform>() as vk::DeviceSize,
            vk::BufferUsageFlags::UNIFORM_BUFFER,
            vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT,
        )?;

        let descriptor_set = unsafe {
            device.device.allocate_descriptor_sets(
                &vk::DescriptorSetAllocateInfo::default()
                    .descriptor_pool(pool)
                    .set_layouts(std::slice::from_ref(&dsl)),
            )?[0]
        };

        Ok(Self {
            device,
            instance,
            render_pass,
            pipeline,
            pipeline_layout: layout,
            descriptor_set_layout: dsl,
            descriptor_pool: pool,
            descriptor_set,
            uniform_buffer,
            uniform_mem,
            uniform: ViewProjUniform {
                view_proj: Mat4::IDENTITY.to_cols_array_2d(),
                cam_pos: [0.0; 4],
            },
            chunk_meshes: Vec::new(),
            depth_format: vk::Format::D32_SFLOAT,
            depth_image: vk::Image::null(),
            depth_mem: vk::DeviceMemory::null(),
            depth_view: vk::ImageView::null(),
        })
    }

    pub fn set_camera(&mut self, view_proj: Mat4, cam_pos: [f32; 3]) {
        self.uniform = ViewProjUniform {
            view_proj: view_proj.to_cols_array_2d(),
            cam_pos: [cam_pos[0], cam_pos[1], cam_pos[2], 1.0],
        };
        let size = std::mem::size_of::<ViewProjUniform>() as vk::DeviceSize;
        unsafe {
            let ptr = self
                .device
                .device
                .map_memory(self.uniform_mem, 0, size, vk::MemoryMapFlags::empty())
                .expect("map uniform");
            std::ptr::copy_nonoverlapping(
                std::ptr::addr_of!(self.uniform) as *const u8,
                ptr as *mut u8,
                std::mem::size_of::<ViewProjUniform>(),
            );
            self.device.device.unmap_memory(self.uniform_mem);
        }
        self.update_descriptor();
    }

    fn update_descriptor(&self) {
        let info = vk::DescriptorBufferInfo {
            buffer: self.uniform_buffer,
            offset: 0,
            range: std::mem::size_of::<ViewProjUniform>() as vk::DeviceSize,
        };
        let write = vk::WriteDescriptorSet::default()
            .dst_set(self.descriptor_set)
            .dst_binding(0)
            .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
            .buffer_info(std::slice::from_ref(&info));
        unsafe {
            self.device
                .device
                .update_descriptor_sets(std::slice::from_ref(&write), &[])
        };
    }

    pub fn draw_chunk(&self, cmd: vk::CommandBuffer, mesh: &ChunkMesh, alpha: f32) {
        if mesh.vertices.is_empty() {
            return;
        }
        unsafe {
            self.device
                .device
                .cmd_bind_pipeline(cmd, vk::PipelineBindPoint::GRAPHICS, self.pipeline);
            self.device
                .device
                .cmd_bind_vertex_buffers(cmd, 0, &[mesh.vertex_buffer], &[0]);
            self.device.device.cmd_bind_descriptor_sets(
                cmd,
                vk::PipelineBindPoint::GRAPHICS,
                self.pipeline_layout,
                0,
                &[self.descriptor_set],
                &[],
            );

            let push = WorldPushConstants {
                model: Mat4::IDENTITY.to_cols_array_2d(),
                alpha,
                _pad: [0.0; 3],
            };
            self.device.device.cmd_push_constants(
                cmd,
                self.pipeline_layout,
                vk::ShaderStageFlags::VERTEX,
                0,
                bytemuck::bytes_of(&push),
            );

            self.device
                .device
                .cmd_draw(cmd, mesh.vertices.len() as u32, 1, 0, 0);
        }
    }

    pub fn create_depth_resources(&mut self, extent: vk::Extent2D) -> anyhow::Result<()> {
        if !self.depth_view.is_null() {
            unsafe {
                self.device.device.destroy_image_view(self.depth_view, None);
                self.device.device.destroy_image(self.depth_image, None);
                self.device.device.free_memory(self.depth_mem, None);
            }
        }
        let (img, mem) = self.device.create_image(
            extent.width,
            extent.height,
            self.depth_format,
            vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT,
            vk::MemoryPropertyFlags::DEVICE_LOCAL,
        )?;
        let view = unsafe {
            self.device.device.create_image_view(
                &vk::ImageViewCreateInfo::default()
                    .image(img)
                    .view_type(vk::ImageViewType::TYPE_2D)
                    .format(self.depth_format)
                    .subresource_range(
                        vk::ImageSubresourceRange::default()
                            .aspect_mask(vk::ImageAspectFlags::DEPTH)
                            .level_count(1)
                            .layer_count(1),
                    ),
                None,
            )?
        };
        self.depth_image = img;
        self.depth_mem = mem;
        self.depth_view = view;
        Ok(())
    }
}

unsafe fn create_pipeline(
    device: &Device,
    _instance: &Instance,
    render_pass: vk::RenderPass,
) -> anyhow::Result<(
    vk::Pipeline,
    vk::PipelineLayout,
    vk::DescriptorSetLayout,
    vk::DescriptorPool,
)> {
    let d = &device.device;

    let vert_spv = include_spirv("world_vert")?;
    let frag_spv = include_spirv("world_frag")?;
    let vert_mod =
        d.create_shader_module(&vk::ShaderModuleCreateInfo::default().code(&vert_spv), None)?;
    let frag_mod =
        d.create_shader_module(&vk::ShaderModuleCreateInfo::default().code(&frag_spv), None)?;

    let main_cstr = std::ffi::CString::new("main").unwrap();
    let stages = [
        vk::PipelineShaderStageCreateInfo::default()
            .stage(vk::ShaderStageFlags::VERTEX)
            .module(vert_mod)
            .name(&main_cstr),
        vk::PipelineShaderStageCreateInfo::default()
            .stage(vk::ShaderStageFlags::FRAGMENT)
            .module(frag_mod)
            .name(&main_cstr),
    ];

    let binding = vk::VertexInputBindingDescription::default()
        .binding(0)
        .stride(std::mem::size_of::<WorldVertex>() as u32)
        .input_rate(vk::VertexInputRate::VERTEX);

    let attrs = [
        vertex_attr(0, vk::Format::R32G32B32_SFLOAT, 0),
        vertex_attr(1, vk::Format::R32G32_SFLOAT, 12),
        vertex_attr(2, vk::Format::R32G32B32A32_SFLOAT, 20),
        vertex_attr(3, vk::Format::R32_UINT, 36),
    ];

    let vertex_input = vk::PipelineVertexInputStateCreateInfo::default()
        .vertex_binding_descriptions(std::slice::from_ref(&binding))
        .vertex_attribute_descriptions(&attrs);

    let input_asm = vk::PipelineInputAssemblyStateCreateInfo::default()
        .topology(vk::PrimitiveTopology::TRIANGLE_LIST);

    let viewport_state = vk::PipelineViewportStateCreateInfo::default()
        .viewport_count(1)
        .scissor_count(1);

    let raster = vk::PipelineRasterizationStateCreateInfo::default()
        .polygon_mode(vk::PolygonMode::FILL)
        .line_width(1.0)
        .cull_mode(vk::CullModeFlags::BACK)
        .front_face(vk::FrontFace::COUNTER_CLOCKWISE);

    let multisample = vk::PipelineMultisampleStateCreateInfo::default()
        .rasterization_samples(vk::SampleCountFlags::TYPE_1);

    let depth_stencil = vk::PipelineDepthStencilStateCreateInfo::default()
        .depth_test_enable(true)
        .depth_write_enable(true)
        .depth_compare_op(vk::CompareOp::LESS_OR_EQUAL);

    let color_attachment = vk::PipelineColorBlendAttachmentState::default()
        .blend_enable(true)
        .src_color_blend_factor(vk::BlendFactor::SRC_ALPHA)
        .dst_color_blend_factor(vk::BlendFactor::ONE_MINUS_SRC_ALPHA)
        .color_blend_op(vk::BlendOp::ADD)
        .src_alpha_blend_factor(vk::BlendFactor::ONE)
        .dst_alpha_blend_factor(vk::BlendFactor::ONE_MINUS_SRC_ALPHA)
        .alpha_blend_op(vk::BlendOp::ADD)
        .color_write_mask(vk::ColorComponentFlags::RGBA);

    let color_blend = vk::PipelineColorBlendStateCreateInfo::default()
        .attachments(std::slice::from_ref(&color_attachment));

    let dsl = d.create_descriptor_set_layout(
        &vk::DescriptorSetLayoutCreateInfo::default().bindings(&[vk::DescriptorSetLayoutBinding::default()
            .binding(0)
            .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
            .descriptor_count(1)
            .stage_flags(vk::ShaderStageFlags::VERTEX)]),
        None,
    )?;

    let pool = d.create_descriptor_pool(
        &vk::DescriptorPoolCreateInfo::default()
            .max_sets(1)
            .pool_sizes(&[vk::DescriptorPoolSize {
                ty: vk::DescriptorType::UNIFORM_BUFFER,
                descriptor_count: 1,
            }]),
        None,
    )?;

    let push_range = vk::PushConstantRange {
        stage_flags: vk::ShaderStageFlags::VERTEX,
        offset: 0,
        size: std::mem::size_of::<WorldPushConstants>() as u32,
    };

    let pipeline_layout = d.create_pipeline_layout(
        &vk::PipelineLayoutCreateInfo::default()
            .set_layouts(std::slice::from_ref(&dsl))
            .push_constant_ranges(std::slice::from_ref(&push_range)),
        None,
    )?;

    let pipeline = d
        .create_graphics_pipelines(
            vk::PipelineCache::null(),
            &[vk::GraphicsPipelineCreateInfo::default()
                .stages(&stages)
                .vertex_input_state(&vertex_input)
                .input_assembly_state(&input_asm)
                .viewport_state(&viewport_state)
                .rasterization_state(&raster)
                .multisample_state(&multisample)
                .depth_stencil_state(&depth_stencil)
                .color_blend_state(&color_blend)
                .layout(pipeline_layout)
                .render_pass(render_pass)
                .subpass(0)],
            None,
        )
        .map_err(|(_, e)| e)?
        .remove(0);

    d.destroy_shader_module(vert_mod, None);
    d.destroy_shader_module(frag_mod, None);

    Ok((pipeline, pipeline_layout, dsl, pool))
}

fn vertex_attr(location: u32, format: vk::Format, offset: u32) -> vk::VertexInputAttributeDescription {
    vk::VertexInputAttributeDescription {
        location,
        binding: 0,
        format,
        offset,
    }
}

fn include_spirv(name: &str) -> anyhow::Result<Vec<u32>> {
    let dir = env!("WSARNDR_SHADERS");
    let path = std::path::Path::new(dir).join(format!("{}.spv", name));
    let bytes = std::fs::read(&path)?;
    let words: Vec<u32> = bytes
        .chunks_exact(4)
        .map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect();
    Ok(words)
}

impl Drop for WorldContext {
    fn drop(&mut self) {
        unsafe {
            self.device.device.device_wait_idle().ok();
            if !self.depth_view.is_null() {
                self.device.device.destroy_image_view(self.depth_view, None);
                self.device.device.destroy_image(self.depth_image, None);
                self.device.device.free_memory(self.depth_mem, None);
            }
            self.device.device.destroy_buffer(self.uniform_buffer, None);
            self.device.device.free_memory(self.uniform_mem, None);
            self.device.device.destroy_descriptor_pool(self.descriptor_pool, None);
            self.device
                .device
                .destroy_descriptor_set_layout(self.descriptor_set_layout, None);
            self.device.device.destroy_pipeline(self.pipeline, None);
            self.device
                .device
                .destroy_pipeline_layout(self.pipeline_layout, None);
        }
    }
}