//! Native UI: a status-bar menu (with a real volume slider) and a dashboard
//! window. Both surfaces share one `Controller` and write through `Shared`, so
//! the menu and the window stay behaviourally identical.
#![allow(unused_unsafe)] // objc2 marks some AppKit setters safe; keep blocks uniform

use std::cell::RefCell;

use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{define_class, msg_send, DefinedClass, MainThreadOnly};
use objc2_app_kit::{
    NSApplication, NSApplicationDelegate, NSBackingStoreType, NSBox, NSBoxType, NSColor,
    NSControlStateValue, NSControlStateValueOff, NSControlStateValueOn, NSFont, NSImage,
    NSImageView, NSLayoutAttribute, NSMenu, NSMenuItem, NSPopUpButton,
    NSSegmentedControl, NSSlider, NSStackView, NSStackViewGravity, NSStatusBar, NSStatusItem,
    NSSwitch, NSTextField, NSTitlePosition, NSUserInterfaceLayoutOrientation,
    NSVariableStatusItemLength, NSVisualEffectBlendingMode, NSVisualEffectMaterial,
    NSVisualEffectState, NSVisualEffectView, NSView, NSWindow, NSWindowStyleMask,
};
use objc2_foundation::{
    ns_string, MainThreadMarker, NSObject, NSObjectProtocol, NSPoint, NSRect, NSSize, NSString,
};

use crate::state::Shared;

const PITCH_STEPS: [f32; 4] = [0.0, 1.0, 2.0, 4.0];
const PITCH_LABELS: [&str; 4] = ["Off", "Subtle", "Medium", "Wild"];

pub struct Ivars {
    shared: Shared,
    kb_indices: Vec<usize>,
    window: RefCell<Option<Retained<NSWindow>>>,
    status: RefCell<Option<Retained<NSStatusItem>>>,
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
        // --- dashboard window controls (NSSwitch / slider / segmented / popup) ---
        #[unsafe(method(swEnabled:))]
        fn dash_enabled(&self, sender: &NSSwitch) {
            self.ivars().shared.set_enabled(sw_on(sender));
        }
        #[unsafe(method(volumeChanged:))]
        fn volume_changed(&self, sender: &NSSlider) {
            self.ivars().shared.set_volume(unsafe { sender.doubleValue() } as f32 / 100.0);
        }
        #[unsafe(method(pitchSeg:))]
        fn dash_pitch(&self, sender: &NSSegmentedControl) {
            let i = unsafe { sender.selectedSegment() }.max(0) as usize;
            self.ivars().shared.set_pitch(PITCH_STEPS.get(i).copied().unwrap_or(0.0));
        }
        #[unsafe(method(packChanged:))]
        fn dash_pack(&self, sender: &NSPopUpButton) {
            let i = unsafe { sender.indexOfSelectedItem() }.max(0) as usize;
            if let Some(&g) = self.ivars().kb_indices.get(i) {
                self.ivars().shared.set_pack(g);
            }
        }
        #[unsafe(method(swRapid:))]
        fn dash_rapid(&self, sender: &NSSwitch) {
            self.ivars().shared.set_ignore_rapid(sw_on(sender));
        }
        #[unsafe(method(swMods:))]
        fn dash_mods(&self, sender: &NSSwitch) {
            self.ivars().shared.set_disable_modifiers(sw_on(sender));
        }
        #[unsafe(method(swLogin:))]
        fn dash_login(&self, sender: &NSSwitch) {
            self.ivars().shared.set_launch_at_login(sw_on(sender));
        }

