//! Job manager: turns a request into one job per input file and runs them one after another
//! on a single worker thread, notifying a [`JobSink`] on every state change.
//!
//! All requests share one queue, so two batches never run at the same time (PDFium and the
//! memory of big files are used by one job at a time). A batch is the set of jobs created by
//! one call to [`JobManager::start`].
//!
//! It knows nothing about Tauri, so it is unit-tested without a window.

use std::collections::{BTreeMap, VecDeque};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};

use outmark_core::{
    CancelToken, ConversionError, ConversionRequest, Job, JobStatus, Progress, Tool,
    ToolDescriptor,
};
use outmark_tools::Registry;

/// Receives every job state change (the Tauri shell turns it into an event).
///
/// The notifications are sent while the manager holds its event lock, so that they keep the
/// order of the changes. A sink may read the manager (`list`), but must not change it
/// (`start`, `cancel`, `retry`...): that would wait for its own lock.
pub trait JobSink: Send + Sync + 'static {
    fn updated(&self, job: &Job);
}

struct Entry {
    job: Job,
    cancel: CancelToken,
    tool: Arc<dyn Tool>,
    /// The request of this job alone (one input).
    request: ConversionRequest,
}

#[derive(Default)]
struct State {
    jobs: BTreeMap<u64, Entry>,
    /// Ids of the queued jobs, in the order they will run.
    queue: VecDeque<u64>,
    shutdown: bool,
}

#[derive(Default)]
struct Shared {
    state: Mutex<State>,
    /// Held while a change is applied and its notifications are sent, so they stay in order.
    events: Mutex<()>,
    /// Woken when a job is queued or the manager is dropped.
    ready: Condvar,
}

pub struct JobManager {
    registry: Registry,
    sink: Arc<dyn JobSink>,
    shared: Arc<Shared>,
    next_id: AtomicU64,
    next_batch: AtomicU64,
}

fn lock(shared: &Shared) -> MutexGuard<'_, State> {
    // A poisoned lock only means another job thread panicked; the data is still usable.
    shared.state.lock().unwrap_or_else(|e| e.into_inner())
}

/// Applies a change to the state and then notifies the sink about the jobs it touched. The
/// state lock is released before the notifications, so the sink can read the manager.
fn change<R>(
    shared: &Shared,
    sink: &dyn JobSink,
    f: impl FnOnce(&mut State, &mut Vec<Job>) -> R,
) -> R {
    let _events = shared.events.lock().unwrap_or_else(|e| e.into_inner());
    let mut touched = Vec::new();
    let result = f(&mut lock(shared), &mut touched);
    for job in &touched {
        sink.updated(job);
    }
    result
}

/// Sets the status of a job, unless the job already finished (a late progress report).
fn set_status(shared: &Shared, sink: &dyn JobSink, id: u64, status: JobStatus) {
    change(shared, sink, |state, touched| {
        let Some(entry) = state.jobs.get_mut(&id) else { return };
        if entry.job.status.is_finished() {
            return;
        }
        entry.job.status = status;
        touched.push(entry.job.clone());
    });
}

struct JobProgress {
    shared: Arc<Shared>,
    sink: Arc<dyn JobSink>,
    id: u64,
}

impl Progress for JobProgress {
    fn report(&self, fraction: f32) {
        set_status(
            &self.shared,
            self.sink.as_ref(),
            self.id,
            JobStatus::Running { progress: fraction.clamp(0.0, 1.0) },
        );
    }
}

impl JobManager {
    pub fn new(registry: Registry, sink: Arc<dyn JobSink>) -> Self {
        let shared = Arc::new(Shared::default());
        let worker_shared = shared.clone();
        let worker_sink = sink.clone();
        std::thread::spawn(move || worker(worker_shared, worker_sink));
        Self { registry, sink, shared, next_id: AtomicU64::new(1), next_batch: AtomicU64::new(1) }
    }

    pub fn tools(&self) -> Vec<ToolDescriptor> {
        self.registry.descriptors()
    }

    pub fn list(&self) -> Vec<Job> {
        lock(&self.shared).jobs.values().map(|e| e.job.clone()).collect()
    }

    /// Creates one job per input, all in one new batch, and queues them behind the jobs
    /// already waiting.
    pub fn start(&self, request: ConversionRequest) -> Result<Vec<Job>, ConversionError> {
        let tool = self.registry.get(&request.tool_id).ok_or_else(|| ConversionError::Internal {
            message: format!("unknown tool: {}", request.tool_id),
        })?;
        let batch = format!("batch-{}", self.next_batch.fetch_add(1, Ordering::Relaxed));

        let created = change(&self.shared, self.sink.as_ref(), |state, touched| {
            let mut created = Vec::new();
            for input in &request.inputs {
                let id = self.next_id.fetch_add(1, Ordering::Relaxed);
                let job = Job {
                    id: id.to_string(),
                    tool_id: request.tool_id.clone(),
                    input: input.clone(),
                    batch: batch.clone(),
                    status: JobStatus::Queued,
                };
                let entry = Entry {
                    job: job.clone(),
                    cancel: CancelToken::new(),
                    tool: tool.clone(),
                    request: ConversionRequest { inputs: vec![input.clone()], ..request.clone() },
                };
                state.jobs.insert(id, entry);
                state.queue.push_back(id);
                touched.push(job.clone());
                created.push(job);
            }
            created
        });
        self.shared.ready.notify_one();
        Ok(created)
    }

