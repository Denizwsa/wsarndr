use std::sync::Arc;

use ash::ext::debug_utils;
use ash::khr::{surface, swapchain};
use ash::vk;
use ash::Device as AshDevice;
use raw_window_handle::{RawDisplayHandle, RawWindowHandle};

// Window size notification from JNI/demo (used during swapchain creation)
thread_local! {
    pub static REQUESTED_EXTENT: std::cell::RefCell<Option<vk::Extent2D>> = const { std::cell::RefCell::new(None) };
}

pub struct Instance {
    pub entry: ash::Entry,
    pub instance: ash::Instance,
    pub surface_loader: surface::Instance,
    pub debug_utils: Option<debug_utils::Instance>,
    pub debug_messenger: Option<vk::DebugUtilsMessengerEXT>,
}

pub struct Device {
    pub physical_device: vk::PhysicalDevice,
    pub device: AshDevice,
    pub queue_family_index: u32,
    pub graphics_queue: vk::Queue,
    pub present_queue: vk::Queue,
    pub swapchain_loader: swapchain::Device,
    pub graphics_mem_props: vk::PhysicalDeviceMemoryProperties,
}

pub struct Swapchain {
    pub swapchain: vk::SwapchainKHR,
    pub loader: swapchain::Device,
    pub images: Vec<vk::Image>,
    pub image_views: Vec<vk::ImageView>,
    pub format: vk::Format,
    pub extent: vk::Extent2D,
}

impl Instance {
    pub fn new(
        entry: ash::Entry,
        display_handle: RawDisplayHandle,
        window_handle: RawWindowHandle,
        app_name: &str,
        enable_validation: bool,
    ) -> anyhow::Result<(Self, vk::SurfaceKHR)> {
        let app_name_c = std::ffi::CString::new(app_name).unwrap();
        let app_info = vk::ApplicationInfo::default()
            .api_version(vk::API_VERSION_1_3)
            .application_name(app_name_c.as_c_str());

        let layer_names = if enable_validation {
            // Check if validation layer is available
            let available: Vec<String> = unsafe { entry.enumerate_instance_layer_properties() }
                .unwrap_or_default()
                .iter()
                .map(|l| l.layer_name_as_c_str().map(|s| s.to_string_lossy().to_string()).unwrap_or_default())
                .collect();
            log::info!("available layers: {:?}", available);
            if available.iter().any(|l| l == "VK_LAYER_KHRONOS_validation") {
                vec![std::ffi::CString::new("VK_LAYER_KHRONOS_validation").unwrap()]
            } else {
                log::warn!("VK_LAYER_KHRONOS_validation not installed, running without validation");
                vec![]
            }
        } else {
            vec![]
        };

        let extension_names = ash_window::enumerate_required_extensions(display_handle)?;
        let mut ext_strings = Vec::new();
        let mut enabled_exts: Vec<*const std::ffi::c_char> = Vec::new();
        for e in extension_names {
            let s = unsafe { std::ffi::CStr::from_ptr(*e) }
                .to_string_lossy()
                .to_string();
            ext_strings.push(std::ffi::CString::new(s).unwrap());
            enabled_exts.push(ext_strings.last().unwrap().as_ptr());
        }
        if enable_validation {
            ext_strings.push(std::ffi::CString::new("VK_EXT_debug_utils").unwrap());
            enabled_exts.push(ext_strings.last().unwrap().as_ptr());
        }

        let layer_cstrs: Vec<std::ffi::CString> = layer_names;
        let layer_ptrs: Vec<*const std::ffi::c_char> =
            layer_cstrs.iter().map(|c| c.as_ptr()).collect();

        let create_info = vk::InstanceCreateInfo::default()
            .application_info(&app_info)
            .enabled_layer_names(&layer_ptrs)
            .enabled_extension_names(&enabled_exts);

        let instance = unsafe { entry.create_instance(&create_info, None)? };

        let surface_loader = surface::Instance::new(&entry, &instance);

        let mut debug_utils = None;
        let mut debug_messenger = None;
        if enable_validation {
            let loader = debug_utils::Instance::new(&entry, &instance);
            let info = vk::DebugUtilsMessengerCreateInfoEXT::default()
                .message_severity(
                    vk::DebugUtilsMessageSeverityFlagsEXT::WARNING
                        | vk::DebugUtilsMessageSeverityFlagsEXT::ERROR,
                )
                .message_type(
                    vk::DebugUtilsMessageTypeFlagsEXT::GENERAL
                        | vk::DebugUtilsMessageTypeFlagsEXT::VALIDATION
                        | vk::DebugUtilsMessageTypeFlagsEXT::PERFORMANCE,
                )
                .pfn_user_callback(Some(debug_callback));
            let messenger = unsafe { loader.create_debug_utils_messenger(&info, None)? };
            debug_utils = Some(loader);
            debug_messenger = Some(messenger);
        }

        let surface = unsafe {
            ash_window::create_surface(&entry, &instance, display_handle, window_handle, None)?
        };

        Ok((
            Self {
                entry,
                instance,
                surface_loader,
                debug_utils,
                debug_messenger,
            },
            surface,
        ))
    }

