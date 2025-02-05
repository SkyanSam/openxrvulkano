use vulkano::device::DeviceExtensions;
use vulkano::swapchain::Surface;

// https://github.com/vulkano-rs/vulkano-book/blob/main/chapter-code/07-windowing/main.rs

mod openxr_init;
mod openxr_session;
mod vulkan;
use std::sync::Arc;
use vulkano;

use vulkano::buffer::{Buffer, BufferContents, BufferCreateInfo, BufferUsage, Subbuffer};
use vulkano::command_buffer::allocator::StandardCommandBufferAllocator;
use vulkano::command_buffer::{
    AutoCommandBufferBuilder, CommandBufferUsage, PrimaryAutoCommandBuffer, RenderPassBeginInfo,
    SubpassBeginInfo, SubpassContents,
};
use vulkano::device::physical::{PhysicalDevice, PhysicalDeviceType};
use vulkano::image::view::ImageView;
use vulkano::image::{Image, ImageUsage};
use vulkano::instance::{Instance, InstanceCreateFlags, InstanceCreateInfo};
use vulkano::memory::allocator::{AllocationCreateInfo, MemoryTypeFilter, StandardMemoryAllocator};
use vulkano::pipeline::graphics::color_blend::{ColorBlendAttachmentState, ColorBlendState};
use vulkano::pipeline::graphics::input_assembly::InputAssemblyState;
use vulkano::pipeline::graphics::multisample::MultisampleState;
use vulkano::pipeline::graphics::rasterization::RasterizationState;
use vulkano::pipeline::graphics::vertex_input::{Vertex, VertexDefinition};
use vulkano::pipeline::graphics::viewport::{Viewport, ViewportState};
use vulkano::pipeline::graphics::GraphicsPipelineCreateInfo;
use vulkano::pipeline::layout::PipelineDescriptorSetLayoutCreateInfo;
use vulkano::pipeline::{GraphicsPipeline, PipelineLayout, PipelineShaderStageCreateInfo};
use vulkano::render_pass::{Framebuffer, FramebufferCreateInfo, RenderPass, Subpass};
use vulkano::shader;
use vulkano::shader::ShaderModule;
use vulkano::sync::future::FenceSignalFuture;
use vulkano::sync::{self, GpuFuture};
use vulkano::{Validated, VulkanError};

use crate::openxr_session::OpenxrSession;
use vulkan::MyVertex;

mod vs {
    vulkano_shaders::shader! {
        ty: "vertex",
        src: r"
            #version 460

            layout(location = 0) in vec2 position;

            void main() {
                gl_Position = vec4(position, 0.0, 1.0);
            }
        ",
    }
}

mod fs {
    vulkano_shaders::shader! {
        ty: "fragment",
        src: r"
            #version 460

            layout(location = 0) out vec4 f_color;

            void main() {
                f_color = vec4(1.0, 0.0, 0.0, 1.0);
            }
        ",
    }
}

fn main() {
    let openxr_objects = openxr_init::start_openxr();
    let instance = openxr_objects.vulkan_instance;
    let physical_device = openxr_objects.vulkan_physical_device;
    let device = openxr_objects.vulkan_device;
    let queues = openxr_objects.vulkan_device_queues;
    let queue = queues.iter().next().unwrap();

    let openxr_session = OpenxrSession::init(
        &openxr_objects.xr_instance,
        &instance,
        &physical_device,
        &device,
        queue,
        &openxr_objects.xr_system_id,
    );

    // Note images is created from Vulkano Swapchain, so currently we have a empty vec of images.
    // This may cause issues later.
    let images: Vec<Arc<Image>> = vec![];

    let render_pass = vulkan::get_render_pass(device.clone());
    let framebuffers = vulkan::get_framebuffers(&images, render_pass.clone());
    let memory_allocator = Arc::new(StandardMemoryAllocator::new_default(device.clone()));
    let vertex1 = MyVertex {
        position: [-0.5, -0.5],
    };
    let vertex2 = MyVertex {
        position: [0.0, 0.5],
    };
    let vertex3 = MyVertex {
        position: [0.5, -0.25],
    };
    let vertex_buffer = Buffer::from_iter(
        memory_allocator,
        BufferCreateInfo {
            usage: BufferUsage::VERTEX_BUFFER,
            ..Default::default()
        },
        AllocationCreateInfo {
            memory_type_filter: MemoryTypeFilter::PREFER_DEVICE
                | MemoryTypeFilter::HOST_SEQUENTIAL_WRITE,
            ..Default::default()
        },
        vec![vertex1, vertex2, vertex3],
    )
    .unwrap();

    let vs = vs::load(device.clone()).expect("failed to create shader module");
    let fs = fs::load(device.clone()).expect("failed to create shader module");

    let mut viewport = Viewport {
        offset: [0.0, 0.0],
        extent: [1920.0, 1080.0],
        depth_range: 0.0..=1.0,
    };

    let pipeline = vulkan::get_pipeline(
        device.clone(),
        vs.clone(),
        fs.clone(),
        render_pass.clone(),
        viewport.clone(),
    );

    let command_buffer_allocator =
        StandardCommandBufferAllocator::new(device.clone(), Default::default());

    let mut command_buffers = vulkan::get_command_buffers(
        &command_buffer_allocator,
        &queue,
        &pipeline,
        &framebuffers,
        &vertex_buffer,
    );

    let swapchain = openxr_init::create_swapchain(
        &openxr_objects.xr_instance,
        &openxr_session.session,
        &openxr_objects.xr_system_id,
    );
    //
    // let frames_in_flight = images.len();
    // let mut fences: Vec<Option<Arc<FenceSignalFuture<PresentFuture<CommandBufferExecFuture<JoinFuture<_>>>>>>> = vec![None; frames_in_flight];
    // let mut previous_fence_i = 0;

    loop {
        // let future = sync::now(device.clone())
        //     .then_execute(queue.clone(), command_buffers[0].clone())
        //     .unwrap()
        //     .then_signal_fence_and_flush()
        //     .unwrap();
        // future.wait(None).unwrap();

        /*
        if let Some(image_fence) = &fences[image_i as usize] {
            image_fence.wait(None).unwrap();
        }
        let previous_future = match fences[previous_fence_i as usize].clone() {
            // Create a NowFuture
            None => {
                let mut now = sync::now(device.clone());
                now.cleanup_finished();

                now.boxed()
            }
            // Use the existing FenceSignalFuture
            Some(fence) => fence.boxed(),
        };

        // acquire future is from swapchain..
        let future = previous_future
            .join(acquire_future)
            .then_execute(queue.clone(), command_buffers[image_i as usize].clone())
            .unwrap()
            .then_swapchain_present(
                queue.clone(),
                SwapchainPresentInfo::swapchain_image_index(swapchain.clone(), image_i),
            )
            .then_signal_fence_and_flush();

        fences[image_i as usize] = match future.map_err(Validated::unwrap) {
            Ok(value) => Some(Arc::new(value)),
            Err(VulkanError::OutOfDate) => {
                recreate_swapchain = true;
                None
            }
            Err(e) => {
                println!("failed to flush future: {e}");
                None
            }
        };*/

        //previous_fence_i = image_i;
        // break;
    }
}
