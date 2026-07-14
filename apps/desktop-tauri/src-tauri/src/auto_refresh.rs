use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

use pulsebar::settings::Settings;
use tauri::Manager;
use tokio::sync::Notify;

use crate::state::AppState;

const AUTO_REFRESH_POLL_INTERVAL: Duration = Duration::from_secs(15);

/// Set from the app's `RunEvent::ExitRequested` handler so the background poll
/// loop stops promptly and never outlives the process.
static SHUTDOWN_REQUESTED: AtomicBool = AtomicBool::new(false);
static SHUTDOWN_NOTIFY: LazyLock<Notify> = LazyLock::new(Notify::new);

/// Request the background auto-refresh loop to shut down. Wakes the loop out of
/// its sleep immediately; the `AtomicBool` guards against a missed wake-up if
/// the loop is between iterations when this is called.
pub fn request_shutdown() {
    SHUTDOWN_REQUESTED.store(true, Ordering::SeqCst);
    SHUTDOWN_NOTIFY.notify_waiters();
}

fn shutdown_requested() -> bool {
    SHUTDOWN_REQUESTED.load(Ordering::SeqCst)
}

pub fn install(app: tauri::AppHandle) {
    tauri::async_runtime::spawn(async move {
        loop {
            if shutdown_requested() {
                break;
            }
            if should_refresh(&app) {
                let _ = crate::commands::do_refresh_providers_if_stale(&app).await;
            }
            tokio::select! {
                _ = tokio::time::sleep(AUTO_REFRESH_POLL_INTERVAL) => {}
                _ = SHUTDOWN_NOTIFY.notified() => {}
            }
        }
        tracing::debug!("auto_refresh: background loop stopped");
    });
}

fn should_refresh(app: &tauri::AppHandle) -> bool {
    let settings = Settings::load();
    let Some(interval) = refresh_interval(settings.refresh_interval_secs) else {
        return false;
    };

    let state = app.state::<Mutex<AppState>>();
    state
        .lock()
        .map(|guard| should_refresh_from_state(&guard, interval))
        .unwrap_or(false)
}

fn refresh_interval(seconds: u64) -> Option<Duration> {
    (seconds > 0).then(|| Duration::from_secs(seconds))
}

fn should_refresh_from_state(state: &AppState, interval: Duration) -> bool {
    if state.is_refreshing {
        return false;
    }
    last_provider_refresh_at(state.provider_cache_updated_at, state.app_started_at).elapsed()
        >= interval
}

fn last_provider_refresh_at(updated_at: Option<Instant>, app_started_at: Instant) -> Instant {
    updated_at.unwrap_or(app_started_at)
}

#[cfg(test)]
pub(crate) fn should_refresh_from_values(
    is_refreshing: bool,
    updated_at: Option<Instant>,
    app_started_at: Instant,
    interval_secs: u64,
) -> bool {
    let Some(interval) = refresh_interval(interval_secs) else {
        return false;
    };
    if is_refreshing {
        return false;
    }
    last_provider_refresh_at(updated_at, app_started_at).elapsed() >= interval
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manual_refresh_setting_disables_background_refresh() {
        assert!(!should_refresh_from_values(
            false,
            None,
            Instant::now() - Duration::from_secs(999),
            0,
        ));
    }

    #[test]
    fn missing_cache_waits_for_configured_interval() {
        assert!(!should_refresh_from_values(
            false,
            None,
            Instant::now() - Duration::from_secs(299),
            300,
        ));
    }

    #[test]
    fn missing_cache_refreshes_after_configured_interval() {
        assert!(should_refresh_from_values(
            false,
            None,
            Instant::now() - Duration::from_secs(300),
            300,
        ));
    }

    #[test]
    fn fresh_cache_does_not_refresh_before_interval() {
        assert!(!should_refresh_from_values(
            false,
            Some(Instant::now() - Duration::from_secs(299)),
            Instant::now() - Duration::from_secs(999),
            300,
        ));
    }

    #[test]
    fn stale_cache_refreshes_after_configured_interval() {
        assert!(should_refresh_from_values(
            false,
            Some(Instant::now() - Duration::from_secs(300)),
            Instant::now() - Duration::from_secs(999),
            300,
        ));
    }

    #[test]
    fn active_refresh_blocks_overlapping_background_refresh() {
        assert!(!should_refresh_from_values(
            true,
            None,
            Instant::now() - Duration::from_secs(999),
            300,
        ));
    }

    #[test]
    fn shutdown_request_is_observable() {
        // This test owns the global flag; restore it so it never leaks to
        // other tests in the process.
        let previous = SHUTDOWN_REQUESTED.swap(false, Ordering::SeqCst);
        assert!(!shutdown_requested());
        request_shutdown();
        assert!(shutdown_requested());
        SHUTDOWN_REQUESTED.store(previous, Ordering::SeqCst);
    }
}