    pub fn pick_physical_device(
        &self,
        surface: vk::SurfaceKHR,
    ) -> anyhow::Result<(vk::PhysicalDevice, u32, u32, vk::SurfaceCapabilitiesKHR)> {
        let devices = unsafe { self.instance.enumerate_physical_devices()? };
        let (mut chosen, mut chosen_gfx, mut chosen_present, mut chosen_caps) =
            (None, None, None, None);

        for device in devices {
            let props = unsafe { self.instance.get_physical_device_properties(device) };
            let name_cstr = unsafe { std::ffi::CStr::from_ptr(props.device_name.as_ptr()) };
            log::info!("GPU: {}", name_cstr.to_string_lossy());
            let qf = unsafe { self.instance.get_physical_device_queue_family_properties(device) };
            let mut gfx = None;
            let mut present = None;
            for (i, f) in qf.iter().enumerate() {
                let supports_present = unsafe {
                    self.surface_loader
                        .get_physical_device_surface_support(device, i as u32, surface)?
                };
                if f.queue_flags.contains(vk::QueueFlags::GRAPHICS) && gfx.is_none() {
                    gfx = Some(i as u32);
                }
                if supports_present && present.is_none() {
                    present = Some(i as u32);
                }
                if gfx.is_some() && present.is_some() {
                    break;
                }
            }
            if let (Some(g), Some(p)) = (gfx, present) {
                let caps = unsafe {
                    self.surface_loader
                        .get_physical_device_surface_capabilities(device, surface)?
                };
                chosen = Some(device);
                chosen_gfx = Some(g);
                chosen_present = Some(p);
                chosen_caps = Some(caps);
                break;
            }
        }

        Ok((
            chosen.ok_or_else(|| anyhow::anyhow!("no suitable GPU"))?,
            chosen_gfx.unwrap(),
            chosen_present.unwrap(),
            chosen_caps.unwrap(),
        ))
    }

    pub fn create_device(
        &self,
        physical: vk::PhysicalDevice,
        gfx_queue: u32,
        present_queue: u32,
    ) -> anyhow::Result<Device> {
        let features = vk::PhysicalDeviceFeatures::default();
        let queues = [gfx_queue, present_queue];
        let unique_queues: Vec<u32> = queues
            .iter()
            .copied()
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect();
        let mut queue_infos = Vec::new();
        for q in &unique_queues {
            let info = vk::DeviceQueueCreateInfo::default()
                .queue_family_index(*q)
                .queue_priorities(&[1.0]);
            queue_infos.push(info);
        }

        let device_extensions = [std::ffi::CString::new("VK_KHR_swapchain").unwrap()];
        let ext_ptrs: Vec<*const std::ffi::c_char> =
            device_extensions.iter().map(|c| c.as_ptr()).collect();

        let create_info = vk::DeviceCreateInfo::default()
            .queue_create_infos(&queue_infos)
            .enabled_features(&features)
            .enabled_extension_names(&ext_ptrs);

        let device = unsafe { self.instance.create_device(physical, &create_info, None)? };
        let graphics_queue = unsafe { device.get_device_queue(gfx_queue, 0) };
        let present_queue = unsafe { device.get_device_queue(present_queue, 0) };
        let swapchain_loader = swapchain::Device::new(&self.instance, &device);
        let graphics_mem_props = unsafe {
            self.instance
                .get_physical_device_memory_properties(physical)
        };

        Ok(Device {
            physical_device: physical,
            device,
            queue_family_index: gfx_queue,
            graphics_queue,
            present_queue,
            swapchain_loader,
            graphics_mem_props,
        })
    }

