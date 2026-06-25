//! Launch at login via SMAppService. Only works for a registered `.app` bundle
//! (it errors harmlessly when run as a bare `cargo run` binary).

use objc2_service_management::SMAppService;

pub fn set(enable: bool) {
    unsafe {
        let svc = SMAppService::mainAppService();
        let res = if enable {
            svc.registerAndReturnError()
        } else {
            svc.unregisterAndReturnError()
        };
        if let Err(e) = res {
            let verb = if enable { "register" } else { "unregister" };
            eprintln!("clack: launch-at-login {verb} failed (needs a bundled .app): {e:?}");
        }
    }
}
