//! Listening-reminder scheduler.
//!
//! A single background task sleeps until the nearest due reminder (or a snoozed
//! notification's re-fire time), wakes, fires everything due, then recomputes.
//! It is also woken immediately via a process-global `Notify` when a reminder is
//! created / cancelled / snoozed, so a freshly-added near-future reminder fires
//! on time. Overdue reminders (daemon was down) fire once on the first pass; a
//! recurring reminder advances past all missed occurrences to the next future
//! one. All state is in SQLite, so the schedule survives restarts.
//!
//! Modeled on `server::spawn_retention_loop` (same bg-runtime + shutdown-watch
//! pattern).

use std::sync::Arc;
use std::time::Duration;

use chrono::Months;
use tokio::sync::Notify;
use tokio::task::JoinHandle;

use spotuify_core::{now_ms, Notification, NotificationState, Recurrence, Reminder};
use spotuify_protocol::DaemonEvent;

use crate::state::DaemonState;

/// Process-global wake signal: handlers nudge the scheduler without threading a
/// field through every `DaemonState` constructor (there is exactly one daemon
/// process and one scheduler).
fn wake_signal() -> &'static Notify {
    static WAKE: std::sync::OnceLock<Notify> = std::sync::OnceLock::new();
    WAKE.get_or_init(Notify::new)
}

/// Recompute the scheduler's next wake (call after create/cancel/snooze).
pub(crate) fn wake_scheduler() {
    wake_signal().notify_one();
}

pub(crate) fn spawn_reminder_loop(state: Arc<DaemonState>) -> JoinHandle<()> {
    let bg = state.bg_runtime_handle();
    bg.spawn(async move {
        let mut shutdown_rx = state.shutdown_receiver();
        loop {
            let sleep = next_sleep(&state).await;
            tokio::select! {
                _ = tokio::time::sleep(sleep) => fire_due(&state).await,
                _ = wake_signal().notified() => {} // recompute on next loop
                changed = shutdown_rx.changed() => {
                    if changed.is_err() || *shutdown_rx.borrow_and_update() {
                        break;
                    }
                }
            }
        }
    })
}

/// Duration until the next due time; clamps overdue to 0 and idles ~1h when
/// nothing is scheduled (still woken early by `wake_scheduler`).
async fn next_sleep(state: &DaemonState) -> Duration {
    match state.store().next_reminder_wake_ms().await {
        Ok(Some(t)) => {
            let now = now_ms();
            if t <= now {
                Duration::from_millis(0)
            } else {
                Duration::from_millis((t - now) as u64)
            }
        }
        Ok(None) => Duration::from_secs(3600),
        Err(err) => {
            tracing::warn!(error = %err, "reminder wake query failed");
            Duration::from_secs(60)
        }
    }
}

async fn fire_due(state: &DaemonState) {
    let now = now_ms();
    match state.store().due_reminders(now).await {
        Ok(due) => {
            for reminder in due {
                fire_reminder(state, &reminder, now).await;
            }
        }
        Err(err) => tracing::warn!(error = %err, "due reminders query failed"),
    }
    match state.store().due_snoozed_notifications(now).await {
        Ok(snoozed) => {
            for notification in snoozed {
                refire_snoozed(state, notification).await;
            }
        }
        Err(err) => tracing::warn!(error = %err, "snoozed notifications query failed"),
    }
}

async fn fire_reminder(state: &DaemonState, reminder: &Reminder, now: i64) {
    let notification = Notification {
        id: uuid::Uuid::now_v7().to_string(),
        reminder_id: reminder.id.clone(),
        media_uri: reminder.media_uri.clone(),
        media_kind: reminder.media_kind.clone(),
        name: reminder.name.clone(),
        subtitle: reminder.subtitle.clone(),
        image_url: reminder.image_url.clone(),
        due_at_ms: reminder.next_due_at_ms,
        fired_at_ms: now,
        state: NotificationState::Unseen,
        snoozed_until_ms: None,
        acted: None,
        message: reminder.message.clone(),
    };
    if let Err(err) = state.store().insert_notification(&notification).await {
        tracing::warn!(error = %err, "failed to persist reminder notification");
        return;
    }
    state.emit_event(DaemonEvent::ReminderDue { notification });

    if reminder.recurrence.is_recurring() {
        let next = next_occurrence(reminder.next_due_at_ms, reminder.recurrence);
        if let Err(err) = state.store().advance_reminder(&reminder.id, next).await {
            tracing::warn!(error = %err, "failed to advance recurring reminder");
        }
    } else if let Err(err) = state.store().complete_reminder(&reminder.id).await {
        tracing::warn!(error = %err, "failed to complete reminder");
    }
}

async fn refire_snoozed(state: &DaemonState, mut notification: Notification) {
    let _ = state
        .store()
        .set_notification_state(&notification.id, NotificationState::Unseen, None, None)
        .await;
    notification.state = NotificationState::Unseen;
    notification.snoozed_until_ms = None;
    state.emit_event(DaemonEvent::ReminderDue { notification });
}

/// First `next_due_at` for a freshly created reminder. One-shot uses the anchor
/// as-is (a past anchor fires immediately). A recurring reminder with a past
/// anchor jumps to the next future occurrence.
pub(crate) fn initial_next_due(anchor_at_ms: i64, recurrence: Recurrence) -> i64 {
    if anchor_at_ms > now_ms() || !recurrence.is_recurring() {
        anchor_at_ms
    } else {
        next_occurrence(anchor_at_ms, recurrence)
    }
}

/// Next occurrence strictly after now, skipping any missed occurrences (so a
/// long-downed daemon fires once and schedules the next *future* time).
fn next_occurrence(from_ms: i64, recurrence: Recurrence) -> i64 {
    let now = now_ms();
    let mut next = step(from_ms, recurrence);
    for _ in 0..10_000 {
        if next > now {
            break;
        }
        next = step(next, recurrence);
    }
    next
}

fn step(from_ms: i64, recurrence: Recurrence) -> i64 {
    const DAY_MS: i64 = 86_400_000;
    match recurrence {
        Recurrence::Daily => from_ms + DAY_MS,
        Recurrence::Weekly => from_ms + 7 * DAY_MS,
        // Calendar-aware month add (UTC); wall-clock drift across DST is
        // cosmetic for the inbox — the macOS OS alert uses local wall-clock
        // components so "9am" stays "9am".
        Recurrence::Monthly => chrono::DateTime::from_timestamp_millis(from_ms)
            .and_then(|dt| dt.checked_add_months(Months::new(1)))
            .map_or(from_ms + 30 * DAY_MS, |dt| dt.timestamp_millis()),
        Recurrence::None => from_ms + DAY_MS,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn weekly_step_is_seven_days() {
        assert_eq!(step(0, Recurrence::Weekly), 7 * 86_400_000);
    }

    #[test]
    fn next_occurrence_skips_missed_and_lands_in_future() {
        // from far in the past, daily → must end strictly after now.
        let past = now_ms() - 30 * 86_400_000;
        let next = next_occurrence(past, Recurrence::Daily);
        assert!(next > now_ms());
        // and it should be within one day of now (the next aligned slot).
        assert!(next - now_ms() <= 86_400_000);
    }
}
