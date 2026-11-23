//! Tauri shell: thin commands over [`jobs::JobManager`], and the composition root.

mod jobs;
mod reveal;

use std::sync::Arc;

use mpt_core::{ConversionError, ConversionRequest, Job, ToolDescriptor};
use mpt_tools::{Registry, ToolsConfig};
use tauri::{AppHandle, Emitter, Manager, State};

use jobs::{JobManager, JobSink};

/// Name of the event carrying every job state change (payload: `Job`).
const JOB_EVENT: &str = "job-updated";

struct TauriSink(AppHandle);

impl JobSink for TauriSink {
    fn updated(&self, job: &Job) {
        // The window may be closing; a lost UI update is not an error.
        let _ = self.0.emit(JOB_EVENT, job);
    }
}

#[tauri::command]
fn list_tools(jobs: State<'_, JobManager>) -> Vec<ToolDescriptor> {
    jobs.tools()
}

#[tauri::command]
fn list_jobs(jobs: State<'_, JobManager>) -> Vec<Job> {
    jobs.list()
}

#[tauri::command]
fn start_jobs(
    jobs: State<'_, JobManager>,
    request: ConversionRequest,
) -> Result<Vec<Job>, ConversionError> {
    jobs.start(request)
}

#[tauri::command]
fn cancel_job(jobs: State<'_, JobManager>, id: String) {
    jobs.cancel(&id);
}

/// The user's Downloads folder, where results go unless another one is chosen. `None` when
/// the system does not know it: the window then says that results go next to the originals.
#[tauri::command]
fn default_output_dir(app: AppHandle) -> Option<String> {
    app.path().download_dir().ok().map(|p| p.to_string_lossy().into_owned())
}

/// Opens the file manager with `path` selected. Only results of this session's jobs.
#[tauri::command]
fn reveal_in_folder(jobs: State<'_, JobManager>, path: String) -> Result<(), ConversionError> {
    reveal::reveal(&path, &jobs.list())
}

/// Whether `path` is an existing folder (a remembered destination may be gone).
#[tauri::command]
fn folder_exists(path: String) -> bool {
    std::path::Path::new(&path).is_dir()
}

/// Where the tools look for the PDFium library: the packaged resource folder of the app
/// first, then the usual places next to the executable (see `ToolsConfig::new`).
fn tools_config(app: &AppHandle) -> ToolsConfig {
    let mut folders = Vec::new();
    if let Ok(resources) = app.path().resource_dir() {
        folders.push(resources.join("pdfium"));
        folders.push(resources);
    }
    // `npm run tauri dev`: use the library `scripts/get-pdfium` downloaded into the repo.
    #[cfg(debug_assertions)]
    {
        let downloads = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../pdfium");
        folders.push(downloads.join("win-x64/bin"));
        folders.push(downloads.join("mac-univ/lib"));
    }
    ToolsConfig::new(folders)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let sink = Arc::new(TauriSink(app.handle().clone()));
            app.manage(JobManager::new(Registry::new(&tools_config(app.handle())), sink));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            list_tools,
            list_jobs,
            start_jobs,
            cancel_job,
            default_output_dir,
            reveal_in_folder,
            folder_exists
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
