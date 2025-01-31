use vulkano::swapchain::Surface;


// https://github.com/vulkano-rs/vulkano-book/blob/main/chapter-code/07-windowing/main.rs


mod openxr_init;
mod vulkan;

fn main() {
    let openxr_objects = openxr_init::start_openxr();
    let instance = openxr_objects.vulkan_instance;
    let physical_device = openxr_objects.vulkan_physical_device;
    let device = openxr_objects.vulkan_device;
    let queues = openxr_objects.vulkan_device_queues;
    let queue = queues.next().unwrap();

    Surface::from(instance, physical_device, queue);
}