        // --- status-bar menu actions ---
        #[unsafe(method(menuEnable:))]
        fn menu_enable(&self, sender: &NSMenuItem) {
            let v = !self.ivars().shared.enabled();
            self.ivars().shared.set_enabled(v);
            unsafe { sender.setState(state(v)) };
        }
        #[unsafe(method(menuRapid:))]
        fn menu_rapid(&self, sender: &NSMenuItem) {
            let v = !self.ivars().shared.ignore_rapid();
            self.ivars().shared.set_ignore_rapid(v);
            unsafe { sender.setState(state(v)) };
        }
        #[unsafe(method(menuMods:))]
        fn menu_mods(&self, sender: &NSMenuItem) {
            let v = !self.ivars().shared.disable_modifiers();
            self.ivars().shared.set_disable_modifiers(v);
            unsafe { sender.setState(state(v)) };
        }
        #[unsafe(method(menuLogin:))]
        fn menu_login(&self, sender: &NSMenuItem) {
            let v = !self.ivars().shared.launch_at_login();
            self.ivars().shared.set_launch_at_login(v);
            unsafe { sender.setState(state(v)) };
        }
        #[unsafe(method(menuPitch:))]
        fn menu_pitch(&self, sender: &NSMenuItem) {
            let tag = unsafe { sender.tag() }.max(0) as usize;
            self.ivars().shared.set_pitch(PITCH_STEPS.get(tag).copied().unwrap_or(0.0));
            radio(sender);
        }
        #[unsafe(method(menuPack:))]
        fn menu_pack(&self, sender: &NSMenuItem) {
            let tag = unsafe { sender.tag() }.max(0) as usize;
            self.ivars().shared.set_pack(tag);
            radio(sender);
        }
        #[unsafe(method(openDashboard:))]
        fn open_dashboard(&self, _sender: &NSMenuItem) {
            self.show();
        }
        #[unsafe(method(quitClack:))]
        fn quit(&self, _sender: &NSMenuItem) {
            let mtm = MainThreadMarker::from(self);
            unsafe { NSApplication::sharedApplication(mtm).terminate(None) };
        }
    }
);