    /// Requests cancellation. A queued job is cancelled at once; a running one stops at its
    /// next check.
    pub fn cancel(&self, id: &str) {
        let Ok(id) = id.parse::<u64>() else { return };
        self.cancel_ids(&[id]);
    }

    /// Cancels every unfinished job of `batch`.
    pub fn cancel_batch(&self, batch: &str) {
        let ids: Vec<u64> = lock(&self.shared)
            .jobs
            .iter()
            .filter(|(_, e)| e.job.batch == batch && !e.job.status.is_finished())
            .map(|(id, _)| *id)
            .collect();
        self.cancel_ids(&ids);
    }

    fn cancel_ids(&self, ids: &[u64]) {
        change(&self.shared, self.sink.as_ref(), |state, touched| {
            let State { jobs, queue, .. } = state;
            for id in ids {
                let Some(entry) = jobs.get_mut(id) else { continue };
                if entry.job.status.is_finished() {
                    continue;
                }
                entry.cancel.cancel();
                if matches!(entry.job.status, JobStatus::Queued) {
                    queue.retain(|queued| queued != id);
                    entry.job.status = JobStatus::Cancelled;
                    touched.push(entry.job.clone());
                }
            }
        });
    }

    /// Queues a new copy of a failed or cancelled job (same batch, same input) and removes
    /// the old one. The removal is not announced: the caller gets the full list back.
    pub fn retry(&self, id: &str) -> Result<Vec<Job>, ConversionError> {
        let old: u64 = id
            .parse()
            .map_err(|_| ConversionError::Internal { message: format!("no job {id}") })?;
        change(&self.shared, self.sink.as_ref(), |state, touched| -> Result<(), ConversionError> {
            let (batch, tool, request, tool_id) = {
                let entry = state
                    .jobs
                    .get(&old)
                    .ok_or_else(|| ConversionError::Internal { message: format!("no job {id}") })?;
                if !matches!(entry.job.status, JobStatus::Failed { .. } | JobStatus::Cancelled) {
                    return Err(ConversionError::Internal {
                        message: format!("job {id} is not failed or cancelled"),
                    });
                }
                (entry.job.batch.clone(), entry.tool.clone(), entry.request.clone(), entry.job.tool_id.clone())
            };
            state.jobs.remove(&old);

            let new = self.next_id.fetch_add(1, Ordering::Relaxed);
            let job = Job {
                id: new.to_string(),
                tool_id,
                input: request.inputs[0].clone(),
                batch,
                status: JobStatus::Queued,
            };
            touched.push(job.clone());
            state.jobs.insert(new, Entry { job, cancel: CancelToken::new(), tool, request });
            state.queue.push_back(new);
            Ok(())
        })?;
        self.shared.ready.notify_one();
        Ok(self.list())
    }

    /// Removes the finished jobs from the list. Queued and running ones stay. Returns the
    /// full list.
    pub fn clear_finished(&self) -> Vec<Job> {
        lock(&self.shared).jobs.retain(|_, e| !e.job.status.is_finished());
        self.list()
    }
}

impl Drop for JobManager {
    fn drop(&mut self) {
        lock(&self.shared).shutdown = true;
        self.shared.ready.notify_all();
    }
}

/// The worker thread: runs the queued jobs one at a time, until the manager is dropped.
fn worker(shared: Arc<Shared>, sink: Arc<dyn JobSink>) {
    loop {
        let (id, tool, request, cancel) = {
            let mut state = lock(&shared);
            loop {
                if state.shutdown {
                    return;
                }
                if let Some(id) = state.queue.pop_front() {
                    if let Some(entry) = state.jobs.get(&id) {
                        break (id, entry.tool.clone(), entry.request.clone(), entry.cancel.clone());
                    }
                    // Unreachable: `retry` only removes entries that are not queued.
                    debug_assert!(false, "queued job {id} has no entry");
                    continue;
                }
                state = shared.ready.wait(state).unwrap_or_else(|e| e.into_inner());
            }
        };
        run_one(&shared, &sink, id, tool.as_ref(), &request, &cancel);
    }
}

