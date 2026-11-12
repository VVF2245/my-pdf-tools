//! Tauri shell: thin commands over [`jobs::JobManager`], and the composition root.

mod jobs;

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
        .invoke_handler(tauri::generate_handler![list_tools, list_jobs, start_jobs, cancel_job])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
