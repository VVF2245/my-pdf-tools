//! Job manager: turns a request into one job per input file, runs them on a
//! worker thread, and notifies a [`JobSink`] on every state change.
//!
//! It knows nothing about Tauri, so it is unit-tested without a window.

use std::collections::BTreeMap;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use mpt_core::{
    CancelToken, ConversionError, ConversionRequest, Job, JobStatus, Progress, Tool,
    ToolDescriptor,
};
use mpt_tools::Registry;

/// Receives every job state change (the Tauri shell turns it into an event).
pub trait JobSink: Send + Sync + 'static {
    fn updated(&self, job: &Job);
}

struct Entry {
    job: Job,
    cancel: CancelToken,
}

type Jobs = Arc<Mutex<BTreeMap<u64, Entry>>>;

pub struct JobManager {
    registry: Registry,
    sink: Arc<dyn JobSink>,
    jobs: Jobs,
    next_id: AtomicU64,
}

fn lock(jobs: &Jobs) -> MutexGuard<'_, BTreeMap<u64, Entry>> {
    // A poisoned lock only means another job thread panicked; the data is still usable.
    jobs.lock().unwrap_or_else(|e| e.into_inner())
}

fn set_status(jobs: &Jobs, sink: &dyn JobSink, id: u64, status: JobStatus) {
    let snapshot = {
        let mut map = lock(jobs);
        let Some(entry) = map.get_mut(&id) else { return };
        // A finished job never changes again (e.g. a late progress report).
        if entry.job.status.is_finished() {
            return;
        }
        entry.job.status = status;
        entry.job.clone()
    };
    sink.updated(&snapshot);
}

struct JobProgress {
    jobs: Jobs,
    sink: Arc<dyn JobSink>,
    id: u64,
}

impl Progress for JobProgress {
    fn report(&self, fraction: f32) {
        set_status(
            &self.jobs,
            self.sink.as_ref(),
            self.id,
            JobStatus::Running { progress: fraction.clamp(0.0, 1.0) },
        );
    }
}

impl JobManager {
    pub fn new(registry: Registry, sink: Arc<dyn JobSink>) -> Self {
        Self { registry, sink, jobs: Jobs::default(), next_id: AtomicU64::new(1) }
    }

    pub fn tools(&self) -> Vec<ToolDescriptor> {
        self.registry.descriptors()
    }

    pub fn list(&self) -> Vec<Job> {
        lock(&self.jobs).values().map(|e| e.job.clone()).collect()
    }

    /// Creates one job per input and starts running them in order on a worker thread.
    pub fn start(&self, request: ConversionRequest) -> Result<Vec<Job>, ConversionError> {
        let tool = self.registry.get(&request.tool_id).ok_or_else(|| ConversionError::Internal {
            message: format!("unknown tool: {}", request.tool_id),
        })?;

        let mut queue: Vec<(u64, ConversionRequest)> = Vec::new();
        let mut created = Vec::new();
        for input in &request.inputs {
            let id = self.next_id.fetch_add(1, Ordering::Relaxed);
            let job = Job {
                id: id.to_string(),
                tool_id: request.tool_id.clone(),
                input: input.clone(),
                status: JobStatus::Queued,
            };
            lock(&self.jobs).insert(id, Entry { job: job.clone(), cancel: CancelToken::new() });
            created.push(job);
            queue.push((id, ConversionRequest { inputs: vec![input.clone()], ..request.clone() }));
        }

        for job in &created {
            self.sink.updated(job);
        }

        let jobs = self.jobs.clone();
        let sink = self.sink.clone();
        std::thread::spawn(move || {
            for (id, single) in queue {
                run_one(&*tool, &jobs, &sink, id, &single);
            }
        });
        Ok(created)
    }

    /// Requests cancellation; a queued job becomes `Cancelled` when its turn comes.
    pub fn cancel(&self, id: &str) {
        let Ok(id) = id.parse::<u64>() else { return };
        if let Some(entry) = lock(&self.jobs).get(&id) {
            entry.cancel.cancel();
        }
    }
}

fn run_one(
    tool: &dyn Tool,
    jobs: &Jobs,
    sink: &Arc<dyn JobSink>,
    id: u64,
    request: &ConversionRequest,
) {
    let cancel = match lock(jobs).get(&id) {
        Some(e) => e.cancel.clone(),
        None => return,
    };
    if cancel.is_cancelled() {
        set_status(jobs, sink.as_ref(), id, JobStatus::Cancelled);
        return;
    }
    set_status(jobs, sink.as_ref(), id, JobStatus::Running { progress: 0.0 });

    let progress = JobProgress { jobs: jobs.clone(), sink: sink.clone(), id };
    let result = catch_unwind(AssertUnwindSafe(|| tool.run(request, &progress, &cancel)));
    let status = match result {
        Ok(Ok(outcome)) => JobStatus::Succeeded { outcome },
        Ok(Err(ConversionError::Cancelled)) => JobStatus::Cancelled,
        Ok(Err(error)) => JobStatus::Failed { error },
        Err(_) => JobStatus::Failed {
            error: ConversionError::Internal { message: "the tool panicked".to_string() },
        },
    };
    set_status(jobs, sink.as_ref(), id, status);
}

