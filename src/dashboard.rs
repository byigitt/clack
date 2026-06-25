//! Native dashboard window (AppKit). Shows on launch and whenever the Dock icon
//! is clicked. Controls write through `Shared`, the same path the menu uses.
#![allow(unused_unsafe)] // objc2 marks some AppKit setters safe; keep blocks uniform

use std::cell::RefCell;

use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{define_class, msg_send, DefinedClass, MainThreadOnly};
use objc2_app_kit::{
    NSApplication, NSApplicationDelegate, NSBackingStoreType, NSButton, NSControlStateValueOff,
    NSControlStateValueOn, NSPopUpButton, NSSlider, NSStackView, NSTextField,
    NSUserInterfaceLayoutOrientation, NSWindow, NSWindowStyleMask,
};
use objc2_foundation::{
    ns_string, MainThreadMarker, NSObject, NSObjectProtocol, NSPoint, NSRect, NSSize, NSString,
};

use crate::state::Shared;

const PITCH_STEPS: [f32; 4] = [0.0, 1.0, 2.0, 4.0];

pub struct Ivars {
    shared: Shared,
    kb_indices: Vec<usize>,
    window: RefCell<Option<Retained<NSWindow>>>,
}

define_class!(
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "ClackController"]
    #[ivars = Ivars]
    pub struct Controller;

    unsafe impl NSObjectProtocol for Controller {}

    unsafe impl NSApplicationDelegate for Controller {
        #[unsafe(method(applicationShouldHandleReopen:hasVisibleWindows:))]
        fn reopen(&self, _app: &NSApplication, _has_visible: bool) -> bool {
            self.show();
            true
        }
    }

    impl Controller {
        #[unsafe(method(toggleEnabled:))]
        fn toggle_enabled(&self, sender: &NSButton) {
            self.ivars().shared.set_enabled(checked(sender));
        }

        #[unsafe(method(volumeChanged:))]
        fn volume_changed(&self, sender: &NSSlider) {
            let v = unsafe { sender.doubleValue() } as f32 / 100.0;
            self.ivars().shared.set_volume(v);
        }

        #[unsafe(method(pitchChanged:))]
        fn pitch_changed(&self, sender: &NSPopUpButton) {
            let i = unsafe { sender.indexOfSelectedItem() }.max(0) as usize;
            let semis = PITCH_STEPS.get(i).copied().unwrap_or(0.0);
            self.ivars().shared.set_pitch(semis);
        }

        #[unsafe(method(packChanged:))]
        fn pack_changed(&self, sender: &NSPopUpButton) {
            let i = unsafe { sender.indexOfSelectedItem() }.max(0) as usize;
            if let Some(&global) = self.ivars().kb_indices.get(i) {
                self.ivars().shared.set_pack(global);
            }
        }

        #[unsafe(method(toggleRapid:))]
        fn toggle_rapid(&self, sender: &NSButton) {
            self.ivars().shared.set_ignore_rapid(checked(sender));
        }

        #[unsafe(method(toggleModifiers:))]
        fn toggle_modifiers(&self, sender: &NSButton) {
            self.ivars().shared.set_disable_modifiers(checked(sender));
        }

        #[unsafe(method(toggleLogin:))]
        fn toggle_login(&self, sender: &NSButton) {
            self.ivars().shared.set_launch_at_login(checked(sender));
        }
    }
);

fn checked(b: &NSButton) -> bool {
    unsafe { b.state() == NSControlStateValueOn }
}

impl Controller {
    pub fn new(mtm: MainThreadMarker, shared: Shared) -> Retained<Self> {
        let kb_indices: Vec<usize> = shared
            .packs
            .iter()
            .enumerate()
            .filter(|(_, p)| p.category == "keyboard")
            .map(|(i, _)| i)
            .collect();

        let this = Self::alloc(mtm).set_ivars(Ivars {
            shared,
            kb_indices,
            window: RefCell::new(None),
        });
        let this: Retained<Self> = unsafe { msg_send![super(this), init] };
        this.build_window(mtm);
        this
    }

    /// Show (or re-show) the dashboard window.
    pub fn show(&self) {
        if let Some(w) = self.ivars().window.borrow().as_ref() {
            w.makeKeyAndOrderFront(None);
            let mtm = MainThreadMarker::from(self);
            #[allow(deprecated)]
            NSApplication::sharedApplication(mtm).activateIgnoringOtherApps(true);
        }
    }

