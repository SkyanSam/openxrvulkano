use crate::vulkan::COLOR_FORMAT;
use ash::vk::{DeviceCreateInfo, DeviceQueueCreateInfo, Handle, InstanceCreateInfo};
use openxr::{
    ExtensionSet, FormFactor, SwapchainCreateFlags, SwapchainCreateInfo, SwapchainUsageFlags,
    ViewConfigurationType,
};
use std::sync::{Arc, OnceLock};
use vulkano::device::physical::PhysicalDevice;
use vulkano::device::{Device, Queue, QueueCreateInfo, QueueFamilyProperties, QueueFlags};
use vulkano::{Version, VulkanLibrary, VulkanObject};

static VULKAN_INSTANCE: OnceLock<Arc<VulkanLibrary>> = OnceLock::new();
fn get_vulkan_lib() -> &'static Arc<VulkanLibrary> {
    VULKAN_INSTANCE
        .get_or_init(|| VulkanLibrary::new().expect("Vulkan library could not be loaded."))
}

unsafe extern "system" fn get_instance_proc_addr_static(
    instance_ptr: *const std::ffi::c_void,
    name: *const std::ffi::c_char,
) -> Option<unsafe extern "system" fn()> {
    get_vulkan_lib().get_instance_proc_addr(ash::vk::Instance::from_raw(instance_ptr as _), name)
}

/// Objects which were initialized from OpenXR
pub struct ObjectsFromOpenxr {
    pub xr_instance: openxr::Instance,
    pub xr_system_id: openxr::SystemId,
    pub vulkan_instance: Arc<vulkano::instance::Instance>,
    pub vulkan_physical_device: Arc<PhysicalDevice>,
    pub vulkan_device: Arc<Device>,
    pub vulkan_device_queues: Vec<Arc<Queue>>, // TODO there should only be one -- CHECK in start_openxr!
}

pub fn start_openxr() -> ObjectsFromOpenxr {
    let xr_library = openxr::Entry::linked();

    let available_extensions = xr_library.enumerate_extensions();
    assert!(available_extensions.unwrap().khr_vulkan_enable2);

    let mut wanted_extensions = ExtensionSet::default();
    wanted_extensions.khr_vulkan_enable2 = true;

    let xr_instance = xr_library
        .create_instance(
            &openxr::ApplicationInfo {
                application_name: "openxrs example",
                application_version: 0,
                engine_name: "openxrs example",
                engine_version: 0,
                api_version: openxr::Version::new(1, 0, 0),
            },
            &wanted_extensions,
            &[],
        )
        .unwrap();
    let instance_props = xr_instance.properties().unwrap();

    let form_factor = xr_instance
        .system(FormFactor::HEAD_MOUNTED_DISPLAY)
        .expect("This test program needs a head mounted display -- could not find one.");

    // IMPORTANT: The OpenXR API REQUIRES that we call this!
    let _graphics_requirements = xr_instance
        .graphics_requirements::<openxr::Vulkan>(form_factor)
        .unwrap();

    get_vulkan_lib(); // TODO may not be necessary to load ahead of time
    let vk_app_info = ash::vk::ApplicationInfo {
        application_version: 0,
        engine_version: 0,
        api_version: ash::vk::make_api_version(0, 1, 1, 0),
        ..Default::default()
    };

    let (vk_instance, physical_device) = unsafe {
        let raw_vk_instance = xr_instance
            .create_vulkan_instance(
                form_factor,
                get_instance_proc_addr_static,
                &InstanceCreateInfo {
                    p_application_info: &vk_app_info as *const _,
                    ..Default::default()
                } as *const _ as *const _,
            )
            .expect("XR error creating Vulkan instance")
            .expect("Vulkan error creating Vulkan instance");
        let vk_instance = vulkano::instance::Instance::from_handle(
            get_vulkan_lib().clone(),
            Handle::from_raw(raw_vk_instance as _),
            // TODO copy the fields for this struct from `vk_app_info`
            vulkano::instance::InstanceCreateInfo {
                max_api_version: Some(Version {
                    major: 1,
                    minor: 1,
                    patch: 0,
                }),
                ..Default::default()
            },
        );

        let raw_physical_device = xr_instance
            .vulkan_graphics_device(form_factor, raw_vk_instance)
            .expect("Could not find a suitable graphics device");
        let physical_device = PhysicalDevice::from_handle(
            vk_instance.clone(),
            Handle::from_raw(raw_physical_device as _),
        )
        .expect("Could not convert the graphics device to a vulkano object");

        (vk_instance, physical_device)
    };

    let queue_family_index = physical_device
        .queue_family_properties()
        .into_iter()
        .enumerate()
        .find_map(|(ix, QueueFamilyProperties { queue_flags, .. })| {
            queue_flags
                .contains(QueueFlags::GRAPHICS)
                .then_some(ix as u32)
        })
        .expect("Could not find a graphics queue in the available physical device"); // TODO report what this device is

    let (device, queues) = unsafe {
        let raw_device = xr_instance
            .create_vulkan_device(
                form_factor,
                get_instance_proc_addr_static,
                physical_device.handle().as_raw() as _,
                &DeviceCreateInfo::builder()
                    .queue_create_infos(&[DeviceQueueCreateInfo::builder()
                        .queue_family_index(queue_family_index)
                        .queue_priorities(&[1.0])
                        .build()])
                    // TODO we do not need multiview right now, but maybe later
                    // .push_next(&mut vk::PhysicalDeviceMultiviewFeatures::builder().multiview(true))
                    .build() as *const _ as _,
            )
            .expect("XR error creating Vulkan device")
            .expect("Vulkan error creating Vulkan device");

        Device::from_handle(
            physical_device.clone(),
            Handle::from_raw(raw_device as _),
            // TODO again, copy below from the DeviceCreateInfo struct used in the openxr call
            vulkano::device::DeviceCreateInfo {
                queue_create_infos: vec![QueueCreateInfo {
                    queue_family_index,
                    queues: vec![1.0],
                    ..Default::default()
                }],
                ..Default::default()
            },
        )
    };

    ObjectsFromOpenxr {
        xr_instance,
        xr_system_id: form_factor,
        vulkan_instance: vk_instance,
        vulkan_physical_device: physical_device,
        vulkan_device: device,
        vulkan_device_queues: queues.collect(),
    }
}