#[cfg(test)]
mod tests {
    use super::*;
    use mpt_core::ConversionOutcome;
    use std::collections::BTreeMap as Map;
    use std::time::{Duration, Instant};

    #[derive(Default)]
    struct Collector(Mutex<Vec<Job>>);

    impl JobSink for Collector {
        fn updated(&self, job: &Job) {
            self.0.lock().unwrap().push(job.clone());
        }
    }

    /// Behaves according to the input name: `fail`, `panic`, `block` (until cancelled) or ok.
    struct FakeTool;

    impl Tool for FakeTool {
        fn descriptor(&self) -> ToolDescriptor {
            ToolDescriptor {
                id: "fake".into(),
                name: "n".into(),
                description: "d".into(),
                inputs: vec![],
                outputs: vec![],
                options: vec![],
            }
        }

        fn run(
            &self,
            request: &ConversionRequest,
            progress: &dyn Progress,
            cancel: &CancelToken,
        ) -> Result<ConversionOutcome, ConversionError> {
            let input = request.inputs[0].as_str();
            match input {
                "fail" => Err(ConversionError::Corrupt),
                "panic" => panic!("boom"),
                "block" => loop {
                    cancel.check()?;
                    std::thread::sleep(Duration::from_millis(5));
                },
                _ => {
                    progress.report(0.5);
                    Ok(ConversionOutcome { outputs: vec![format!("{input}.out")], ..Default::default() })
                }
            }
        }
    }

    fn manager() -> (JobManager, Arc<Collector>) {
        let sink = Arc::new(Collector::default());
        let registry = Registry::from_tools(vec![Arc::new(FakeTool)]);
        (JobManager::new(registry, sink.clone()), sink)
    }

    fn request(inputs: &[&str]) -> ConversionRequest {
        ConversionRequest {
            tool_id: "fake".into(),
            inputs: inputs.iter().map(|s| s.to_string()).collect(),
            output_dir: None,
            options: Map::new(),
        }
    }

    fn wait_until(manager: &JobManager, done: impl Fn(&[Job]) -> bool) -> Vec<Job> {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let jobs = manager.list();
            if done(&jobs) {
                return jobs;
            }
            assert!(Instant::now() < deadline, "timed out waiting for jobs: {jobs:?}");
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    fn all_finished(jobs: &[Job]) -> bool {
        jobs.iter().all(|j| j.status.is_finished())
    }

    #[test]
    fn one_job_per_input_run_in_order() {
        let (m, sink) = manager();
        let created = m.start(request(&["a", "b"])).unwrap();
        assert_eq!(created.len(), 2);
        assert!(created.iter().all(|j| j.status == JobStatus::Queued));

        let jobs = wait_until(&m, all_finished);
        assert!(matches!(jobs[0].status, JobStatus::Succeeded { .. }));
        assert!(matches!(jobs[1].status, JobStatus::Succeeded { .. }));
        // queued, running, progress, running(0.5), succeeded... for each job
        let events = sink.0.lock().unwrap();
        assert!(events.iter().any(|j| j.status == JobStatus::Running { progress: 0.5 }));
    }

    #[test]
    fn errors_and_panics_become_failed_jobs() {
        let (m, _) = manager();
        m.start(request(&["fail", "panic", "ok"])).unwrap();
        let jobs = wait_until(&m, all_finished);
        assert_eq!(jobs[0].status, JobStatus::Failed { error: ConversionError::Corrupt });
        assert!(matches!(
            jobs[1].status,
            JobStatus::Failed { error: ConversionError::Internal { .. } }
        ));
        // a failure does not stop the rest of the batch
        assert!(matches!(jobs[2].status, JobStatus::Succeeded { .. }));
    }

    #[test]
    fn cancelling_a_running_job_and_a_queued_one() {
        let (m, _) = manager();
        let created = m.start(request(&["block", "x"])).unwrap();
        wait_until(&m, |jobs| matches!(jobs[0].status, JobStatus::Running { .. }));
        m.cancel(&created[1].id); // queued behind the blocking one
        m.cancel(&created[0].id);
        let jobs = wait_until(&m, all_finished);
        assert_eq!(jobs[0].status, JobStatus::Cancelled);
        assert_eq!(jobs[1].status, JobStatus::Cancelled);
    }

    #[test]
    fn unknown_tool_is_an_error() {
        let (m, _) = manager();
        let mut req = request(&["a"]);
        req.tool_id = "nope".into();
        assert!(matches!(m.start(req), Err(ConversionError::Internal { .. })));
        assert!(m.list().is_empty());
    }
}