fn sw_on(s: &NSSwitch) -> bool {
    unsafe { s.state() == NSControlStateValueOn }
}
fn state(on: bool) -> NSControlStateValue {
    if on {
        NSControlStateValueOn
    } else {
        NSControlStateValueOff
    }
}
/// Set the clicked item On and all its siblings Off (radio behaviour).
fn radio(sender: &NSMenuItem) {
    unsafe {
        if let Some(menu) = sender.menu() {
            for it in menu.itemArray().iter() {
                it.setState(NSControlStateValueOff);
            }
        }
        sender.setState(NSControlStateValueOn);
    }
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
            status: RefCell::new(None),
        });
        let this: Retained<Self> = unsafe { msg_send![super(this), init] };
        this.build_status_item(mtm);
        this.build_window(mtm);
        this
    }

    pub fn show(&self) {
        if let Some(w) = self.ivars().window.borrow().as_ref() {
            w.makeKeyAndOrderFront(None);
            let mtm = MainThreadMarker::from(self);
            #[allow(deprecated)]
            NSApplication::sharedApplication(mtm).activateIgnoringOtherApps(true);
        }
    }

    // ---- status-bar menu ----

    fn build_status_item(&self, mtm: MainThreadMarker) {
        let shared = &self.ivars().shared;
        let target: &AnyObject = self;
        let item = unsafe {
            NSStatusBar::systemStatusBar().statusItemWithLength(NSVariableStatusItemLength)
        };
        if let Some(btn) = item.button(mtm) {
            let img = unsafe {
                NSImage::imageWithSystemSymbolName_accessibilityDescription(
                    ns_string!("keyboard"),
                    None,
                )
            };
            unsafe { btn.setImage(img.as_deref()) };
        }

        let menu = NSMenu::new(mtm);

        // Enable
        let enable = menu_item(mtm, "Enable clack", target, sel(c"menuEnable:"));
        unsafe { enable.setState(state(shared.enabled())) };
        menu.addItem(&enable);
        let sep = unsafe { NSMenuItem::separatorItem(mtm) };
        menu.addItem(&sep);

        // Volume slider (custom view)
        menu.addItem(&label_item(mtm, "Volume"));
        let slider = unsafe {
            NSSlider::sliderWithValue_minValue_maxValue_target_action(
                (shared.volume() * 100.0) as f64,
                0.0,
                100.0,
                Some(target),
                Some(sel(c"volumeChanged:")),
                mtm,
            )
        };
        unsafe { slider.setContinuous(true) };
        menu.addItem(&slider_item(mtm, &slider));

        // Pitch submenu (radio)
        let pitch_sub = NSMenu::new(mtm);
        let cur_pitch = shared.pitch();
        for (i, lbl) in PITCH_LABELS.iter().enumerate() {
            let it = menu_item(mtm, lbl, target, sel(c"menuPitch:"));
            unsafe {
                it.setTag(i as isize);
                it.setState(state(PITCH_STEPS[i] == cur_pitch));
            }
            pitch_sub.addItem(&it);
        }
        let pitch_root = menu_item(mtm, "Pitch variation", target, sel(c"noop:"));
        unsafe {
            pitch_root.setTarget(None);
            pitch_root.setSubmenu(Some(&pitch_sub));
        }
        menu.addItem(&pitch_root);

        // Soundpack submenu (radio)
        let pack_sub = NSMenu::new(mtm);
        let active = shared.current_pack_index();
        for &g in &self.ivars().kb_indices {
            let it = menu_item(mtm, &shared.packs[g].name, target, sel(c"menuPack:"));
            unsafe {
                it.setTag(g as isize);
                it.setState(state(Some(g) == active));
            }
            pack_sub.addItem(&it);
        }
        let pack_root = menu_item(mtm, "Soundpack", target, sel(c"noop:"));
        unsafe {
            pack_root.setTarget(None);
            pack_root.setSubmenu(Some(&pack_sub));
        }
        menu.addItem(&pack_root);
        let sep = unsafe { NSMenuItem::separatorItem(mtm) };
        menu.addItem(&sep);

        // Quick settings
        let rapid = menu_item(mtm, "Ignore rapid keys (<10ms)", target, sel(c"menuRapid:"));
        unsafe { rapid.setState(state(shared.ignore_rapid())) };
        menu.addItem(&rapid);
        let mods = menu_item(mtm, "Disable modifier sounds", target, sel(c"menuMods:"));
        unsafe { mods.setState(state(shared.disable_modifiers())) };
        menu.addItem(&mods);
        let login = menu_item(mtm, "Launch at login", target, sel(c"menuLogin:"));
        unsafe { login.setState(state(shared.launch_at_login())) };
        menu.addItem(&login);
        let sep = unsafe { NSMenuItem::separatorItem(mtm) };
        menu.addItem(&sep);

        // Dashboard + Quit
        menu.addItem(&menu_item(mtm, "Open Dashboard\u{2026}", target, sel(c"openDashboard:")));
        menu.addItem(&menu_item(mtm, "Quit clack", target, sel(c"quitClack:")));

        unsafe { item.setMenu(Some(&menu)) };
        *self.ivars().status.borrow_mut() = Some(item);
    }

    // ---- dashboard window ----

    fn build_window(&self, mtm: MainThreadMarker) {
        let shared = &self.ivars().shared;
        let target: &AnyObject = self;
        const W: f64 = 312.0; // inner content width

        // Header: keyboard glyph + title/subtitle.
        let glyph = unsafe { NSImageView::new(mtm) };
        unsafe {
            if let Some(img) = NSImage::imageWithSystemSymbolName_accessibilityDescription(
                ns_string!("keyboard"),
                None,
            ) {
                glyph.setImage(Some(&img));
            }
            glyph.setContentTintColor(Some(&NSColor::controlAccentColor()));
            size(&glyph, 30.0, 26.0);
        }
        let titles = vstack(mtm, 1.0);
        unsafe {
            titles.addArrangedSubview(&label(mtm, "clack", 17.0, Weight::Bold, false));
            titles.addArrangedSubview(&label(
                mtm,
                "mechanical keyboard sounds",
                11.0,
                Weight::Regular,
                true,
            ));
        }
        let header = hstack(mtm, 10.0);
        unsafe {
            header.addArrangedSubview(&glyph);
            header.addArrangedSubview(&titles);
        }

        // --- SOUND card ---
        let enable = switch(mtm, shared.enabled(), target, sel(c"swEnabled:"));
        let slider = unsafe {
            NSSlider::sliderWithValue_minValue_maxValue_target_action(
                (shared.volume() * 100.0) as f64,
                0.0,
                100.0,
                Some(target),
                Some(sel(c"volumeChanged:")),
                mtm,
            )
        };
        unsafe { slider.setContinuous(true) };
        width(&slider, 150.0);

        let labels: Vec<Retained<NSString>> =
            PITCH_LABELS.iter().map(|t| NSString::from_str(t)).collect();
        let arr = objc2_foundation::NSArray::from_retained_slice(&labels);
        let seg = unsafe {
            NSSegmentedControl::segmentedControlWithLabels_trackingMode_target_action(
                &arr,
                objc2_app_kit::NSSegmentSwitchTracking::SelectOne,
                Some(target),
                Some(sel(c"pitchSeg:")),
                mtm,
            )
        };
        let pidx = PITCH_STEPS.iter().position(|&s| s == shared.pitch()).unwrap_or(0);
        unsafe { seg.setSelectedSegment(pidx as isize) };

        let packs = unsafe { NSPopUpButton::new(mtm) };
        unsafe {
            for &g in &self.ivars().kb_indices {
                packs.addItemWithTitle(&NSString::from_str(&shared.packs[g].name));
            }
            if let Some(a) = shared.current_pack_index() {
                if let Some(pos) = self.ivars().kb_indices.iter().position(|&i| i == a) {
                    packs.selectItemAtIndex(pos as isize);
                }
            }
            packs.setTarget(Some(target));
            packs.setAction(Some(sel(c"packChanged:")));
        }
        width(&packs, 170.0);

        let sound = card(
            mtm,
            "SOUND",
            W,
            vec![
                row(mtm, "Enable", &enable, W),
                row(mtm, "Volume", &slider, W),
                row(mtm, "Pitch", &seg, W),
                row(mtm, "Soundpack", &packs, W),
            ],
        );

        // --- BEHAVIOR card ---
        let rapid = switch(mtm, shared.ignore_rapid(), target, sel(c"swRapid:"));
        let mods = switch(mtm, shared.disable_modifiers(), target, sel(c"swMods:"));
        let login = switch(mtm, shared.launch_at_login(), target, sel(c"swLogin:"));
        let behavior = card(
            mtm,
            "BEHAVIOR",
            W,
            vec![
                row(mtm, "Ignore rapid keys", &rapid, W),
                row(mtm, "Disable modifier sounds", &mods, W),
                row(mtm, "Launch at login", &login, W),
            ],
        );

        let main = vstack(mtm, 14.0);
        unsafe {
            main.addArrangedSubview(&header);
            main.addArrangedSubview(&sound);
            main.addArrangedSubview(&behavior);
            main.setTranslatesAutoresizingMaskIntoConstraints(false);
        }

        // Frosted background.
        let effect = unsafe { NSVisualEffectView::new(mtm) };
        unsafe {
            effect.setMaterial(NSVisualEffectMaterial::WindowBackground);
            effect.setBlendingMode(NSVisualEffectBlendingMode::BehindWindow);
            effect.setState(NSVisualEffectState::Active);
            effect.addSubview(&main);
        }
        pin(&main, &effect, 20.0);

        let style = NSWindowStyleMask::Titled
            | NSWindowStyleMask::Closable
            | NSWindowStyleMask::Miniaturizable
            | NSWindowStyleMask::FullSizeContentView;
        let frame = NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(W + 40.0, 412.0));
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
            window.setTitlebarAppearsTransparent(true);
            window.setTitleVisibility(objc2_app_kit::NSWindowTitleVisibility::Hidden);
            window.setMovableByWindowBackground(true);
            window.setReleasedWhenClosed(false);
            window.setContentView(Some(&effect));
            window.center();
            window.makeKeyAndOrderFront(None);
        }
        *self.ivars().window.borrow_mut() = Some(window);
    }
}

