//! Standard AppKit window chrome for the portable Rust/OpenGL editor.
//! Native glass belongs in the later macOS shell: putting AppKit effect views
//! beside the content view breaks titlebar layout, and putting them inside it
//! overlays the OpenGL surface. Keep this shell opaque and its text readable.

use std::sync::atomic::{AtomicBool, Ordering};

static SETTINGS_REQUESTED: AtomicBool = AtomicBool::new(false);

/// Install into winit's existing application menu after the event loop has started.
/// Other standard AppKit items and their actions remain owned by winit.
pub fn install_settings_menu(ctx: &eframe::egui::Context) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        settings_menu::install(ctx)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = ctx;
        Ok(())
    }
}

/// Consume an action already delivered by AppKit. No timer or polling wakeup is used.
pub fn take_settings_request() -> bool {
    SETTINGS_REQUESTED.swap(false, Ordering::AcqRel)
}

pub fn update_settings_menu_title() {
    #[cfg(target_os = "macos")]
    settings_menu::update_title();
}

#[cfg(any(target_os = "macos", test))]
fn request_settings(ctx: &eframe::egui::Context) {
    SETTINGS_REQUESTED.store(true, Ordering::Release);
    // eframe routes this callback through its event-loop proxy, including when idle.
    ctx.request_repaint();
}

#[cfg(target_os = "macos")]
mod settings_menu {
    use std::cell::RefCell;

    use objc2::{
        define_class, msg_send, rc::Retained, sel, DefinedClass, MainThreadMarker, MainThreadOnly,
    };
    use objc2_app_kit::{NSApplication, NSEventModifierFlags, NSMenuItem};
    use objc2_foundation::{ns_string, NSObject, NSObjectProtocol, NSString};

    struct MenuIvars {
        context: eframe::egui::Context,
    }

    define_class!(
        // SAFETY: NSObject has no subclassing requirements; this object stays on
        // AppKit's main thread and its ivars are initialized before NSObject.init.
        #[unsafe(super = NSObject)]
        #[thread_kind = MainThreadOnly]
        #[ivars = MenuIvars]
        struct LomSettingsMenuTarget;

        unsafe impl NSObjectProtocol for LomSettingsMenuTarget {}

        impl LomSettingsMenuTarget {
            // SAFETY: AppKit menu actions have one object sender and return void.
            #[unsafe(method(lomOpenSettings:))]
            fn open_settings(&self, _sender: &NSMenuItem) {
                super::request_settings(&self.ivars().context);
            }
        }
    );

    impl LomSettingsMenuTarget {
        fn new(main: MainThreadMarker, context: eframe::egui::Context) -> Retained<Self> {
            let this = Self::alloc(main).set_ivars(MenuIvars { context });
            // SAFETY: NSObject.init initializes the allocated NSObject subclass.
            unsafe { msg_send![super(this), init] }
        }
    }

    struct MenuBridge {
        // NSMenuItem.target is weak. Retain its target for the application lifetime.
        _target: Retained<LomSettingsMenuTarget>,
        item: Retained<NSMenuItem>,
    }

    thread_local! {
        static BRIDGE: RefCell<Option<MenuBridge>> = const { RefCell::new(None) };
    }

    pub(super) fn install(ctx: &eframe::egui::Context) -> Result<(), String> {
        let main = MainThreadMarker::new().ok_or("系统菜单只能在 AppKit 主线程安装")?;
        BRIDGE.with(|bridge| {
            let mut bridge = bridge.borrow_mut();
            if bridge.is_some() {
                return Ok(());
            }
            // winit initializes its default application menu before NewEvents;
            // eframe's first update runs after that initialization.
            let app = NSApplication::sharedApplication(main);
            let app_menu = app
                .mainMenu()
                .and_then(|menu| menu.itemAtIndex(0))
                .and_then(|item| item.submenu())
                .ok_or("系统应用菜单尚未初始化")?;
            let target = LomSettingsMenuTarget::new(main, ctx.clone());
            // SAFETY: lomOpenSettings: is implemented above with the action signature.
            let item = unsafe {
                NSMenuItem::initWithTitle_action_keyEquivalent(
                    NSMenuItem::alloc(main),
                    &NSString::from_str(&crate::i18n::tr("设置")),
                    Some(sel!(lomOpenSettings:)),
                    ns_string!(","),
                )
            };
            item.setKeyEquivalentModifierMask(NSEventModifierFlags::Command);
            // SAFETY: target implements this item's action and BRIDGE retains it.
            unsafe {
                item.setTarget(Some(&target));
            }
            let insert_at = (0..app_menu.numberOfItems())
                .find(|&index| {
                    app_menu
                        .itemAtIndex(index)
                        .is_some_and(|item| item.isSeparatorItem())
                })
                .map_or(0, |index| index + 1);
            app_menu.insertItem_atIndex(&item, insert_at);
            app_menu.insertItem_atIndex(&NSMenuItem::separatorItem(main), insert_at + 1);
            *bridge = Some(MenuBridge {
                _target: target,
                item,
            });
            Ok(())
        })
    }

    pub(super) fn update_title() {
        if MainThreadMarker::new().is_none() {
            return;
        }
        BRIDGE.with(|bridge| {
            if let Some(bridge) = bridge.borrow().as_ref() {
                bridge
                    .item
                    .setTitle(&NSString::from_str(&crate::i18n::tr("设置")));
            }
        });
    }
}

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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{atomic::AtomicUsize, Arc};

    #[test]
    fn settings_action_wakes_idle_ui_and_is_consumed_once() {
        let context = eframe::egui::Context::default();
        // Let egui settle its initial paints so the callback proves an idle wakeup.
        for _ in 0..3 {
            let _ = context.run(eframe::egui::RawInput::default(), |_| {});
        }
        let wakes = Arc::new(AtomicUsize::new(0));
        let count = wakes.clone();
        context.set_request_repaint_callback(move |_| {
            count.fetch_add(1, Ordering::Relaxed);
        });
        assert!(!take_settings_request());
        request_settings(&context);
        assert!(wakes.load(Ordering::Relaxed) > 0);
        assert!(take_settings_request());
        assert!(!take_settings_request());
    }
}
