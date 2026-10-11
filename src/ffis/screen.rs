//! Query the primary display before creating a window or laying out a page.

/// Usable window width and height for a non-resizable window, excluding dock and menu (macOS).
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
pub fn get_window_dimensions() -> Option<(usize, usize)> {
    use std::ffi::{c_char, c_void};

    #[link(name = "AppKit", kind = "framework")]
    #[link(name = "objc")]
    extern "C" {
        fn objc_getClass(name: *const c_char) -> *mut c_void;
        fn sel_registerName(name: *const c_char) -> *mut c_void;
        fn objc_msgSend();
    }
    extern "C" {
        fn pthread_main_np() -> i32;
    }

    unsafe {
        if pthread_main_np() == 0 {
            return None;
        }
        // Send will be a function in C, taking in 2 pointers of unknown type, returning a pointer.
        // *mut c_void - a pointer to an unknown-type mutable value.
        type SendFunction = unsafe extern "C" fn(*mut c_void, *mut c_void) -> *mut c_void;

        let function_address = objc_msgSend as *const ();
        let send: SendFunction = std::mem::transmute(function_address);

        let selector = |name: &[u8]| sel_registerName(name.as_ptr().cast());
        let screens = send(objc_getClass(c"NSScreen".as_ptr()), selector(b"screens\0"));
        let screen = send(screens, selector(b"firstObject\0"));
        if screen.is_null() {
            return None;
        }

        let rect_entry = objc_msgSend as *const ();
        let rect: unsafe extern "C" fn(*mut c_void, *mut c_void) -> Rect =
            std::mem::transmute(rect_entry);
        let content_rect: unsafe extern "C" fn(*mut c_void, *mut c_void, Rect, usize) -> Rect =
            std::mem::transmute(rect_entry);

        let visible = rect(screen, selector(b"visibleFrame\0"));
        // Reserve the title bar of a non-resizable window.
        let content = content_rect(
            objc_getClass(c"NSWindow".as_ptr()),
            selector(b"contentRectForFrameRect:styleMask:\0"),
            visible,
            1 | 2,
        );
        Some((
            content.size.width.floor() as usize,
            content.size.height.floor() as usize,
        ))
    }
}

#[cfg(not(all(target_os = "macos", target_arch = "aarch64")))]
pub fn get_window_dimensions() -> Option<(usize, usize)> {
    None
}

#[derive(Clone, Copy)]
#[repr(C)]
struct Size {
    width: f64,
    height: f64,
}

// CGRect contains a CGPoint and CGSize, with double-precision components.
#[cfg(target_os = "macos")]
#[derive(Clone, Copy)]
#[repr(C)]
struct Point {
    x: f64,
    y: f64,
}

#[cfg(target_os = "macos")]
#[derive(Clone, Copy)]
#[repr(C)]
struct Rect {
    origin: Point,
    size: Size,
}
