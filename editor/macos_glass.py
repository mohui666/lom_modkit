"""AppKit's public Liquid Glass API behind the Qt content view on macOS."""
from __future__ import annotations

import logging
import sys

from PySide6.QtCore import Qt
from PySide6.QtWidgets import QApplication


def prepare_macos_glass(window) -> None:
    if sys.platform == "darwin" and QApplication.platformName() == "cocoa":
        window.setAttribute(Qt.WidgetAttribute.WA_TranslucentBackground)


def install_macos_glass(window) -> str:
    """Attach once after show(); keep the native view alive with its Qt owner."""
    if sys.platform != "darwin" or QApplication.platformName() != "cocoa":
        return "portable"
    if getattr(window, "_macos_glass", None) is not None:
        return window.property("glassBackend")
    try:
        import AppKit
        import objc

        qt_view = objc.objc_object(c_void_p=int(window.winId()))
        native_window = qt_view.window()
        native_window.setAppearance_(AppKit.NSAppearance.appearanceNamed_(AppKit.NSAppearanceNameDarkAqua))
        if AppKit.NSWorkspace.sharedWorkspace().accessibilityDisplayShouldReduceTransparency():
            native_window.setOpaque_(True)
            window.setProperty("glassBackend", "reduced-transparency")
            return "reduced-transparency"
        parent = qt_view.superview()
        if parent is None:
            raise RuntimeError("Qt content view has no native parent")
        try:
            glass_class = objc.lookUpClass("NSGlassEffectView")
            backend = "NSGlassEffectView"
        except objc.nosuchclass_error:
            glass_class = AppKit.NSVisualEffectView
            backend = "NSVisualEffectView"
        glass = glass_class.alloc().initWithFrame_(qt_view.frame())
        glass.setAutoresizingMask_(AppKit.NSViewWidthSizable | AppKit.NSViewHeightSizable)
        if backend == "NSGlassEffectView":
            glass.setStyle_(AppKit.NSGlassEffectViewStyleRegular)
            glass.setCornerRadius_(14.0)
            glass.setTintColor_(AppKit.NSColor.colorWithSRGBRed_green_blue_alpha_(0.10, 0.12, 0.18, 0.30))
            glass.setContentView_(AppKit.NSView.alloc().initWithFrame_(glass.bounds()))
        else:
            glass.setMaterial_(AppKit.NSVisualEffectMaterialSidebar)
            glass.setBlendingMode_(AppKit.NSVisualEffectBlendingModeBehindWindow)
            glass.setState_(AppKit.NSVisualEffectStateActive)
        parent.addSubview_positioned_relativeTo_(glass, AppKit.NSWindowBelow, qt_view)
        # Keep the window/titlebar readable over busy windows. The glass view
        # supplies the material inside this opaque shell, rather than exposing
        # the desktop directly behind text.
        native_window.setOpaque_(True)
        native_window.setBackgroundColor_(AppKit.NSColor.colorWithSRGBRed_green_blue_alpha_(0.06, 0.07, 0.10, 1.0))
        native_window.setTitlebarAppearsTransparent_(False)
        window._macos_glass = glass
        window.setProperty("nativeGlass", True)
        window.setProperty("glassBackend", backend)
        window.style().unpolish(window)
        window.style().polish(window)
        window.update()
        return backend
    except (ImportError, AttributeError, RuntimeError, ValueError) as exc:
        logging.getLogger(__name__).warning("Native macOS glass unavailable: %s", exc)
        window.setProperty("glassBackend", "portable")
        return "portable"
