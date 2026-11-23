// Merging job updates. Pure, so `npm test` covers it.
// Types come from the generated bindings: importing the IPC module would load Tauri.
import type { Job } from "./ipc/bindings/Job";

function isFinished(job: Job): boolean {
  const { state } = job.status;
  return state === "succeeded" || state === "failed" || state === "cancelled";
}

/**
 * The job to keep when `incoming` arrives for a job already known as `existing`.
 *
 * Updates can arrive out of order: the answer of `start_jobs` carries the job as it was when
 * it was created (queued), and a quick job may have finished before that answer arrives. A
 * job never goes back: a queued snapshot does not replace a known job, and an unfinished one
 * does not replace a finished one.
 */
export function mergeJob(existing: Job | undefined, incoming: Job): Job {
  if (!existing) return incoming;
  if (incoming.status.state === "queued") return existing;
  if (isFinished(existing) && !isFinished(incoming)) return existing;
  return incoming;
}

/**
 * The list to show when the engine answers with its full list (after a retry or a clear).
 * Jobs missing from `list` were removed, unless they are unfinished: those were created after
 * the answer was prepared. Jobs in both keep the newest state.
 */
export function mergeList(current: Job[], list: Job[]): Job[] {
  const known = new Map(current.map((job) => [job.id, job]));
  const listed = new Set(list.map((job) => job.id));
  const merged = list.map((job) => mergeJob(known.get(job.id), job));
  const newer = current.filter((job) => !listed.has(job.id) && !isFinished(job));
  return [...merged, ...newer];
}