fn run_one(
    shared: &Arc<Shared>,
    sink: &Arc<dyn JobSink>,
    id: u64,
    tool: &dyn Tool,
    request: &ConversionRequest,
    cancel: &CancelToken,
) {
    if cancel.is_cancelled() {
        set_status(shared, sink.as_ref(), id, JobStatus::Cancelled);
        return;
    }
    set_status(shared, sink.as_ref(), id, JobStatus::Running { progress: 0.0 });

    let progress = JobProgress { shared: shared.clone(), sink: sink.clone(), id };
    let result = catch_unwind(AssertUnwindSafe(|| tool.run(request, &progress, cancel)));
    let status = match result {
        Ok(Ok(outcome)) => JobStatus::Succeeded { outcome },
        Ok(Err(ConversionError::Cancelled)) => JobStatus::Cancelled,
        Ok(Err(error)) => JobStatus::Failed { error },
        Err(_) => JobStatus::Failed {
            error: ConversionError::Internal { message: "the tool panicked".to_string() },
        },
    };
    set_status(shared, sink.as_ref(), id, status);
}

#[cfg(test)]
mod tests {
    use super::*;
    use outmark_core::ConversionOutcome;
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

    fn by_id<'a>(jobs: &'a [Job], id: &str) -> &'a Job {
        jobs.iter().find(|j| j.id == id).expect("job is listed")
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
    fn a_second_batch_waits_for_the_first_one() {
        let (m, _) = manager();
        let first = m.start(request(&["block"])).unwrap();
        wait_until(&m, |jobs| matches!(jobs[0].status, JobStatus::Running { .. }));
        let second = m.start(request(&["x"])).unwrap();
        // the blocking job is still running, so the new one is still waiting
        assert_eq!(by_id(&m.list(), &second[0].id).status, JobStatus::Queued);

        m.cancel(&first[0].id);
        let jobs = wait_until(&m, all_finished);
        assert!(matches!(by_id(&jobs, &second[0].id).status, JobStatus::Succeeded { .. }));
    }

    #[test]
    fn a_queued_job_is_cancelled_at_once() {
        let (m, _) = manager();
        let first = m.start(request(&["block"])).unwrap();
        wait_until(&m, |jobs| matches!(jobs[0].status, JobStatus::Running { .. }));
        let queued = m.start(request(&["x"])).unwrap();
        m.cancel(&queued[0].id);
        assert_eq!(by_id(&m.list(), &queued[0].id).status, JobStatus::Cancelled);
        m.cancel(&first[0].id);
        wait_until(&m, all_finished);
    }

    #[test]
    fn cancel_batch_stops_only_its_own_jobs() {
        let (m, _) = manager();
        let first = m.start(request(&["block", "x"])).unwrap();
        let second = m.start(request(&["ok"])).unwrap();
        assert_ne!(first[0].batch, second[0].batch);

        m.cancel_batch(&first[0].batch);
        let jobs = wait_until(&m, all_finished);
        assert_eq!(by_id(&jobs, &first[0].id).status, JobStatus::Cancelled);
        assert_eq!(by_id(&jobs, &first[1].id).status, JobStatus::Cancelled);
        assert!(matches!(by_id(&jobs, &second[0].id).status, JobStatus::Succeeded { .. }));
    }

    #[test]
    fn retry_replaces_a_failed_job_within_its_batch() {
        let (m, _) = manager();
        let created = m.start(request(&["fail", "ok"])).unwrap();
        wait_until(&m, all_finished);

        let jobs = m.retry(&created[0].id).unwrap();
        assert_eq!(jobs.len(), 2);
        assert!(jobs.iter().all(|j| j.id != created[0].id));
        let again = jobs.iter().find(|j| j.input == "fail").expect("the copy is listed").clone();
        assert_eq!(again.batch, created[0].batch);
        let jobs = wait_until(&m, all_finished);
        assert!(matches!(by_id(&jobs, &again.id).status, JobStatus::Failed { .. }));
    }

    #[test]
    fn only_failed_or_cancelled_jobs_can_be_retried() {
        let (m, _) = manager();
        let created = m.start(request(&["ok"])).unwrap();
        wait_until(&m, all_finished);
        assert!(m.retry(&created[0].id).is_err());
        assert!(m.retry("999").is_err());
    }

    #[test]
    fn clear_finished_keeps_running_and_queued_jobs() {
        let (m, _) = manager();
        m.start(request(&["ok"])).unwrap();
        let blocking = m.start(request(&["block", "x"])).unwrap();
        wait_until(&m, |jobs| {
            jobs.iter().any(|j| j.input == "ok" && j.status.is_finished())
                && jobs.iter().any(|j| matches!(j.status, JobStatus::Running { .. }))
        });
        let jobs = m.clear_finished();
        assert_eq!(jobs.len(), 2);
        assert!(jobs.iter().all(|j| !j.status.is_finished()));

        m.cancel_batch(&blocking[0].batch);
        wait_until(&m, all_finished);
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
