//! Tauri shell: thin commands over [`jobs::JobManager`], and the composition root.

mod jobs;

use std::sync::Arc;

use mpt_core::{ConversionError, ConversionRequest, Job, ToolDescriptor};
use mpt_tools::Registry;
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

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let sink = Arc::new(TauriSink(app.handle().clone()));
            app.manage(JobManager::new(Registry::new(), sink));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![list_tools, list_jobs, start_jobs, cancel_job])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
