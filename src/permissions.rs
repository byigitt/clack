//! Accessibility permission gate. The CGEventTap cannot see global key events
//! without it, so we check and (once) prompt the user to grant access.

use macos_accessibility_client::accessibility;

/// True if this process is already trusted for Accessibility.
#[allow(dead_code)] // used by the menu status item in a later phase
pub fn is_trusted() -> bool {
    accessibility::application_is_trusted()
}

/// Check trust and, if missing, fire the system prompt that deep-links the user
/// to Privacy & Security → Accessibility. Returns the current trust state.
// ponytail: prompt is one-shot per process; macOS only re-prompts on identity change.
pub fn ensure_trusted() -> bool {
    accessibility::application_is_trusted_with_prompt()
}
