//! Desktop coordination of OMP connection, events, queue state and model controls.
use crate::desktop::workspace::*;
pub(in crate::desktop::workspace) mod collaboration;
mod connection;
mod event_dispatch;
mod event_loop;
pub(in crate::desktop::workspace) mod model_controls;

mod model_state;
pub(in crate::desktop::workspace) use model_state::ModelControlState;

mod session_state;
pub(in crate::desktop::workspace) use session_state::SessionRuntime;
mod sessions;
