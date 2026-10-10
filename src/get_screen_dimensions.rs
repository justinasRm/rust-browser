//! Query the primary display before creating a window or laying out a page.

/// Usable content height for a titled, non-resizable window.
#[cfg(target_os = "macos")]
pub fn get_window_height() -> Option<usize> {
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
        let screens = send(
            objc_getClass(b"NSScreen\0".as_ptr().cast()),
            selector(b"screens\0"),
        );
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
        // Match minifb's titled, closable, non-resizable window.
        let content = content_rect(
            objc_getClass(b"NSWindow\0".as_ptr().cast()),
            selector(b"contentRectForFrameRect:styleMask:\0"),
            visible,
            1 | 2,
        );
        Some(content.size.height.floor() as usize)
    }
}

/// Return the full width of the primary display.
pub fn get_screen_width() -> Option<f64> {
    let dims = platform_dimensions();
    if dims.width.is_finite() && dims.width > 0.0 {
        Some(dims.width)
    } else {
        None
    }
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

#[cfg(target_os = "macos")]
fn platform_dimensions() -> Size {
    #[link(name = "CoreGraphics", kind = "framework")]
    extern "C" {
        fn CGMainDisplayID() -> u32;
        fn CGDisplayBounds(display: u32) -> Rect;
    }

    unsafe { CGDisplayBounds(CGMainDisplayID()).size }
}

#[cfg(not(target_os = "macos"))]
fn platform_dimensions() -> Size {
    Size {
        width: 0.0,
        height: 0.0,
    }
}
