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
use iced::widget::Id;
use iced_runtime::core::Rectangle;
use iced_runtime::core::widget::operation::{Focusable, Operation, Outcome};
use iced_runtime::task as runtime_task;

use crate::types::{InputId, Message};

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