pub fn create_swapchain(
    xr_instance: &openxr::Instance,
    xr_session: &openxr::Session<openxr::Vulkan>,
    xr_system_id: &openxr::SystemId,
    
    device: &Device,
) {
    // Now we need to find all the viewpoints we need to take care of! This is a
    // property of the view configuration type; in this example we use PRIMARY_STEREO,
    // so we should have 2 viewpoints.
    //
    // Because we are using multiview in this example, we require that all view
    // dimensions are identical.
    let views = xr_instance
        .enumerate_view_configuration_views(*xr_system_id, ViewConfigurationType::PRIMARY_STEREO)
        .unwrap();
    let view_count = views.len();
    // assert_eq!(views.len(), VIEW_COUNT as usize);
    assert_eq!(views[0], views[1]);

    // Create a swapchain for the viewpoints! A swapchain is a set of texture buffers
    // used for displaying to screen, typically this is a backbuffer and a front buffer,
    // one for rendering data to, and one for displaying on-screen.
    // let resolution = vk::Extent2D {
    //     width: views[0].recommended_image_rect_width,
    //     height: views[0].recommended_image_rect_height,
    // };
    let handle = xr_session
        .create_swapchain(&SwapchainCreateInfo {
            create_flags: SwapchainCreateFlags::EMPTY,
            usage_flags: SwapchainUsageFlags::COLOR_ATTACHMENT | SwapchainUsageFlags::SAMPLED,
            format: COLOR_FORMAT as i32 as _,
            // The Vulkan graphics pipeline we create is not set up for multisampling,
            // so we hardcode this to 1. If we used a proper multisampling setup, we
            // could set this to `views[0].recommended_swapchain_sample_count`.
            sample_count: 1,
            width: views[0].recommended_image_rect_width,
            height: views[0].recommended_image_rect_height,
            face_count: 1,
            array_size: view_count as u32,
            mip_count: 1,
        })
        .expect("Problem when creating swapchain");
    
    let raw_images = handle.enumerate_images().unwrap();

    // let image_view = (device.fns().v1_0.create_image_view)(
    //     
    // );   
}
