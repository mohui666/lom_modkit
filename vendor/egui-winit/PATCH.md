# Windows IME backport

Source: crates.io `egui-winit` 0.33.3, unchanged dependency versions and platform features.
The original MIT / Apache-2.0 licenses are included.

Backports the Windows keyboard-event handling from
[emilk/egui#7967](https://github.com/emilk/egui/pull/7967), which fixes
[emilk/egui#7908](https://github.com/emilk/egui/issues/7908).
Windows marks IME-consumed key presses as `NamedKey::Process`. Drop these before
the logical-to-physical fallback, and drop their corresponding key releases.
Clear tracked keys when the text field no longer allows IME; do not clear them
on composition completion, because a processed key release can arrive afterward.

The private keyboard handler accepts the unchanged event fields separately so
the regression exercises the real backend-to-TextEdit path without a native window.
The old handler reproduces deletion of the last committed character. The patched
handler preserves it and still allows ordinary Backspace afterward.

Run: `cargo test --locked -p egui-winit ime_processed_backspace`

This patch does not replace the existing preedit handling or upgrade the GUI stack.