// --- helpers ---

enum Weight {
    Regular,
    Bold,
}

fn label(mtm: MainThreadMarker, text: &str, size: f64, w: Weight, secondary: bool) -> Retained<NSTextField> {
    let field = unsafe { NSTextField::labelWithString(&NSString::from_str(text), mtm) };
    unsafe {
        let font = match w {
            Weight::Bold => NSFont::boldSystemFontOfSize(size),
            Weight::Regular => NSFont::systemFontOfSize(size),
        };
        field.setFont(Some(&font));
        if secondary {
            field.setTextColor(Some(&NSColor::secondaryLabelColor()));
        }
    }
    field
}

fn label_item(mtm: MainThreadMarker, text: &str) -> Retained<NSMenuItem> {
    let it = NSMenuItem::new(mtm);
    unsafe {
        it.setTitle(&NSString::from_str(text));
        it.setEnabled(false);
    }
    it
}

fn slider_item(mtm: MainThreadMarker, slider: &NSSlider) -> Retained<NSMenuItem> {
    let container = unsafe {
        NSView::initWithFrame(
            NSView::alloc(mtm),
            NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(220.0, 28.0)),
        )
    };
    unsafe {
        slider.setFrame(NSRect::new(NSPoint::new(20.0, 4.0), NSSize::new(184.0, 20.0)));
        container.addSubview(slider);
    }
    let it = NSMenuItem::new(mtm);
    unsafe { it.setView(Some(&container)) };
    it
}

fn menu_item(
    mtm: MainThreadMarker,
    title: &str,
    target: &AnyObject,
    action: objc2::runtime::Sel,
) -> Retained<NSMenuItem> {
    let it = NSMenuItem::new(mtm);
    unsafe {
        it.setTitle(&NSString::from_str(title));
        it.setTarget(Some(target));
        it.setAction(Some(action));
    }
    it
}

