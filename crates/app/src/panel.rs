use app::debug_panel::{PanelInfo, PanelState};
use app::sdl::WindowHandle;
use app::session::Fatal;
use render::{Device, FrameContext};
use sdl3_sys::events::SDL_Event;

#[cfg(feature = "debug-panel")]
mod backends {
    use render::vk;
    use sdl3_sys::events::SDL_Event;
    use sdl3_sys::video::SDL_Window;

    unsafe extern "C" {
        #[allow(clippy::too_many_arguments)]
        pub fn crystal_imgui_init(
            window: *mut SDL_Window,
            loader: vk::PFN_vkGetInstanceProcAddr,
            instance: vk::Instance,
            physical_device: vk::PhysicalDevice,
            device: vk::Device,
            queue_family: u32,
            queue: vk::Queue,
            swapchain_image_count: u32,
            format: vk::Format,
        ) -> bool;
        pub fn crystal_imgui_process_event(event: *const SDL_Event);
        pub fn crystal_imgui_wants_keyboard() -> bool;
        pub fn crystal_imgui_new_frame();
        pub fn crystal_imgui_render();
        pub fn crystal_imgui_draw(cmd: vk::CommandBuffer);
        pub fn crystal_imgui_shutdown();
    }
}

pub struct Gui;

#[cfg(feature = "debug-panel")]
impl Gui {
    pub fn new(window: WindowHandle, device: &Device) -> Result<Gui, Fatal> {
        // SAFETY: a live window and device; the backends keep these handles until shutdown.
        let ready = unsafe {
            backends::crystal_imgui_init(
                window.0,
                device.entry().static_fn().get_instance_proc_addr,
                device.instance().handle(),
                device.physical_device(),
                device.device().handle(),
                device.queue_family(),
                device.queue(),
                device.swapchain_image_count(),
                device.swapchain_format(),
            )
        };
        if !ready {
            return Err(Fatal::Error("ImGui Vulkan backend".to_owned()));
        }
        Ok(Gui)
    }

    pub fn process_event(&mut self, event: &SDL_Event) {
        // SAFETY: the backends are initialised.
        unsafe { backends::crystal_imgui_process_event(event) };
    }

    pub fn wants_keyboard(&self) -> bool {
        // SAFETY: as above.
        unsafe { backends::crystal_imgui_wants_keyboard() }
    }

    pub fn frame(&mut self, panel: &mut PanelState, info: &PanelInfo) {
        // SAFETY: as above.
        unsafe { backends::crystal_imgui_new_frame() };
        app::debug_panel::draw_panel(panel, info);
        // SAFETY: as above.
        unsafe { backends::crystal_imgui_render() };
    }

    pub fn render(&mut self, device: &Device, context: &FrameContext) -> Result<(), Fatal> {
        use render::vk;
        let colour = [vk::RenderingAttachmentInfo::default()
            .image_view(context.view)
            .image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
            .load_op(vk::AttachmentLoadOp::LOAD)
            .store_op(vk::AttachmentStoreOp::STORE)];
        let rendering = vk::RenderingInfo::default()
            .render_area(vk::Rect2D { offset: vk::Offset2D::default(), extent: device.swapchain_extent() })
            .layer_count(1)
            .color_attachments(&colour);
        // SAFETY: the frame's command buffer is recording and its swapchain image is a colour attachment.
        unsafe {
            device.device().cmd_begin_rendering(context.cmd, &rendering);
            backends::crystal_imgui_draw(context.cmd);
            device.device().cmd_end_rendering(context.cmd);
        }
        Ok(())
    }

    pub fn shutdown(self, _device: &Device) {
        // SAFETY: the device is idle.
        unsafe { backends::crystal_imgui_shutdown() };
    }
}

#[cfg(not(feature = "debug-panel"))]
impl Gui {
    pub fn new(_window: WindowHandle, _device: &Device) -> Result<Gui, Fatal> {
        Ok(Gui)
    }

    pub fn process_event(&mut self, _event: &SDL_Event) {}

    pub fn wants_keyboard(&self) -> bool {
        false
    }

    pub fn frame(&mut self, _panel: &mut PanelState, _info: &PanelInfo) {}

    pub fn render(&mut self, _device: &Device, _context: &FrameContext) -> Result<(), Fatal> {
        Ok(())
    }

    pub fn shutdown(self, _device: &Device) {}
}
