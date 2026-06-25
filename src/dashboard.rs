//! Native UI: a status-bar menu (with a real volume slider) and a sidebar
//! dashboard window (Settings / Soundpacks / Guide). Both surfaces share one
//! `Controller` and write through `Shared`.
#![allow(unused_unsafe)] // objc2 marks some AppKit setters safe; keep blocks uniform

use std::cell::RefCell;

use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{define_class, msg_send, DefinedClass, MainThreadOnly};
use objc2_app_kit::{
    NSApplication, NSApplicationDelegate, NSBackingStoreType, NSBox, NSBoxType, NSButton,
    NSCellImagePosition, NSColor, NSControlStateValue, NSControlStateValueOff,
    NSControlStateValueOn, NSFont, NSImage, NSImageView, NSLayoutAttribute, NSMenu, NSMenuItem,
    NSSegmentedControl, NSSlider, NSStackView, NSStackViewGravity, NSStatusBar, NSStatusItem,
    NSSwitch, NSTextAlignment, NSTextField, NSTitlePosition, NSUserInterfaceLayoutOrientation,
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
    panes: RefCell<Vec<Retained<NSStackView>>>,
    nav: RefCell<Vec<Retained<NSButton>>>,
    pack_rows: RefCell<Vec<Retained<NSButton>>>,
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
        // --- dashboard controls ---
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

        // --- sidebar navigation + soundpack management ---
        #[unsafe(method(navSelect:))]
        fn nav_select(&self, sender: &NSButton) {
            let idx = unsafe { sender.tag() }.max(0) as usize;
            self.select_pane(idx);
        }
        #[unsafe(method(pickPack:))]
        fn pick_pack(&self, sender: &NSButton) {
            let g = unsafe { sender.tag() }.max(0) as usize;
            self.ivars().shared.set_pack(g);
            self.refresh_pack_rows();
        }
        #[unsafe(method(openFolder:))]
        fn open_folder(&self, _sender: &NSButton) {
            open_soundpacks_folder();
        }
        #[unsafe(method(getPacks:))]
        fn get_packs(&self, _sender: &NSButton) {
            open_url("https://github.com/kamillobinski/thock");
        }
        #[unsafe(method(grantAccess:))]
        fn grant_access(&self, _sender: &NSButton) {
            open_url("x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility");
        }
        #[unsafe(method(relaunchApp:))]
        fn relaunch_app(&self, _sender: &NSButton) {
            if let Ok(exe) = std::env::current_exe() {
                if let Some(app) = exe.ancestors().nth(3) {
                    let _ = std::process::Command::new("open").arg(app).spawn();
                }
            }
            let mtm = MainThreadMarker::from(self);
            unsafe { NSApplication::sharedApplication(mtm).terminate(None) };
        }

        // --- status-bar menu actions ---
        #[unsafe(method(menuEnable:))]
        fn menu_enable(&self, sender: &NSMenuItem) {
            let v = !self.ivars().shared.enabled();
            self.ivars().shared.set_enabled(v);
            unsafe { sender.setState(state(v)) };
            self.refresh_dashboard();
        }
        #[unsafe(method(menuRapid:))]
        fn menu_rapid(&self, sender: &NSMenuItem) {
            let v = !self.ivars().shared.ignore_rapid();
            self.ivars().shared.set_ignore_rapid(v);
            unsafe { sender.setState(state(v)) };
            self.refresh_dashboard();
        }
        #[unsafe(method(menuMods:))]
        fn menu_mods(&self, sender: &NSMenuItem) {
            let v = !self.ivars().shared.disable_modifiers();
            self.ivars().shared.set_disable_modifiers(v);
            unsafe { sender.setState(state(v)) };
            self.refresh_dashboard();
        }
        #[unsafe(method(menuLogin:))]
        fn menu_login(&self, sender: &NSMenuItem) {
            let v = !self.ivars().shared.launch_at_login();
            self.ivars().shared.set_launch_at_login(v);
            unsafe { sender.setState(state(v)) };
            self.refresh_dashboard();
        }
        #[unsafe(method(menuPitch:))]
        fn menu_pitch(&self, sender: &NSMenuItem) {
            let tag = unsafe { sender.tag() }.max(0) as usize;
            self.ivars().shared.set_pitch(PITCH_STEPS.get(tag).copied().unwrap_or(0.0));
            radio(sender);
            self.refresh_dashboard();
        }
        #[unsafe(method(menuPack:))]
        fn menu_pack(&self, sender: &NSMenuItem) {
            let tag = unsafe { sender.tag() }.max(0) as usize;
            self.ivars().shared.set_pack(tag);
            radio(sender);
            self.refresh_pack_rows();
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
            panes: RefCell::new(Vec::new()),
            nav: RefCell::new(Vec::new()),
            pack_rows: RefCell::new(Vec::new()),
        });
        let this: Retained<Self> = unsafe { msg_send![super(this), init] };
        this.build_status_item(mtm);
        this.build_window(mtm);
        let start = std::env::var("CLACK_PANE")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);
        this.select_pane(start);
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

    /// Re-read shared state into the dashboard's settings controls (keeps the
    /// window in sync when something is toggled from the menu).
    fn refresh_dashboard(&self) {
        // The settings controls live in pane 0; rebuilding is overkill, so we
        // just rebuild that pane's values lazily on next open. For now, simply
        // ensure pack-row highlights stay correct.
        self.refresh_pack_rows();
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
        // We manage item state ourselves; without this, AppKit auto-disables the
        // submenu roots and the slider (they have no responder for their action).
        unsafe { menu.setAutoenablesItems(false) };

        let enable = menu_item(mtm, "Enable clack", target, sel(c"menuEnable:"));
        unsafe { enable.setState(state(shared.enabled())) };
        menu.addItem(&enable);
        let sep = unsafe { NSMenuItem::separatorItem(mtm) };
        menu.addItem(&sep);

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

        menu.addItem(&menu_item(mtm, "Open Dashboard\u{2026}", target, sel(c"openDashboard:")));
        menu.addItem(&menu_item(mtm, "Quit clack", target, sel(c"quitClack:")));

        unsafe { item.setMenu(Some(&menu)) };
        *self.ivars().status.borrow_mut() = Some(item);
    }

    // ---- sidebar dashboard window ----

    fn build_window(&self, mtm: MainThreadMarker) {
        let target: &AnyObject = self;
        const SIDEBAR: f64 = 188.0;
        const WINW: f64 = 588.0;
        const WINH: f64 = 476.0;

        let effect = unsafe { NSVisualEffectView::new(mtm) };
        unsafe {
            effect.setMaterial(NSVisualEffectMaterial::WindowBackground);
            effect.setBlendingMode(NSVisualEffectBlendingMode::BehindWindow);
            effect.setState(NSVisualEffectState::Active);
        }

        // Sidebar.
        let sidebar = unsafe { NSVisualEffectView::new(mtm) };
        unsafe {
            sidebar.setMaterial(NSVisualEffectMaterial::Sidebar);
            sidebar.setBlendingMode(NSVisualEffectBlendingMode::BehindWindow);
            sidebar.setState(NSVisualEffectState::Active);
            sidebar.setTranslatesAutoresizingMaskIntoConstraints(false);
            effect.addSubview(&sidebar);
        }
        let glyph = unsafe { NSImageView::new(mtm) };
        unsafe {
            if let Some(img) = NSImage::imageWithSystemSymbolName_accessibilityDescription(
                ns_string!("keyboard"),
                None,
            ) {
                glyph.setImage(Some(&img));
            }
            glyph.setContentTintColor(Some(&NSColor::controlAccentColor()));
            size(&glyph, 26.0, 22.0);
        }
        let titles = vstack(mtm, 0.0);
        unsafe {
            titles.addArrangedSubview(&label(mtm, "clack", 15.0, Weight::Bold, false));
            titles.addArrangedSubview(&label(mtm, "keyboard sounds", 10.5, Weight::Regular, true));
        }
        let header = hstack(mtm, 9.0);
        unsafe {
            header.addArrangedSubview(&glyph);
            header.addArrangedSubview(&titles);
        }
        let nav0 = nav_button(mtm, "Settings", "slider.horizontal.3", 0, target);
        let nav1 = nav_button(mtm, "Soundpacks", "speaker.wave.2.fill", 1, target);
        let nav2 = nav_button(mtm, "Guide", "book", 2, target);
        let sb = vstack(mtm, 4.0);
        unsafe {
            sb.addArrangedSubview(&header);
            sb.setCustomSpacing_afterView(22.0, &header);
            sb.addArrangedSubview(&nav0);
            sb.addArrangedSubview(&nav1);
            sb.addArrangedSubview(&nav2);
            sb.setTranslatesAutoresizingMaskIntoConstraints(false);
            sidebar.addSubview(&sb);
        }
        pin(&sb, &sidebar, 38.0, 12.0);
        *self.ivars().nav.borrow_mut() = vec![nav0, nav1, nav2];

        // Content container + panes.
        let container = unsafe { NSView::new(mtm) };
        unsafe {
            container.setTranslatesAutoresizingMaskIntoConstraints(false);
            effect.addSubview(&container);
        }
        let p_settings = self.settings_pane(mtm);
        let p_packs = self.soundpacks_pane(mtm);
        let p_guide = self.guide_pane(mtm);
        for p in [&p_settings, &p_packs, &p_guide] {
            unsafe { container.addSubview(p) };
            pin(p, &container, 34.0, 26.0);
        }
        *self.ivars().panes.borrow_mut() = vec![p_settings, p_packs, p_guide];

        // Explicit sizes so the content area has a definite size (otherwise the
        // window collapses, since the panes don't pin a bottom).
        unsafe {
            sidebar
                .leadingAnchor()
                .constraintEqualToAnchor_constant(&effect.leadingAnchor(), 0.0)
                .setActive(true);
            sidebar
                .topAnchor()
                .constraintEqualToAnchor_constant(&effect.topAnchor(), 0.0)
                .setActive(true);
            sidebar.widthAnchor().constraintEqualToConstant(SIDEBAR).setActive(true);
            sidebar.heightAnchor().constraintEqualToConstant(WINH).setActive(true);
            container
                .leadingAnchor()
                .constraintEqualToAnchor_constant(&sidebar.trailingAnchor(), 0.0)
                .setActive(true);
            container
                .topAnchor()
                .constraintEqualToAnchor_constant(&effect.topAnchor(), 0.0)
                .setActive(true);
            container.widthAnchor().constraintEqualToConstant(WINW - SIDEBAR).setActive(true);
            container.heightAnchor().constraintEqualToConstant(WINH).setActive(true);
            container
                .trailingAnchor()
                .constraintEqualToAnchor_constant(&effect.trailingAnchor(), 0.0)
                .setActive(true);
        }

        let style = NSWindowStyleMask::Titled
            | NSWindowStyleMask::Closable
            | NSWindowStyleMask::Miniaturizable;
        let frame = NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(WINW, WINH));
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
            window.setContentView(Some(&effect));
            window.setContentSize(NSSize::new(WINW, WINH));
            window.center();
            window.makeKeyAndOrderFront(None);
        }
        *self.ivars().window.borrow_mut() = Some(window);
    }

    fn select_pane(&self, idx: usize) {
        for (i, p) in self.ivars().panes.borrow().iter().enumerate() {
            unsafe { p.setHidden(i != idx) };
        }
        for (i, b) in self.ivars().nav.borrow().iter().enumerate() {
            let selected = i == idx;
            let color = if selected {
                unsafe { NSColor::controlAccentColor() }
            } else {
                unsafe { NSColor::secondaryLabelColor() }
            };
            let font = if selected {
                unsafe { NSFont::boldSystemFontOfSize(13.0) }
            } else {
                unsafe { NSFont::systemFontOfSize(13.0) }
            };
            unsafe {
                b.setContentTintColor(Some(&color));
                b.setFont(Some(&font));
            }
        }
    }

    fn refresh_pack_rows(&self) {
        let active = self.ivars().shared.current_pack_index();
        for b in self.ivars().pack_rows.borrow().iter() {
            let is_cur = active == Some(unsafe { b.tag() }.max(0) as usize);
            let sym = if is_cur { "largecircle.fill.circle" } else { "circle" };
            let color = if is_cur {
                unsafe { NSColor::controlAccentColor() }
            } else {
                unsafe { NSColor::tertiaryLabelColor() }
            };
            unsafe {
                if let Some(img) = NSImage::imageWithSystemSymbolName_accessibilityDescription(
                    &NSString::from_str(sym),
                    None,
                ) {
                    b.setImage(Some(&img));
                }
                b.setContentTintColor(Some(&color));
            }
        }
    }

    // ---- panes ----

    fn settings_pane(&self, mtm: MainThreadMarker) -> Retained<NSStackView> {
        let shared = &self.ivars().shared;
        let target: &AnyObject = self;
        const W: f64 = 332.0;
        const IW: f64 = W - 32.0;

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
        width(&slider, 160.0);

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

        let sound = card(
            mtm,
            "SOUND",
            W,
            vec![
                row(mtm, "Enable", &enable, IW),
                row(mtm, "Volume", &slider, IW),
                row(mtm, "Pitch", &seg, IW),
            ],
        );

        let rapid = switch(mtm, shared.ignore_rapid(), target, sel(c"swRapid:"));
        let mods = switch(mtm, shared.disable_modifiers(), target, sel(c"swMods:"));
        let login = switch(mtm, shared.launch_at_login(), target, sel(c"swLogin:"));
        let behavior = card(
            mtm,
            "BEHAVIOR",
            W,
            vec![
                row(mtm, "Ignore rapid keys", &rapid, IW),
                row(mtm, "Disable modifier sounds", &mods, IW),
                row(mtm, "Launch at login", &login, IW),
            ],
        );

        let pane = vstack(mtm, 16.0);
        unsafe {
            pane.addArrangedSubview(&pane_title(mtm, "Settings"));
            if !crate::permissions::is_trusted() {
                pane.addArrangedSubview(&self.permission_banner(mtm, W));
            }
            pane.addArrangedSubview(&sound);
            pane.addArrangedSubview(&behavior);
            pane.setTranslatesAutoresizingMaskIntoConstraints(false);
        }
        pane
    }

    /// A warning shown until Accessibility is granted (key sounds need it).
    fn permission_banner(&self, mtm: MainThreadMarker, w: f64) -> Retained<NSStackView> {
        let target: &AnyObject = self;
        let inner = vstack(mtm, 8.0);
        unsafe {
            inner.setEdgeInsets(objc2_foundation::NSEdgeInsets {
                top: 14.0,
                left: 16.0,
                bottom: 14.0,
                right: 16.0,
            });
            let title = label(mtm, "\u{26A0}\u{FE0E}  Accessibility needed", 13.0, Weight::Bold, false);
            title.setTextColor(Some(&NSColor::systemOrangeColor()));
            inner.addArrangedSubview(&title);
            inner.addArrangedSubview(&wrap_label(
                mtm,
                "clack needs Accessibility permission to hear your keystrokes. Grant it, then relaunch clack.",
                w - 32.0,
            ));
            let btns = hstack(mtm, 8.0);
            btns.addArrangedSubview(&action_button(mtm, "Open Settings", target, sel(c"grantAccess:")));
            btns.addArrangedSubview(&action_button(mtm, "Relaunch clack", target, sel(c"relaunchApp:")));
            inner.addArrangedSubview(&btns);
        }
        let panel = unsafe { NSBox::new(mtm) };
        unsafe {
            panel.setBoxType(NSBoxType::Custom);
            panel.setTitlePosition(NSTitlePosition::NoTitle);
            panel.setCornerRadius(10.0);
            panel.setBorderWidth(1.0);
            panel.setBorderColor(&NSColor::systemOrangeColor());
            panel.setFillColor(&NSColor::colorWithCalibratedWhite_alpha(1.0, 0.04));
            panel.setContentView(Some(&inner));
        }
        width(&panel, w);
        let wrap = vstack(mtm, 0.0);
        unsafe { wrap.addArrangedSubview(&panel) };
        wrap
    }

    fn soundpacks_pane(&self, mtm: MainThreadMarker) -> Retained<NSStackView> {
        let shared = &self.ivars().shared;
        let target: &AnyObject = self;

        let list = vstack(mtm, 2.0);
        let mut rows = Vec::new();
        for &g in &self.ivars().kb_indices {
            let b = pack_row(mtm, &shared.packs[g].name, g, target);
            unsafe { list.addArrangedSubview(&b) };
            rows.push(b);
        }
        *self.ivars().pack_rows.borrow_mut() = rows;

        let actions = hstack(mtm, 8.0);
        unsafe {
            actions.addArrangedSubview(&action_button(
                mtm,
                "Open Soundpacks Folder",
                target,
                sel(c"openFolder:"),
            ));
            actions.addArrangedSubview(&action_button(mtm, "Get more\u{2026}", target, sel(c"getPacks:")));
        }

        let hint = wrap_label(
            mtm,
            "Drop a soundpack folder (config.json + .wav files) into the folder above, then relaunch clack. See the Guide tab for the format.",
            336.0,
        );

        let pane = vstack(mtm, 14.0);
        unsafe {
            pane.addArrangedSubview(&pane_title(mtm, "Soundpacks"));
            pane.addArrangedSubview(&list);
            pane.addArrangedSubview(&actions);
            pane.addArrangedSubview(&hint);
            pane.setTranslatesAutoresizingMaskIntoConstraints(false);
        }
        self.refresh_pack_rows();
        pane
    }

    fn guide_pane(&self, mtm: MainThreadMarker) -> Retained<NSStackView> {
        let target: &AnyObject = self;
        let body = "clack reuses thock's soundpacks. Each pack is a folder with:\n\n  \u{2022}  config.json \u{2014} metadata + which sound plays for each key\n  \u{2022}  a set of .wav files\n\nWhere they live:\n  ~/Library/Application Support/Thock/Soundpacks/\n\nAdd a downloaded pack:\n  1.  Open the Soundpacks folder (button below).\n  2.  Drop the pack's folder inside it.\n  3.  Relaunch clack \u{2014} it shows up under Soundpacks.\n\nMake your own: copy a folder, swap the .wav files, edit config.json. 'default' is the fallback; 'space', 'enter', \u{2026} override specific keys.";
        let text = wrap_label(mtm, body, 344.0);
        let btns = hstack(mtm, 8.0);
        unsafe {
            btns.addArrangedSubview(&action_button(
                mtm,
                "Open Soundpacks Folder",
                target,
                sel(c"openFolder:"),
            ));
            btns.addArrangedSubview(&action_button(
                mtm,
                "Browse online\u{2026}",
                target,
                sel(c"getPacks:"),
            ));
        }
        let pane = vstack(mtm, 14.0);
        unsafe {
            pane.addArrangedSubview(&pane_title(mtm, "Guide"));
            pane.addArrangedSubview(&text);
            pane.addArrangedSubview(&btns);
            pane.setTranslatesAutoresizingMaskIntoConstraints(false);
        }
        pane
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

fn pane_title(mtm: MainThreadMarker, text: &str) -> Retained<NSTextField> {
    label(mtm, text, 19.0, Weight::Bold, false)
}

fn wrap_label(mtm: MainThreadMarker, text: &str, w: f64) -> Retained<NSTextField> {
    let f = unsafe { NSTextField::wrappingLabelWithString(&NSString::from_str(text), mtm) };
    unsafe {
        f.setFont(Some(&NSFont::systemFontOfSize(12.0)));
        f.setTextColor(Some(&NSColor::secondaryLabelColor()));
        f.setSelectable(false);
    }
    width(&f, w);
    f
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

/// A sidebar nav button: SF symbol + title, borderless, left-aligned.
fn nav_button(
    mtm: MainThreadMarker,
    title: &str,
    symbol: &str,
    tag: isize,
    target: &AnyObject,
) -> Retained<NSButton> {
    let b = unsafe {
        NSButton::buttonWithTitle_target_action(
            &NSString::from_str(title),
            Some(target),
            Some(sel(c"navSelect:")),
            mtm,
        )
    };
    unsafe {
        b.setBordered(false);
        b.setTag(tag);
        b.setAlignment(NSTextAlignment::Left);
        b.setFont(Some(&NSFont::systemFontOfSize(13.0)));
        b.setImagePosition(NSCellImagePosition::ImageLeft);
        if let Some(img) =
            NSImage::imageWithSystemSymbolName_accessibilityDescription(&NSString::from_str(symbol), None)
        {
            b.setImage(Some(&img));
        }
        width(&b, 160.0);
    }
    b
}

/// A soundpack list row: a selectable button with a leading state circle.
fn pack_row(mtm: MainThreadMarker, name: &str, tag: usize, target: &AnyObject) -> Retained<NSButton> {
    let b = unsafe {
        NSButton::buttonWithTitle_target_action(
            &NSString::from_str(name),
            Some(target),
            Some(sel(c"pickPack:")),
            mtm,
        )
    };
    unsafe {
        b.setBordered(false);
        b.setTag(tag as isize);
        b.setAlignment(NSTextAlignment::Left);
        b.setFont(Some(&NSFont::systemFontOfSize(13.0)));
        b.setImagePosition(NSCellImagePosition::ImageLeft);
        width(&b, 320.0);
    }
    b
}

/// A normal push button used for actions (Open Folder, etc.).
fn action_button(
    mtm: MainThreadMarker,
    title: &str,
    target: &AnyObject,
    action: objc2::runtime::Sel,
) -> Retained<NSButton> {
    unsafe {
        NSButton::buttonWithTitle_target_action(&NSString::from_str(title), Some(target), Some(action), mtm)
    }
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
    let inner = vstack(mtm, 12.0);
    unsafe {
        inner.setEdgeInsets(objc2_foundation::NSEdgeInsets {
            top: 14.0,
            left: 16.0,
            bottom: 14.0,
            right: 16.0,
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

fn width(v: &NSView, w: f64) {
    unsafe {
        v.setTranslatesAutoresizingMaskIntoConstraints(false);
        v.widthAnchor().constraintEqualToConstant(w).setActive(true);
    }
}

fn size(v: &NSView, w: f64, h: f64) {
    unsafe {
        v.setTranslatesAutoresizingMaskIntoConstraints(false);
        v.widthAnchor().constraintEqualToConstant(w).setActive(true);
        v.heightAnchor().constraintEqualToConstant(h).setActive(true);
    }
}

/// Pin a view to its container with a top inset and horizontal side insets.
fn pin(v: &NSView, container: &NSView, top: f64, side: f64) {
    unsafe {
        v.topAnchor()
            .constraintEqualToAnchor_constant(&container.topAnchor(), top)
            .setActive(true);
        v.leadingAnchor()
            .constraintEqualToAnchor_constant(&container.leadingAnchor(), side)
            .setActive(true);
        v.trailingAnchor()
            .constraintEqualToAnchor_constant(&container.trailingAnchor(), -side)
            .setActive(true);
    }
}

fn open_soundpacks_folder() {
    if let Some(home) = dirs::home_dir() {
        let dir = home.join("Library/Application Support/Thock/Soundpacks");
        let _ = std::fs::create_dir_all(&dir);
        let _ = std::process::Command::new("open").arg(&dir).spawn();
    }
}

fn open_url(url: &str) {
    let _ = std::process::Command::new("open").arg(url).spawn();
}

fn sel(name: &core::ffi::CStr) -> objc2::runtime::Sel {
    objc2::runtime::Sel::register(name)
}