    pub unsafe fn destroy(&self) {
        if let (Some(m), Some(l)) = (self.debug_messenger, &self.debug_utils) {
            l.destroy_debug_utils_messenger(m, None);
        }
        unsafe { self.instance.destroy_instance(None) };
    }
}

impl Device {
    pub fn create_render_pass(&self, format: vk::Format) -> anyhow::Result<vk::RenderPass> {
        let color_attachment = vk::AttachmentDescription::default()
            .format(format)
            .samples(vk::SampleCountFlags::TYPE_1)
            .load_op(vk::AttachmentLoadOp::CLEAR)
            .store_op(vk::AttachmentStoreOp::STORE)
            .stencil_load_op(vk::AttachmentLoadOp::DONT_CARE)
            .stencil_store_op(vk::AttachmentStoreOp::DONT_CARE)
            .initial_layout(vk::ImageLayout::UNDEFINED)
            .final_layout(vk::ImageLayout::PRESENT_SRC_KHR);

        let color_ref = vk::AttachmentReference {
            attachment: 0,
            layout: vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
        };

        let subpass = vk::SubpassDescription::default()
            .color_attachments(std::slice::from_ref(&color_ref));

        let dependency = vk::SubpassDependency::default()
            .src_subpass(vk::SUBPASS_EXTERNAL)
            .dst_subpass(0)
            .src_stage_mask(vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT)
            .src_access_mask(vk::AccessFlags::empty())
            .dst_stage_mask(vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT)
            .dst_access_mask(vk::AccessFlags::COLOR_ATTACHMENT_WRITE);

        let create_info = vk::RenderPassCreateInfo::default()
            .attachments(std::slice::from_ref(&color_attachment))
            .subpasses(std::slice::from_ref(&subpass))
            .dependencies(std::slice::from_ref(&dependency));

        Ok(unsafe { self.device.create_render_pass(&create_info, None)? })
    }

