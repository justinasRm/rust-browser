//! Query the primary display before creating a window or laying out a page.

// If multiple displays exist, this returns only the primary displays width
pub fn get_screen_width() -> Option<f32> {
    let width = platform_width();
    if width.is_finite() && width > 0.0 {
        Some(width)
    } else {
        None
    }
}

#[cfg(target_os = "macos")]
fn platform_width() -> f32 {
    // CGRect consists of a CGPoint and CGSize, each containing two CGFloats.
    // CGFloat is a double on the supported 64-bit macOS targets.
    #[repr(C)]
    struct Point {
        x: f64,
        y: f64,
    }
    #[repr(C)]
    struct Size {
        width: f64,
        height: f64,
    }
    #[repr(C)]
    struct Rect {
        origin: Point,
        size: Size,
    }

    #[link(name = "CoreGraphics", kind = "framework")]
    extern "C" {
        fn CGMainDisplayID() -> u32;
        fn CGDisplayBounds(display: u32) -> Rect;
    }

    unsafe { CGDisplayBounds(CGMainDisplayID()).size.width as f32 }
}

#[cfg(target_os = "windows")]
fn platform_width() -> f32 {
    #[link(name = "user32")]
    extern "system" {
        fn GetSystemMetrics(index: i32) -> i32;
    }
    const SM_CXSCREEN: i32 = 0;
    unsafe { GetSystemMetrics(SM_CXSCREEN) as f32 }
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn platform_width() -> f32 {
    0.0
}
