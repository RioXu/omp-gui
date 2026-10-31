//! Native forms for agent definitions and role/model assignments.
mod agent;
mod model_picker;
mod roles;
use super::*;
use crate::infrastructure::configuration::authoring;
pub(super) use agent::AgentEditor;
use model_picker::ModelPicker;
pub(super) use roles::RoleEditor;

pub(super) enum EditorEvent {
    Close,
    Saved,
}
enum EditorResult {
    Models(Result<Vec<Value>, String>),
    Saved(Result<(), String>),
}

fn channel(
    omp: String,
    cwd: PathBuf,
) -> (mpsc::Sender<EditorResult>, mpsc::Receiver<EditorResult>) {
    let (tx, rx) = mpsc::channel();
    let result_tx = tx.clone();
    std::thread::spawn(move || {
        let _ = result_tx.send(EditorResult::Models(
            authoring::available_models(&omp, &cwd).map_err(|e| format!("{e:#}")),
        ));
    });
    (tx, rx)
}