    pub fn create_swapchain(
        &self,
        instance: &Instance,
        surface: vk::SurfaceKHR,
        caps: &vk::SurfaceCapabilitiesKHR,
        old_swapchain: Option<vk::SwapchainKHR>,
        override_extent: Option<vk::Extent2D>,
    ) -> anyhow::Result<Swapchain> {
        let formats = unsafe {
            instance
                .surface_loader
                .get_physical_device_surface_formats(self.physical_device, surface)?
        };
        let present_modes = unsafe {
            instance
                .surface_loader
                .get_physical_device_surface_present_modes(self.physical_device, surface)?
        };

        let format = formats
            .iter()
            .find(|f| {
                f.format == vk::Format::B8G8R8A8_UNORM
                    && f.color_space == vk::ColorSpaceKHR::SRGB_NONLINEAR
            })
            .unwrap_or(&formats[0]);

        let present_mode = if present_modes.contains(&vk::PresentModeKHR::MAILBOX) {
            vk::PresentModeKHR::MAILBOX
        } else {
            vk::PresentModeKHR::FIFO
        };

        let extent = if let Some(e) = override_extent {
            e
        } else {
            let e = caps.current_extent;
            if e.width == u32::MAX || e.height == u32::MAX {
                crate::vk::REQUESTED_EXTENT.with(|r| r.borrow().clone()).unwrap_or(vk::Extent2D {
                    width: 1280,
                    height: 720,
                })
            } else {
                e
            }
        };
        let image_count = (caps.min_image_count + 1)
            .min(if caps.max_image_count == 0 {
                u32::MAX
            } else {
                caps.max_image_count
            });

        log::info!(
            "swapchain: extent={}x{} min_img={} max_img={} formats={} modes={} transform={:?} composite={:?}",
            extent.width,
            extent.height,
            caps.min_image_count,
            caps.max_image_count,
            formats.len(),
            present_modes.len(),
            caps.current_transform,
            caps.supported_composite_alpha,
        );

        let composite_alpha = if caps
            .supported_composite_alpha
            .contains(vk::CompositeAlphaFlagsKHR::OPAQUE)
        {
            vk::CompositeAlphaFlagsKHR::OPAQUE
        } else {
            vk::CompositeAlphaFlagsKHR::INHERIT
        };

        let create_info = vk::SwapchainCreateInfoKHR::default()
            .surface(surface)
            .min_image_count(image_count)
            .image_format(format.format)
            .image_color_space(format.color_space)
            .image_extent(extent)
            .image_array_layers(1)
            .image_usage(vk::ImageUsageFlags::COLOR_ATTACHMENT)
            .image_sharing_mode(vk::SharingMode::EXCLUSIVE)
            .pre_transform(vk::SurfaceTransformFlagsKHR::IDENTITY)
            .composite_alpha(composite_alpha)
            .present_mode(present_mode)
            .clipped(true)
            .old_swapchain(old_swapchain.unwrap_or(vk::SwapchainKHR::null()));

        let swapchain =
            unsafe { self.swapchain_loader.create_swapchain(&create_info, None)? };

        let images = unsafe { self.swapchain_loader.get_swapchain_images(swapchain)? };
        let mut image_views = Vec::new();
        for img in &images {
            let view = unsafe {
                self.device.create_image_view(
                    &vk::ImageViewCreateInfo::default()
                        .image(*img)
                        .view_type(vk::ImageViewType::TYPE_2D)
                        .format(format.format)
                        .subresource_range(
                            vk::ImageSubresourceRange::default()
                                .aspect_mask(vk::ImageAspectFlags::COLOR)
                                .level_count(1)
                                .layer_count(1),
                        ),
                    None,
                )?
            };
            image_views.push(view);
        }

        Ok(Swapchain {
            swapchain,
            loader: self.swapchain_loader.clone(),
            images,
            image_views,
            format: format.format,
            extent,
        })
    }

    pub fn create_framebuffers(
        &self,
        render_pass: vk::RenderPass,
        image_views: &[vk::ImageView],
        extent: vk::Extent2D,
    ) -> anyhow::Result<Vec<vk::Framebuffer>> {
        let mut framebuffers = Vec::new();
        for view in image_views {
            let fb = unsafe {
                self.device.create_framebuffer(
                    &vk::FramebufferCreateInfo::default()
                        .render_pass(render_pass)
                        .attachments(std::slice::from_ref(view))
                        .width(extent.width)
                        .height(extent.height)
                        .layers(1),
                    None,
                )?
            };
            framebuffers.push(fb);
        }
        Ok(framebuffers)
    }

