//! AppKit material behind eframe's transparent OpenGL content view.
//!
//! This is the native counterpart of `editor/macos_glass.py`: prefer the public
//! NSGlassEffectView regular material, fall back to NSVisualEffectView sidebar,
//! and preserve the opaque dark window shell for readable content/titlebar.
//! AppKit's parent view retains the material and releases it with the window.
//! No native object/pointer is stored in Rust or moved onto a worker thread.

/// Install once from the first `App::update`, when the native window exists.
///
/// A returned native backend means an AppKit view was created and attached; it
/// does not claim a screenshot/visual acceptance. Reduced transparency and the
/// portable fallback are reported explicitly, never as a native glass success.
pub fn install(frame: &eframe::Frame) -> Result<String, String> {
    #[cfg(target_os = "macos")]
    {
        native::install(frame)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = frame;
        Ok("portable".to_owned())
    }
}

#[cfg(target_os = "macos")]
mod native {
    use objc2::{rc::Retained, runtime::AnyClass, MainThreadMarker, MainThreadOnly};
    use objc2_app_kit::{
        NSAppearance, NSAppearanceCustomization, NSAppearanceNameDarkAqua,
        NSAutoresizingMaskOptions, NSColor, NSGlassEffectView, NSGlassEffectViewStyle,
        NSUserInterfaceItemIdentification, NSView, NSVisualEffectBlendingMode,
        NSVisualEffectMaterial, NSVisualEffectState, NSVisualEffectView, NSWindowOrderingMode,
        NSWorkspace,
    };
    use objc2_foundation::ns_string;
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};

    pub(super) fn install(frame: &eframe::Frame) -> Result<String, String> {
        // AppKit requires its main thread. This token also bounds every alloc
        // below to that thread; calling from a worker produces an explicit error.
        let mtm = MainThreadMarker::new()
            .ok_or_else(|| "原生 macOS 玻璃只能在 AppKit 主线程安装".to_owned())?;
        let handle = frame
            .window_handle()
            .map_err(|error| format!("无法取得编辑器原生窗口句柄：{error}"))?;
        let RawWindowHandle::AppKit(handle) = handle.as_raw() else {
            return Err("编辑器窗口不是 AppKit 窗口，无法安装原生 macOS 玻璃".to_owned());
        };
        // SAFETY: Frame's borrowed WindowHandle guarantees a live NSView. We
        // are on the main thread and retain it for the duration of this call.
        let view = unsafe { Retained::<NSView>::retain(handle.ns_view.cast().as_ptr()) }
            .ok_or_else(|| "编辑器的 AppKit NSView 不可用".to_owned())?;
        let window = view
            .window()
            .ok_or_else(|| "编辑器的 NSView 尚未附着到 NSWindow".to_owned())?;
        // SAFETY: this is an immutable public AppKit appearance-name constant,
        // available on every macOS version supported by this editor.
        let appearance = NSAppearance::appearanceNamed(unsafe { NSAppearanceNameDarkAqua })
            .ok_or_else(|| "AppKit 无法创建 Dark Aqua 外观".to_owned())?;
        window.setAppearance(Some(&appearance));
        let background = NSColor::colorWithSRGBRed_green_blue_alpha(0.06, 0.07, 0.10, 1.0);
        window.setOpaque(true);
        window.setBackgroundColor(Some(&background));
        window.setTitlebarAppearsTransparent(false);

        if NSWorkspace::sharedWorkspace().accessibilityDisplayShouldReduceTransparency() {
            return Ok("reduced-transparency".to_owned());
        }
        // SAFETY: the retained window owns its view tree throughout this
        // synchronous main-thread call. No event loop runs between this lookup
        // and addSubview; we retain the parent as well.
        let parent = unsafe { view.superview() }
            .ok_or_else(|| "编辑器的 NSView 没有原生父视图，无法附加玻璃材质".to_owned())?;

        // Native ownership lasts until the window is destroyed. Identification
        // prevents accidental duplicate installation without global pointers.
        for child in parent.subviews() {
            let Some(identifier) = child.identifier() else {
                continue;
            };
            if *identifier == *ns_string!("lom-modkit-native-glass-effect-v1") {
                return Ok("NSGlassEffectView".to_owned());
            }
            if *identifier == *ns_string!("lom-modkit-native-visual-effect-v1") {
                return Ok("NSVisualEffectView".to_owned());
            }
        }

        let bounds = view.frame();
        let (material, backend): (Retained<NSView>, &str) = if AnyClass::get(c"NSGlassEffectView")
            .is_some()
        {
            // Runtime availability is checked before invoking class/alloc,
            // allowing the same binary to run on pre-Liquid-Glass macOS.
            let glass = NSGlassEffectView::initWithFrame(NSGlassEffectView::alloc(mtm), bounds);
            glass.setStyle(NSGlassEffectViewStyle::Regular);
            glass.setCornerRadius(14.0);
            glass.setTintColor(Some(&NSColor::colorWithSRGBRed_green_blue_alpha(
                0.10, 0.12, 0.18, 0.30,
            )));
            let content = NSView::initWithFrame(NSView::alloc(mtm), glass.bounds());
            glass.setContentView(Some(&content));
            glass.setIdentifier(Some(ns_string!("lom-modkit-native-glass-effect-v1")));
            (glass.into_super(), "NSGlassEffectView")
        } else if AnyClass::get(c"NSVisualEffectView").is_some() {
            let glass = NSVisualEffectView::initWithFrame(NSVisualEffectView::alloc(mtm), bounds);
            glass.setMaterial(NSVisualEffectMaterial::Sidebar);
            glass.setBlendingMode(NSVisualEffectBlendingMode::BehindWindow);
            glass.setState(NSVisualEffectState::Active);
            glass.setIdentifier(Some(ns_string!("lom-modkit-native-visual-effect-v1")));
            (glass.into_super(), "NSVisualEffectView")
        } else {
            return Err(
                "当前 AppKit 不提供 NSGlassEffectView 或 NSVisualEffectView；已保留深色不透明窗体"
                    .to_owned(),
            );
        };
        material.setAutoresizingMask(
            NSAutoresizingMaskOptions::ViewWidthSizable
                | NSAutoresizingMaskOptions::ViewHeightSizable,
        );
        parent.addSubview_positioned_relativeTo(
            &material,
            NSWindowOrderingMode::Below,
            Some(&view),
        );
        // SAFETY: all three retained views are live on the main thread. Verify
        // attachment before returning a native backend to the UI status label.
        if unsafe { material.superview() }.is_none() {
            return Err(format!("{backend} 创建成功但未附着到编辑器窗口"));
        }
        material.setNeedsDisplay(true);
        view.setNeedsDisplay(true);
        // `parent` retains `material`; releasing these temporary +1 references
        // leaves no leak, dangling Rust pointer, or off-main-thread destructor.
        Ok(backend.to_owned())
    }
}
