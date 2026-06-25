//! Global keyboard capture via CGEventTap. The callback is deliberately tiny:
//! map keycode -> key name -> random sample, then push a `Trigger` across the
//! lock-free ring to the audio thread. No decoding, no I/O, no allocation in the
//! hot path (beyond an Arc clone + ring push).

use std::collections::HashSet;
use std::ffi::c_void;
use std::ptr::NonNull;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;

use arc_swap::ArcSwap;
use objc2_core_foundation::{CFMachPort, CFRunLoop, kCFRunLoopCommonModes};
use objc2_core_graphics::{
    CGEvent, CGEventField, CGEventMask, CGEventTapCallBack, CGEventTapLocation, CGEventTapOptions,
    CGEventTapPlacement, CGEventTapProxy, CGEventType,
};

use crate::audio::bank::SoundBank;
use crate::audio::engine::{Trigger, TriggerTx};

/// Shared, mutable-from-main-thread state the tap callback works with.
pub struct TapState {
    pub tx: TriggerTx,
    pub bank: Arc<ArcSwap<SoundBank>>,
    pub enabled: Arc<AtomicBool>,
    pub pitch: Arc<AtomicU32>, // semitone variation, f32 bits
    pub disable_modifiers: Arc<AtomicBool>,
    pub ignore_rapid: Arc<AtomicBool>,
    pressed: HashSet<i64>,
    last_event_ns: u64,
    port: *const CFMachPort,
}

impl TapState {
    pub fn new(
        tx: TriggerTx,
        bank: Arc<ArcSwap<SoundBank>>,
        enabled: Arc<AtomicBool>,
        pitch: Arc<AtomicU32>,
        disable_modifiers: Arc<AtomicBool>,
        ignore_rapid: Arc<AtomicBool>,
    ) -> Self {
        TapState {
            tx,
            bank,
            enabled,
            pitch,
            disable_modifiers,
            ignore_rapid,
            pressed: HashSet::new(),
            last_event_ns: 0,
            port: std::ptr::null(),
        }
    }
}

const TAP_DISABLED_BY_TIMEOUT: u32 = 0xFFFF_FFFE;
const TAP_DISABLED_BY_USER_INPUT: u32 = 0xFFFF_FFFF;

unsafe extern "C-unwind" fn callback(
    _proxy: CGEventTapProxy,
    event_type: CGEventType,
    event: NonNull<CGEvent>,
    user_info: *mut c_void,
) -> *mut CGEvent {
    let state = unsafe { &mut *(user_info as *mut TapState) };
    let raw = event_type.0;

    // The system disables a tap if it ever times out; just re-enable it.
    if raw == TAP_DISABLED_BY_TIMEOUT || raw == TAP_DISABLED_BY_USER_INPUT {
        if let Some(port) = unsafe { state.port.as_ref() } {
            CGEvent::tap_enable(port, true);
        }
        return event.as_ptr();
    }

    let ev = unsafe { event.as_ref() };
    let code = CGEvent::integer_value_field(Some(ev), CGEventField::KeyboardEventKeycode);

    let (key_down, fire) = match event_type {
        CGEventType::KeyDown => {
            if state.pressed.contains(&code) {
                (true, false) // key-repeat: ignore
            } else {
                state.pressed.insert(code);
                (true, true)
            }
        }
        CGEventType::KeyUp => {
            state.pressed.remove(&code);
            (false, true)
        }
        CGEventType::FlagsChanged => {
            if state.disable_modifiers.load(Ordering::Relaxed) {
                (true, false)
            } else if state.pressed.remove(&code) {
                (false, true) // modifier released
            } else {
                state.pressed.insert(code);
                (true, true) // modifier pressed
            }
        }
        _ => (true, false),
    };

    if fire {
        // Optional rapid-event guard (<10ms between events).
        if state.ignore_rapid.load(Ordering::Relaxed) {
            let now = now_ns();
            if now.saturating_sub(state.last_event_ns) < 10_000_000 {
                state.last_event_ns = now;
                return event.as_ptr();
            }
            state.last_event_ns = now;
        }
        play(state, code, key_down);
    }

    event.as_ptr()
}

fn play(state: &mut TapState, code: i64, key_down: bool) {
    if !state.enabled.load(Ordering::Relaxed) {
        return;
    }
    let bank = state.bank.load();
    let name = crate::input::keymap::key_name(code);
    let id = if key_down {
        bank.pick_down(name)
    } else {
        bank.pick_up(name)
    };
    let Some(id) = id else { return };
    let s = bank.sample(id);

    let variation = f32::from_bits(state.pitch.load(Ordering::Relaxed));
    let ratio = if variation > 0.0 {
        let semis = (fastrand::f32() * 2.0 - 1.0) * variation;
        2f32.powf(semis / 12.0)
    } else {
        1.0
    };

    let _ = state.tx.push(Trigger {
        data: s.data.clone(),
        frames: s.frames,
        gain: 1.0,
        ratio,
    });
}

#[inline]
fn now_ns() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0)
}

/// Install the event tap on the current (main) run loop. Leaks `state` so it
/// lives for the whole process. Returns false if the tap couldn't be created
/// (usually missing Accessibility permission).
pub fn install(state: TapState) -> bool {
    // Leak the state so the pointer stays valid for the tap's lifetime.
    let state_ptr = Box::into_raw(Box::new(state));

    const CB: CGEventTapCallBack = Some(callback);
    let mask: CGEventMask = (1u64 << CGEventType::KeyDown.0)
        | (1u64 << CGEventType::KeyUp.0)
        | (1u64 << CGEventType::FlagsChanged.0);

    let port = unsafe {
        CGEvent::tap_create(
            CGEventTapLocation::HIDEventTap,
            CGEventTapPlacement(1), // tailAppendEventTap
            CGEventTapOptions(1),   // listenOnly
            mask,
            CB,
            state_ptr as *mut c_void,
        )
    };

    let Some(port) = port else {
        eprintln!("clack: failed to create event tap (Accessibility permission?)");
        return false;
    };

    // Store the port pointer back into state so the callback can re-enable it.
    unsafe { (*state_ptr).port = &*port as *const CFMachPort };

    let source = CFMachPort::new_run_loop_source(None, Some(&port), 0);
    let Some(source) = source else {
        eprintln!("clack: failed to create run loop source");
        return false;
    };

    if let Some(rl) = CFRunLoop::main() {
        unsafe {
            rl.add_source(Some(&source), kCFRunLoopCommonModes);
        }
    }
    CGEvent::tap_enable(&port, true);

    // Keep port + source alive for the process lifetime.
    std::mem::forget(port);
    std::mem::forget(source);
    eprintln!("clack: event tap installed");
    true
}