    pub fn create_command_pool(&self) -> anyhow::Result<vk::CommandPool> {
        Ok(unsafe {
            self.device.create_command_pool(
                &vk::CommandPoolCreateInfo::default()
                    .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER)
                    .queue_family_index(self.queue_family_index),
                None,
            )?
        })
    }

    pub fn create_command_buffers(
        &self,
        pool: vk::CommandPool,
        count: u32,
    ) -> anyhow::Result<Vec<vk::CommandBuffer>> {
        Ok(unsafe {
            self.device.allocate_command_buffers(
                &vk::CommandBufferAllocateInfo::default()
                    .command_pool(pool)
                    .level(vk::CommandBufferLevel::PRIMARY)
                    .command_buffer_count(count),
            )?
        })
    }

    pub fn create_semaphores(&self) -> anyhow::Result<(vk::Semaphore, vk::Semaphore, vk::Fence)> {
        let image_available =
            unsafe { self.device.create_semaphore(&vk::SemaphoreCreateInfo::default(), None)? };
        let render_finished =
            unsafe { self.device.create_semaphore(&vk::SemaphoreCreateInfo::default(), None)? };
        let in_flight = unsafe {
            self.device.create_fence(
                &vk::FenceCreateInfo::default().flags(vk::FenceCreateFlags::SIGNALED),
                None,
            )?
        };
        Ok((image_available, render_finished, in_flight))
    }

    pub fn alloc_memory(
        &self,
        reqs: vk::MemoryRequirements,
        properties: vk::MemoryPropertyFlags,
    ) -> anyhow::Result<vk::DeviceMemory> {
        let mem_type = self
            .graphics_mem_props
            .memory_types
            .iter()
            .enumerate()
            .find(|(i, t)| {
                reqs.memory_type_bits & (1 << i) != 0
                    && t.property_flags.contains(properties)
            })
            .map(|(i, _)| i as u32)
            .ok_or_else(|| anyhow::anyhow!("no suitable memory type"))?;

        let alloc = vk::MemoryAllocateInfo::default()
            .allocation_size(reqs.size)
            .memory_type_index(mem_type);
        Ok(unsafe { self.device.allocate_memory(&alloc, None)? })
    }

    pub fn create_buffer(
        &self,
        size: vk::DeviceSize,
        usage: vk::BufferUsageFlags,
        properties: vk::MemoryPropertyFlags,
    ) -> anyhow::Result<(vk::Buffer, vk::DeviceMemory)> {
        let buffer = unsafe {
            self.device.create_buffer(
                &vk::BufferCreateInfo::default()
                    .size(size)
                    .usage(usage)
                    .sharing_mode(vk::SharingMode::EXCLUSIVE),
                None,
            )?
        };
        let reqs = unsafe { self.device.get_buffer_memory_requirements(buffer) };
        let mem = self.alloc_memory(reqs, properties)?;
        unsafe { self.device.bind_buffer_memory(buffer, mem, 0)? };
        Ok((buffer, mem))
    }

    pub fn create_image(
        &self,
        width: u32,
        height: u32,
        format: vk::Format,
        usage: vk::ImageUsageFlags,
        properties: vk::MemoryPropertyFlags,
    ) -> anyhow::Result<(vk::Image, vk::DeviceMemory)> {
        let image = unsafe {
            self.device.create_image(
                &vk::ImageCreateInfo::default()
                    .image_type(vk::ImageType::TYPE_2D)
                    .format(format)
                    .extent(vk::Extent3D {
                        width,
                        height,
                        depth: 1,
                    })
                    .mip_levels(1)
                    .array_layers(1)
                    .samples(vk::SampleCountFlags::TYPE_1)
                    .tiling(vk::ImageTiling::OPTIMAL)
                    .usage(usage)
                    .sharing_mode(vk::SharingMode::EXCLUSIVE)
                    .initial_layout(vk::ImageLayout::UNDEFINED),
                None,
            )?
        };
        let reqs = unsafe { self.device.get_image_memory_requirements(image) };
        let mem = self.alloc_memory(reqs, properties)?;
        unsafe { self.device.bind_image_memory(image, mem, 0)? };
        Ok((image, mem))
    }

    pub fn transition_image_layout(
        &self,
        cmd: vk::CommandBuffer,
        image: vk::Image,
        old_layout: vk::ImageLayout,
        new_layout: vk::ImageLayout,
    ) -> anyhow::Result<()> {
        let mut barrier = vk::ImageMemoryBarrier::default()
            .old_layout(old_layout)
            .new_layout(new_layout)
            .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .image(image)
            .subresource_range(
                vk::ImageSubresourceRange::default()
                    .aspect_mask(vk::ImageAspectFlags::COLOR)
                    .level_count(1)
                    .layer_count(1),
            );

        let (src_mask, dst_mask) = match (old_layout, new_layout) {
            (vk::ImageLayout::UNDEFINED, vk::ImageLayout::TRANSFER_DST_OPTIMAL) => {
                barrier = barrier
                    .src_access_mask(vk::AccessFlags::empty())
                    .dst_access_mask(vk::AccessFlags::TRANSFER_WRITE);
                (vk::PipelineStageFlags::TOP_OF_PIPE, vk::PipelineStageFlags::TRANSFER)
            }
            (vk::ImageLayout::TRANSFER_DST_OPTIMAL, vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL) => {
                barrier = barrier
                    .src_access_mask(vk::AccessFlags::TRANSFER_WRITE)
                    .dst_access_mask(vk::AccessFlags::SHADER_READ);
                (
                    vk::PipelineStageFlags::TRANSFER,
                    vk::PipelineStageFlags::FRAGMENT_SHADER,
                )
            }
            (from, to) => anyhow::bail!("unsupported layout transition {:?} -> {:?}", from, to),
        };

        unsafe {
            self.device.cmd_pipeline_barrier(
                cmd,
                src_mask,
                dst_mask,
                vk::DependencyFlags::empty(),
                &[],
                &[],
                std::slice::from_ref(&barrier),
            );
        }
        Ok(())
    }

    pub fn copy_buffer_to_image(
        &self,
        cmd: vk::CommandBuffer,
        buffer: vk::Buffer,
        image: vk::Image,
        width: u32,
        height: u32,
    ) {
        let region = vk::BufferImageCopy::default()
            .image_subresource(
                vk::ImageSubresourceLayers::default()
                    .aspect_mask(vk::ImageAspectFlags::COLOR)
                    .layer_count(1),
            )
            .image_extent(vk::Extent3D {
                width,
                height,
                depth: 1,
            });
        unsafe {
            self.device
                .cmd_copy_buffer_to_image(cmd, buffer, image, vk::ImageLayout::TRANSFER_DST_OPTIMAL, std::slice::from_ref(&region));
        }
    }

    pub fn single_time_command<F: FnOnce(vk::CommandBuffer)>(
        &self,
        pool: vk::CommandPool,
        f: F,
    ) -> anyhow::Result<()> {
        let cmd = unsafe {
            self.device.allocate_command_buffers(
                &vk::CommandBufferAllocateInfo::default()
                    .command_pool(pool)
                    .level(vk::CommandBufferLevel::PRIMARY)
                    .command_buffer_count(1),
            )?[0]
        };
        let begin = vk::CommandBufferBeginInfo::default()
            .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);
        unsafe { self.device.begin_command_buffer(cmd, &begin)? };
        f(cmd);
        unsafe { self.device.end_command_buffer(cmd)? };
        let submit = vk::SubmitInfo::default().command_buffers(std::slice::from_ref(&cmd));
        unsafe {
            self.device
                .queue_submit(self.graphics_queue, std::slice::from_ref(&submit), vk::Fence::null())?;
            self.device.queue_wait_idle(self.graphics_queue)?;
        }
        unsafe {
            self.device
                .free_command_buffers(pool, std::slice::from_ref(&cmd));
        }
        Ok(())
    }
}

unsafe extern "system" fn debug_callback(
    message_severity: vk::DebugUtilsMessageSeverityFlagsEXT,
    message_type: vk::DebugUtilsMessageTypeFlagsEXT,
    p_callback_data: *const vk::DebugUtilsMessengerCallbackDataEXT,
    _p_user_data: *mut std::ffi::c_void,
) -> vk::Bool32 {
    let data = unsafe { (*p_callback_data).p_message.as_ref() }
        .and_then(|p| unsafe { std::ffi::CStr::from_ptr(p).to_str().ok() });
    if let Some(msg) = data {
        match message_severity {
            vk::DebugUtilsMessageSeverityFlagsEXT::ERROR => log::error!("[vk] {}", msg),
            vk::DebugUtilsMessageSeverityFlagsEXT::WARNING => log::warn!("[vk] {}", msg),
            _ => log::debug!("[vk] {} ({:?})", msg, message_type),
        }
    }
    vk::FALSE
}

pub type SharedInstance = Arc<Instance>;
pub type SharedDevice = Arc<Device>;
pub type SharedSwapchain = Arc<Swapchain>;