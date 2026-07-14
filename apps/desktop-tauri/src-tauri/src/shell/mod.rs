//! Centralized shell behavior: surface transitions, window positioning,
//! and helpers shared across tray, shortcut, and single-instance entry points.

use std::sync::{LazyLock, Mutex, MutexGuard, TryLockError};

use crate::surface::SurfaceMode;
use crate::surface_target::SurfaceTarget;

pub(crate) mod dwm;
mod geometry;
mod position;
pub mod settings_window;
mod transition;
mod window;

#[cfg(test)]
mod tests;

#[allow(unused_imports)]
pub use position::{
    default_surface_position, inferred_tray_panel_position, reanchor_tray_panel_position,
    remember_current_geometry_if_settings, shortcut_panel_position, tray_panel_position,
};
pub use transition::{
    handle_tray_panel_click, reopen_to_target, toggle_tray_panel, transition_to_target,
};
#[allow(unused_imports)]
pub use window::{
    apply_window_properties, hide_to_tray, hide_to_tray_if_current, hide_to_tray_state,
    try_hide_to_tray_if_current,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShellTransitionRequest {
    pub mode: SurfaceMode,
    pub target: SurfaceTarget,
    pub position: Option<(i32, i32)>,
}

pub(super) static SHELL_TRANSITION_SERIAL: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

pub(super) fn lock_shell_transition_serial() -> Result<MutexGuard<'static, ()>, String> {
    // Recover from poisoning: a panic while the serial guard was held must not
    // permanently brick every future shell transition (reopen/hide/reveal).
    Ok(SHELL_TRANSITION_SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner()))
}

pub(super) fn try_lock_shell_transition_serial() -> Result<Option<MutexGuard<'static, ()>>, String>
{
    try_lock_transition_serial(&SHELL_TRANSITION_SERIAL)
}

pub(super) fn try_lock_transition_serial(
    serial: &Mutex<()>,
) -> Result<Option<MutexGuard<'_, ()>>, String> {
    match serial.try_lock() {
        Ok(guard) => Ok(Some(guard)),
        Err(TryLockError::WouldBlock) => Ok(None),
        // Recover the guard rather than bricking the non-blocking hide path.
        Err(TryLockError::Poisoned(poisoned)) => Ok(Some(poisoned.into_inner())),
    }
}