    fn build_window(&self, mtm: MainThreadMarker) {
        let shared = &self.ivars().shared;
        let target: &AnyObject = self;

        let stack = unsafe { NSStackView::new(mtm) };
        unsafe {
            stack.setOrientation(NSUserInterfaceLayoutOrientation::Vertical);
            stack.setSpacing(10.0);
            stack.setAlignment(objc2_app_kit::NSLayoutAttribute::Leading);
            stack.setEdgeInsets(objc2_foundation::NSEdgeInsets {
                top: 18.0,
                left: 20.0,
                bottom: 18.0,
                right: 20.0,
            });
        }

        // Title + status.
        let title = label(mtm, "clack", 20.0, true);
        let pack_name = shared.bank.load().name.clone();
        let status = label(mtm, &format!("Soundpack: {pack_name}"), 11.0, false);
        unsafe {
            stack.addArrangedSubview(&title);
            stack.addArrangedSubview(&status);
        }

        // Enable.
        let enable = checkbox(mtm, "Enable clack", shared.enabled(), target, sel_enable());
        unsafe { stack.addArrangedSubview(&enable) };

        // Volume slider.
        unsafe {
            let vlabel = label(mtm, "Volume", 11.0, false);
            stack.addArrangedSubview(&vlabel);
            let slider = NSSlider::sliderWithValue_minValue_maxValue_target_action(
                (shared.volume() * 100.0) as f64,
                0.0,
                100.0,
                Some(target),
                Some(sel_volume()),
                mtm,
            );
            slider.setContinuous(true);
            stack.addArrangedSubview(&slider);
        }

        // Pitch popup.
        unsafe {
            let plabel = label(mtm, "Pitch variation", 11.0, false);
            stack.addArrangedSubview(&plabel);
            let popup = NSPopUpButton::new(mtm);
            for t in ["Off", "Subtle", "Medium", "Wild"] {
                popup.addItemWithTitle(&NSString::from_str(t));
            }
            let cur = shared.pitch();
            let idx = PITCH_STEPS.iter().position(|&s| s == cur).unwrap_or(0);
            popup.selectItemAtIndex(idx as isize);
            popup.setTarget(Some(target));
            popup.setAction(Some(sel_pitch()));
            stack.addArrangedSubview(&popup);
        }

        // Soundpack popup.
        unsafe {
            let slabel = label(mtm, "Soundpack", 11.0, false);
            stack.addArrangedSubview(&slabel);
            let popup = NSPopUpButton::new(mtm);
            for &i in &self.ivars().kb_indices {
                popup.addItemWithTitle(&NSString::from_str(&shared.packs[i].name));
            }
            if let Some(active) = shared.current_pack_index() {
                if let Some(pos) = self.ivars().kb_indices.iter().position(|&i| i == active) {
                    popup.selectItemAtIndex(pos as isize);
                }
            }
            popup.setTarget(Some(target));
            popup.setAction(Some(sel_pack()));
            stack.addArrangedSubview(&popup);
        }

        // Quick settings.
        unsafe {
            let rapid = checkbox(
                mtm,
                "Ignore rapid keys (<10ms)",
                shared.ignore_rapid(),
                target,
                sel_rapid(),
            );
            stack.addArrangedSubview(&rapid);
            let mods = checkbox(
                mtm,
                "Disable modifier sounds",
                shared.disable_modifiers(),
                target,
                sel_mods(),
            );
            stack.addArrangedSubview(&mods);
            let login = checkbox(
                mtm,
                "Launch at login",
                shared.launch_at_login(),
                target,
                sel_login(),
            );
            stack.addArrangedSubview(&login);
        }

        let style = NSWindowStyleMask::Titled
            | NSWindowStyleMask::Closable
            | NSWindowStyleMask::Miniaturizable;
        let frame = NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(320.0, 460.0));
        let window = unsafe {
            NSWindow::initWithContentRect_styleMask_backing_defer(
                NSWindow::alloc(mtm),
                frame,
                style,
                NSBackingStoreType::Buffered,
                false,
            )
        };
        unsafe {
            window.setTitle(ns_string!("clack"));
            window.setReleasedWhenClosed(false);
            window.setContentView(Some(&stack));
            window.center();
            window.makeKeyAndOrderFront(None);
        }

        *self.ivars().window.borrow_mut() = Some(window);
    }
}

// --- helpers ---

fn label(mtm: MainThreadMarker, text: &str, size: f64, bold: bool) -> Retained<NSTextField> {
    let s = NSString::from_str(text);
    let field = unsafe { NSTextField::labelWithString(&s, mtm) };
    unsafe {
        let font = if bold {
            objc2_app_kit::NSFont::boldSystemFontOfSize(size)
        } else {
            objc2_app_kit::NSFont::systemFontOfSize(size)
        };
        field.setFont(Some(&font));
    }
    field
}

fn checkbox(
    mtm: MainThreadMarker,
    title: &str,
    on: bool,
    target: &AnyObject,
    action: objc2::runtime::Sel,
) -> Retained<NSButton> {
    let s = NSString::from_str(title);
    let btn = unsafe {
        NSButton::checkboxWithTitle_target_action(&s, Some(target), Some(action), mtm)
    };
    unsafe {
        btn.setState(if on {
            NSControlStateValueOn
        } else {
            NSControlStateValueOff
        });
    }
    btn
}

fn sel_enable() -> objc2::runtime::Sel {
    objc2::sel!(toggleEnabled:)
}
fn sel_volume() -> objc2::runtime::Sel {
    objc2::sel!(volumeChanged:)
}
fn sel_pitch() -> objc2::runtime::Sel {
    objc2::sel!(pitchChanged:)
}
fn sel_pack() -> objc2::runtime::Sel {
    objc2::sel!(packChanged:)
}
fn sel_rapid() -> objc2::runtime::Sel {
    objc2::sel!(toggleRapid:)
}
fn sel_mods() -> objc2::runtime::Sel {
    objc2::sel!(toggleModifiers:)
}
fn sel_login() -> objc2::runtime::Sel {
    objc2::sel!(toggleLogin:)
}
