// The window path of Device (surface, swapchain, frames in flight, present) through the Window trait, on an X window
// that is created but never mapped, so nothing is shown. Skips without an X display or libxcb.

mod support;

use std::ffi::{CString, c_char, c_int, c_void};

use render::{Device, DeviceOptions, NativeOutput, NativeRenderer, Window, vk};
use scene::frame::{BlendOp, TargetName};

#[repr(C)]
struct ScreenIterator {
    data: *const u8,
    rem: c_int,
    index: c_int,
}

struct Xcb {
    _library: libloading::Library,
    connection: *mut c_void,
    window: u32,
    size: (i32, i32),
    disconnect: unsafe extern "C" fn(*mut c_void),
}

impl Xcb {
    fn open(width: u16, height: u16) -> Option<Xcb> {
        std::env::var_os("DISPLAY")?;
        // SAFETY: libxcb's C API, used as xcb.h declares it; the window is never mapped.
        unsafe {
            let library = libloading::Library::new("libxcb.so.1").ok()?;
            let connect: libloading::Symbol<unsafe extern "C" fn(*const c_char, *mut c_int) -> *mut c_void> = library.get(b"xcb_connect").ok()?;
            let has_error: libloading::Symbol<unsafe extern "C" fn(*mut c_void) -> c_int> = library.get(b"xcb_connection_has_error").ok()?;
            let get_setup: libloading::Symbol<unsafe extern "C" fn(*mut c_void) -> *const c_void> = library.get(b"xcb_get_setup").ok()?;
            let roots: libloading::Symbol<unsafe extern "C" fn(*const c_void) -> ScreenIterator> = library.get(b"xcb_setup_roots_iterator").ok()?;
            let generate_id: libloading::Symbol<unsafe extern "C" fn(*mut c_void) -> u32> = library.get(b"xcb_generate_id").ok()?;
            #[allow(clippy::type_complexity)]
            let create_window: libloading::Symbol<
                unsafe extern "C" fn(*mut c_void, u8, u32, u32, i16, i16, u16, u16, u16, u16, u32, u32, *const c_void) -> u32,
            > = library.get(b"xcb_create_window").ok()?;
            let flush: libloading::Symbol<unsafe extern "C" fn(*mut c_void) -> c_int> = library.get(b"xcb_flush").ok()?;
            let disconnect: unsafe extern "C" fn(*mut c_void) = *library.get(b"xcb_disconnect").ok()?;
            let connection = connect(std::ptr::null(), std::ptr::null_mut());
            if connection.is_null() || has_error(connection) != 0 {
                return None;
            }
            let screen = roots(get_setup(connection)).data;
            let root = u32::from_ne_bytes(*(screen as *const [u8; 4]));
            let visual = u32::from_ne_bytes(*(screen.add(32) as *const [u8; 4]));
            let window = generate_id(connection);
            create_window(connection, 0, window, root, 0, 0, width, height, 0, 1, visual, 0, std::ptr::null());
            flush(connection);
            Some(Xcb { _library: library, connection, window, size: (i32::from(width), i32::from(height)), disconnect })
        }
    }
}

impl Drop for Xcb {
    fn drop(&mut self) {
        // SAFETY: the connection was opened by xcb_connect and nothing uses it any more.
        unsafe { (self.disconnect)(self.connection) };
    }
}

impl Window for Xcb {
    fn instance_extensions(&self) -> Vec<CString> {
        vec![ash::khr::surface::NAME.into(), ash::khr::xcb_surface::NAME.into()]
    }
    fn create_surface(&self, entry: &ash::Entry, instance: &ash::Instance) -> Result<vk::SurfaceKHR, String> {
        let loader = ash::khr::xcb_surface::Instance::new(entry, instance);
        let info = vk::XcbSurfaceCreateInfoKHR::default().connection(self.connection).window(self.window);
        // SAFETY: a live connection and window.
        unsafe { loader.create_xcb_surface(&info, None) }.map_err(|r| format!("{r:?}"))
    }
    fn size_in_pixels(&self) -> (i32, i32) {
        self.size
    }
    fn minimized(&self) -> bool {
        false
    }
}

#[test]
fn swapchain_frames() {
    let Some(window) = Xcb::open(640, 448) else {
        eprintln!("skipped: no X display or libxcb");
        return;
    };
    let device = match Device::new(Some(Box::new(window)), &DeviceOptions { validation: true, sync_validation: false }) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("skipped: no Vulkan device for the window ({e})");
            return;
        }
    };
    assert_eq!(device.swapchain_format(), vk::Format::B8G8R8A8_UNORM);
    assert_eq!(device.swapchain_extent(), vk::Extent2D { width: 640, height: 448 });
    assert!(device.swapchain_image_count() >= 2);
    let mut renderer = NativeRenderer::new(&device, support::shaders()).unwrap();
    renderer.configure(&NativeOutput { width: 640, height: 448, samples: 4 }).unwrap();
    let mut presented = 0;
    for n in 0..12u8 {
        let Some(context) = device.begin_frame().unwrap() else { continue };
        let frame = support::frame_of(vec![support::sprite(BlendOp::Opaque, n * 20, 100, 50, 0x80)]);
        renderer.record(context.cmd, &frame).unwrap();
        renderer.present(context.cmd, context.image, device.swapchain_extent(), TargetName::Display, 640.0 / 448.0).unwrap();
        device.end_frame(&context).unwrap();
        presented += 1;
    }
    println!("swapchain: {} images {:?}, {presented} frames presented", device.swapchain_image_count(), device.swapchain_extent());
    assert!(presented >= 10);
    assert_eq!(support::pixel(&renderer.read_target(TargetName::Display).unwrap(), 10, 10)[0], 220);
    assert_eq!(device.validation_errors(), 0);
}
