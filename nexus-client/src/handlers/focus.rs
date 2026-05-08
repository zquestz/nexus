//! Tab-cycle focus helpers for form panels.
//!
//! The whole codebase uses a "press Tab → advance to next input"
//! pattern. The naive way to implement that is a local `focused_field`
//! tracker that handlers maintain by hand on Tab presses and on
//! `on_input` events. That breaks when the user clicks into a
//! different field without typing — the click moves the actual iced
//! focus, but our local tracker stays put, and the next Tab cycles
//! from the wrong starting point.
//!
//! The fix is two-step:
//!   1. Tab handler dispatches an iced widget Operation
//!      (`find_focused`) that asks iced for the currently-focused
//!      widget id, mapped into a form-specific `*TabResolved` message.
//!   2. Resolved handler receives the actually-focused id and
//!      advances to the next input in the form's cycle.
//!
//! [`dispatch_find_focused`] handles step 1; [`next_in_cycle`]
//! handles the lookup in step 2. Each form just defines its own
//! cycle (an ordered slice of `InputId`s) and a tiny resolved
//! handler that calls these helpers.

use iced::Task;
use iced::widget::{Id, operation};
use iced_runtime::core::Rectangle;
use iced_runtime::core::widget::operation::{Focusable, Operation, Outcome};
use iced_runtime::task as runtime_task;

use crate::NexusApp;
use crate::types::{InputId, Message};

impl NexusApp {
    /// Set `focused_field` and dispatch the iced focus operation as a
    /// `Task`. Always use this instead of writing the two lines by
    /// hand — the pair (in-memory tracker + iced focus op) is easy to
    /// half-do, and drifting them apart breaks tray-restore /
    /// minimize-restore (the saved `focused_field` no longer matches
    /// what the user's eye sees) and silently mis-targets the next
    /// Tab cycle (which falls back to `focused_field` when iced's
    /// own focus query returns the empty-id sentinel).
    pub(crate) fn focus_field(&mut self, id: InputId) -> Task<Message> {
        self.focused_field = id;
        operation::focus(Id::from(id))
    }
}

/// Variant of iced's `find_focused` that **always** produces an
/// `Outcome::Some(Id)`. Iced's built-in version returns
/// `Outcome::None` when nothing is focused, which causes the
/// downstream `Task<Id>` to never fire its `.map(...)` callback —
/// meaning a Tab press with no focused widget produces no message
/// and the resolver never runs.
///
/// This wrapper substitutes `Id::new("")` as a sentinel "no focus"
/// value so the resolver always fires. The empty id never matches a
/// real `InputId`, so [`next_in_cycle`] naturally falls through to
/// its `cycle[0]` default — i.e. Tab from a no-focus state lands on
/// the first input of the active form.
///
/// Confirmed safe for iced 0.14 — no widget in this crate uses
/// `Id::new("")` and iced's default-id assignment uses internal
/// counters, not literal empties. Re-verify on iced upgrade.
fn find_focused_or_sentinel() -> impl Operation<Id> {
    struct FindFocused {
        focused: Option<Id>,
    }

    impl Operation<Id> for FindFocused {
        fn focusable(&mut self, id: Option<&Id>, _bounds: Rectangle, state: &mut dyn Focusable) {
            if state.is_focused() && id.is_some() {
                self.focused = id.cloned();
            }
        }

        fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation<Id>)) {
            operate(self);
        }

        fn finish(&self) -> Outcome<Id> {
            // Always Some — empty Id signals "no focused widget."
            Outcome::Some(self.focused.clone().unwrap_or_else(|| Id::new("")))
        }
    }

    FindFocused { focused: None }
}

/// Dispatch a Task that queries iced for the currently-focused widget
/// and maps its `Id` into the supplied form-specific resolved message.
///
/// Used by `*TabPressed` handlers. The Task fires the resolved message
/// back through the normal dispatcher, so the form's `*TabResolved`
/// handler runs with the real focused id.
pub fn dispatch_find_focused(into_message: fn(Id) -> Message) -> Task<Message> {
    runtime_task::widget(find_focused_or_sentinel()).map(into_message)
}

/// Given the id of the currently-focused widget and an ordered list
/// of `InputId`s defining a Tab cycle, return the next input to
/// focus. Wraps from the last entry back to the first.
///
/// Falls back to `cycle[0]` if `focused` doesn't match any entry —
/// e.g. nothing is focused, or focus is on a widget like
/// `iced_aw::NumberInput`'s internal text input which iced's
/// `find_focused` doesn't surface (its `Id` differs from any
/// `InputId` we own).
pub fn next_in_cycle(focused: &Id, cycle: &[InputId]) -> InputId {
    for (i, candidate) in cycle.iter().enumerate() {
        if focused == &Id::from(*candidate) {
            return cycle[(i + 1) % cycle.len()];
        }
    }
    cycle[0]
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_CYCLE: &[InputId] = &[
        InputId::TrackerBrowserAddName,
        InputId::TrackerBrowserAddAddress,
        InputId::TrackerBrowserAddPassword,
        InputId::TrackerBrowserAddFingerprint,
    ];

    #[test]
    fn advances_to_next_within_cycle() {
        let focused = Id::from(InputId::TrackerBrowserAddName);
        assert_eq!(
            next_in_cycle(&focused, SAMPLE_CYCLE),
            InputId::TrackerBrowserAddAddress
        );
    }

    #[test]
    fn advances_through_each_position() {
        // Walking from each entry returns the entry that follows it.
        for window in SAMPLE_CYCLE.windows(2) {
            let focused = Id::from(window[0]);
            assert_eq!(next_in_cycle(&focused, SAMPLE_CYCLE), window[1]);
        }
    }

    #[test]
    fn wraps_from_last_to_first() {
        let focused = Id::from(InputId::TrackerBrowserAddFingerprint);
        assert_eq!(
            next_in_cycle(&focused, SAMPLE_CYCLE),
            InputId::TrackerBrowserAddName
        );
    }

    #[test]
    fn unknown_id_falls_through_to_cycle_first() {
        // Mimics focus on a widget outside this form's cycle (e.g. a
        // NumberInput's internal text input, or a button).
        let focused = Id::from(InputId::ServerName);
        assert_eq!(
            next_in_cycle(&focused, SAMPLE_CYCLE),
            InputId::TrackerBrowserAddName
        );
    }

    #[test]
    fn sentinel_empty_id_falls_through_to_cycle_first() {
        // The `find_focused_or_sentinel` operation substitutes
        // `Id::new("")` when no widget is focused. The empty id
        // doesn't match any `InputId`, so the cycle starts at index 0.
        let focused = Id::new("");
        assert_eq!(
            next_in_cycle(&focused, SAMPLE_CYCLE),
            InputId::TrackerBrowserAddName
        );
    }

    #[test]
    fn single_element_cycle_returns_self_on_match() {
        // Length-1 cycle: focused matches the only entry, so wrap
        // (i + 1) % 1 = 0 returns the same entry.
        let cycle: &[InputId] = &[InputId::TrackerBrowserAddName];
        let focused = Id::from(InputId::TrackerBrowserAddName);
        assert_eq!(
            next_in_cycle(&focused, cycle),
            InputId::TrackerBrowserAddName
        );
    }

    #[test]
    fn single_element_cycle_returns_self_on_no_match() {
        // Length-1 cycle: no match falls through to cycle[0].
        let cycle: &[InputId] = &[InputId::TrackerBrowserAddName];
        let focused = Id::from(InputId::ServerName);
        assert_eq!(
            next_in_cycle(&focused, cycle),
            InputId::TrackerBrowserAddName
        );
    }
}
