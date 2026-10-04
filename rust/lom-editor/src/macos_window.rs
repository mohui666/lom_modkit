//! Standard AppKit window chrome for the portable Rust/OpenGL editor.
//! Native glass belongs in the later macOS shell: putting AppKit effect views
//! beside the content view breaks titlebar layout, and putting them inside it
//! overlays the OpenGL surface. Keep this shell opaque and its text readable.

pub fn install(frame: &eframe::Frame) -> Result<String, String> {
    #[cfg(target_os = "macos")]
    {
        use objc2::{rc::Retained, MainThreadMarker};
        use objc2_app_kit::{
            NSAppearance, NSAppearanceCustomization, NSAppearanceNameAqua, NSColor, NSView,
            NSWindowTitleVisibility,
        };
        use raw_window_handle::{HasWindowHandle, RawWindowHandle};
        let _main = MainThreadMarker::new().ok_or("窗口外观只能在 AppKit 主线程设置")?;
        let handle = frame.window_handle().map_err(|e| e.to_string())?;
        let RawWindowHandle::AppKit(handle) = handle.as_raw() else {
            return Err("当前窗口不是 AppKit 窗口".into());
        };
        // SAFETY: Frame's borrowed handle keeps NSView live for this main-thread
        // call. The retained object never leaves the thread or this function.
        let view = unsafe { Retained::<NSView>::retain(handle.ns_view.cast().as_ptr()) }
            .ok_or("编辑器原生窗口不可用")?;
        let window = view.window().ok_or("编辑器视图尚未附着到窗口")?;
        // SAFETY: immutable public AppKit constant available on supported macOS.
        let appearance = NSAppearance::appearanceNamed(unsafe { NSAppearanceNameAqua })
            .ok_or("无法创建浅色窗口外观")?;
        window.setAppearance(Some(&appearance));
        window.setOpaque(true);
        window.setBackgroundColor(Some(&NSColor::colorWithSRGBRed_green_blue_alpha(
            0.96, 0.96, 0.95, 1.0,
        )));
        window.setTitlebarAppearsTransparent(false);
        window.setTitleVisibility(NSWindowTitleVisibility::Visible);
        Ok("AppKit-standard".into())
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = frame;
        Ok("portable".into())
    }
}
