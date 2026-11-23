//! "Show in folder": opens the system file manager with a result selected.

use std::path::{Path, PathBuf};
use std::process::Command;

use mpt_core::{ConversionError, Job, JobStatus};

/// Whether `path` is a result written by one of `jobs`: nothing else may be revealed, so a
/// script in the window cannot point the file manager at any file on disk.
pub fn is_known_output(jobs: &[Job], path: &str) -> bool {
    jobs.iter().any(|job| match &job.status {
        JobStatus::Succeeded { outcome } => outcome.outputs.iter().any(|o| o == path),
        _ => false,
    })
}

/// Only an existing file may be revealed: the path comes from the window, and the file
/// manager must not be pointed at anything else.
pub fn checked_file(path: &str) -> Result<PathBuf, ConversionError> {
    let path = PathBuf::from(path);
    if path.is_file() {
        Ok(path)
    } else {
        Err(ConversionError::Io { message: format!("not a file: {}", path.display()) })
    }
}

/// Waits for the child in a thread of its own, so it does not stay a zombie on Unix.
fn reap(mut child: std::process::Child) {
    std::thread::spawn(move || {
        let _ = child.wait();
    });
}

#[cfg(target_os = "windows")]
fn open_file_manager(file: &Path) -> std::io::Result<()> {
    use std::os::windows::process::CommandExt;
    // explorer wants backslashes, and the path quoted by hand (`raw_arg`): it reads
    // `/select,"C:\a b\c"` but not an argument quoted as a whole. Its exit code is not
    // reliable (1 on success), so only a failure to start is an error.
    let native = file.to_string_lossy().replace('/', "\\");
    reap(Command::new("explorer").raw_arg(format!("/select,\"{native}\"")).spawn()?);
    Ok(())
}

#[cfg(target_os = "macos")]
fn open_file_manager(file: &Path) -> std::io::Result<()> {
    reap(Command::new("open").arg("-R").arg(file).spawn()?);
    Ok(())
}

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
fn open_file_manager(file: &Path) -> std::io::Result<()> {
    // Linux is not a target: open the folder, without selecting the file.
    let folder = file.parent().unwrap_or(file);
    reap(Command::new("xdg-open").arg(folder).spawn()?);
    Ok(())
}

pub fn reveal(path: &str, jobs: &[Job]) -> Result<(), ConversionError> {
    if !is_known_output(jobs, path) {
        return Err(ConversionError::Io { message: format!("not a result of this session: {path}") });
    }
    let file = checked_file(path)?;
    open_file_manager(&file).map_err(ConversionError::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn job_with(status: JobStatus) -> Job {
        Job { id: "1".into(), tool_id: "t".into(), input: "in".into(), status }
    }

    #[test]
    fn only_results_of_finished_jobs_may_be_revealed() {
        use mpt_core::ConversionOutcome;
        let done = job_with(JobStatus::Succeeded {
            outcome: ConversionOutcome { outputs: vec!["a/out.pdf".into()], ..Default::default() },
        });
        let failed = job_with(JobStatus::Cancelled);
        let jobs = [done, failed];
        assert!(is_known_output(&jobs, "a/out.pdf"));
        assert!(!is_known_output(&jobs, "a/other.pdf"));
        assert!(!is_known_output(&[], "a/out.pdf"));
    }

    #[test]
    fn only_existing_files_are_accepted() {
        let here = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        assert!(checked_file(&here.join("Cargo.toml").to_string_lossy()).is_ok());
        // a folder and a missing path are refused
        assert!(matches!(checked_file(&here.to_string_lossy()), Err(ConversionError::Io { .. })));
        assert!(matches!(checked_file("no/such/file.pdf"), Err(ConversionError::Io { .. })));
        assert!(matches!(checked_file(""), Err(ConversionError::Io { .. })));
    }
}