fn vstack(mtm: MainThreadMarker, spacing: f64) -> Retained<NSStackView> {
    let s = unsafe { NSStackView::new(mtm) };
    unsafe {
        s.setOrientation(NSUserInterfaceLayoutOrientation::Vertical);
        s.setAlignment(NSLayoutAttribute::Leading);
        s.setSpacing(spacing);
    }
    s
}

fn hstack(mtm: MainThreadMarker, spacing: f64) -> Retained<NSStackView> {
    let s = unsafe { NSStackView::new(mtm) };
    unsafe {
        s.setOrientation(NSUserInterfaceLayoutOrientation::Horizontal);
        s.setAlignment(NSLayoutAttribute::CenterY);
        s.setSpacing(spacing);
    }
    s
}

fn switch(
    mtm: MainThreadMarker,
    on: bool,
    target: &AnyObject,
    action: objc2::runtime::Sel,
) -> Retained<NSSwitch> {
    let sw = unsafe { NSSwitch::new(mtm) };
    unsafe {
        sw.setState(state(on));
        sw.setTarget(Some(target));
        sw.setAction(Some(action));
    }
    sw
}

/// A settings row: title on the left, control on the right.
fn row(mtm: MainThreadMarker, title: &str, control: &NSView, w: f64) -> Retained<NSStackView> {
    let s = hstack(mtm, 8.0);
    let lbl = label(mtm, title, 13.0, Weight::Regular, false);
    unsafe {
        s.addView_inGravity(&lbl, NSStackViewGravity::Leading);
        s.addView_inGravity(control, NSStackViewGravity::Trailing);
    }
    width(&s, w);
    s
}

/// A rounded card: an uppercase caption above a translucent panel of rows.
fn card(
    mtm: MainThreadMarker,
    title: &str,
    w: f64,
    rows: Vec<Retained<NSStackView>>,
) -> Retained<NSStackView> {
    let inner = vstack(mtm, 10.0);
    unsafe {
        inner.setEdgeInsets(objc2_foundation::NSEdgeInsets {
            top: 12.0,
            left: 14.0,
            bottom: 12.0,
            right: 14.0,
        });
        for r in &rows {
            inner.addArrangedSubview(r);
        }
    }
    let panel = unsafe { NSBox::new(mtm) };
    unsafe {
        panel.setBoxType(NSBoxType::Custom);
        panel.setTitlePosition(NSTitlePosition::NoTitle);
        panel.setCornerRadius(10.0);
        panel.setBorderWidth(0.0);
        panel.setFillColor(&NSColor::colorWithCalibratedWhite_alpha(1.0, 0.05));
        panel.setContentView(Some(&inner));
    }
    width(&panel, w);

    let wrap = vstack(mtm, 6.0);
    let caption = label(mtm, title, 10.5, Weight::Bold, true);
    unsafe {
        wrap.addArrangedSubview(&caption);
        wrap.addArrangedSubview(&panel);
    }
    wrap
}

/// Pin a view to a fixed width via Auto Layout.
fn width(v: &NSView, w: f64) {
    unsafe {
        v.setTranslatesAutoresizingMaskIntoConstraints(false);
        v.widthAnchor().constraintEqualToConstant(w).setActive(true);
    }
}

/// Fixed width + height.
fn size(v: &NSView, w: f64, h: f64) {
    unsafe {
        v.setTranslatesAutoresizingMaskIntoConstraints(false);
        v.widthAnchor().constraintEqualToConstant(w).setActive(true);
        v.heightAnchor().constraintEqualToConstant(h).setActive(true);
    }
}

/// Pin a view to its container's edges with an inset (top/leading/trailing).
fn pin(v: &NSView, container: &NSView, inset: f64) {
    unsafe {
        v.topAnchor()
            .constraintEqualToAnchor_constant(&container.topAnchor(), inset)
            .setActive(true);
        v.leadingAnchor()
            .constraintEqualToAnchor_constant(&container.leadingAnchor(), inset)
            .setActive(true);
        v.trailingAnchor()
            .constraintEqualToAnchor_constant(&container.trailingAnchor(), -inset)
            .setActive(true);
    }
}

fn sel(name: &core::ffi::CStr) -> objc2::runtime::Sel {
    objc2::runtime::Sel::register(name)
}
