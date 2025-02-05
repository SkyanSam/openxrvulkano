use ash::vk::Handle;
use openxr::vulkan::SessionCreateInfo;
use openxr::{FrameStream, FrameWaiter, Session, SystemId};
use std::sync::Arc;
use vulkano::device::physical::PhysicalDevice;
use vulkano::device::Device;
use vulkano::device::Queue;
use vulkano::VulkanObject;

pub struct OpenxrSession {
    // State provided by the openxr API
    pub session: Session<openxr::Vulkan>,
    pub frame_waiter: FrameWaiter,
    pub frame_stream: FrameStream<openxr::Vulkan>,

    // State kept by ourselves
    session_running: bool,
    event_storage: openxr::EventDataBuffer,
}

impl OpenxrSession {
    pub fn init(
        xr_instance: &openxr::Instance,
        vulkan_instance: &Arc<vulkano::instance::Instance>,
        physical_device: &Arc<PhysicalDevice>,
        device: &Arc<Device>,
        queue: &Arc<Queue>,
        system: &SystemId,
    ) -> Self {
        let (session, frame_waiter, frame_stream) = unsafe {
            xr_instance.create_session::<openxr::Vulkan>(
                system.clone(),
                &SessionCreateInfo {
                    instance: vulkan_instance.handle().as_raw() as _,
                    physical_device: physical_device.handle().as_raw() as _,
                    device: device.handle().as_raw() as _,
                    queue_family_index: queue.queue_family_index(),
                    queue_index: queue.id_within_family(),
                },
            )
        }
        .expect("Failed to open OpenXR Session");

        Self {
            session,
            frame_waiter,
            frame_stream,

            session_running: false,
            event_storage: openxr::EventDataBuffer::new(),
        }
    }

    pub fn session_running(&self) -> bool {
        self.session_running
    }

    // TODO This is bad, should be integrated right into the loop design instead
    /// Call this function through the loop. If it returns true, kill the loop
    pub fn read_xr_events(&mut self, xr_instance: &openxr::Instance) -> bool {
        while let Some(event) = xr_instance.poll_event(&mut self.event_storage).unwrap() {
            use openxr::Event::*;
            match event {
                SessionStateChanged(e) => {
                    // Session state change is where we can begin and end sessions, as well as
                    // find quit messages!
                    println!("entered state {:?}", e.state());
                    match e.state() {
                        openxr::SessionState::READY => {
                            self.session
                                .begin(openxr::ViewConfigurationType::PRIMARY_STEREO)
                                .unwrap();
                            self.session_running = true;
                        }
                        openxr::SessionState::STOPPING => {
                            self.session.end().unwrap();
                            self.session_running = false;
                        }
                        openxr::SessionState::EXITING | openxr::SessionState::LOSS_PENDING => {
                            return true
                        }
                        _ => {}
                    }
                }
                InstanceLossPending(_) => return true,
                EventsLost(e) => {
                    println!("lost {} events", e.lost_event_count());
                }
                _ => {}
            }
        }

        false
    }
}
