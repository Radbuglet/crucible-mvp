use winit::window::Window;

#[allow(unused)]
use raw_window_handle::{HasWindowHandle as _, RawWindowHandle};

pub fn is_in_live_resize(window: &Window) -> bool {
    match window.window_handle().unwrap().as_raw() {
        #[cfg(target_os = "macos")]
        RawWindowHandle::AppKit(hnd) => {
            use objc2_app_kit::NSView;

            let ns_view = unsafe { hnd.ns_view.cast::<NSView>().as_ref() };
            let window = ns_view.window().unwrap();

            unsafe { window.inLiveResize() }
        }
        _ => false,
    }
}
