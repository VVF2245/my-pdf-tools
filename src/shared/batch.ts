// Batches: the jobs created by one conversion. Pure functions, so `npm test` covers them.
// Types come from the generated bindings: importing the IPC module would load Tauri.
import type { Job } from "./ipc/bindings/Job";
import type { ConversionError } from "./ipc/bindings/ConversionError";

export interface BatchSummary {
  batch: string;
  total: number;
  queued: number;
  running: number;
  succeeded: number;
  /** Succeeded with at least one warning. */
  withWarnings: number;
  failed: number;
  cancelled: number;
  /** Jobs that ended: succeeded, failed or cancelled. */
  terminated: number;
  /** 0..1: finished jobs count 1, the running one its own progress, queued ones 0. */
  progress: number;
  finished: boolean;
}

/** Groups jobs by batch. Batches and the jobs inside them keep the order they arrived in. */
export function groupByBatch(jobs: Job[]): Map<string, Job[]> {
  const groups = new Map<string, Job[]>();
  for (const job of jobs) {
    const group = groups.get(job.batch);
    if (group) group.push(job);
    else groups.set(job.batch, [job]);
  }
  return groups;
}

export function summarize(batch: string, jobs: Job[]): BatchSummary {
  const summary: BatchSummary = {
    batch,
    total: jobs.length,
    queued: 0,
    running: 0,
    succeeded: 0,
    withWarnings: 0,
    failed: 0,
    cancelled: 0,
    terminated: 0,
    progress: 0,
    finished: false,
  };
  let done = 0;
  for (const job of jobs) {
    const { status } = job;
    switch (status.state) {
      case "queued":
        summary.queued += 1;
        break;
      case "running":
        summary.running += 1;
        done += status.progress;
        break;
      case "succeeded":
        summary.succeeded += 1;
        if (status.outcome.report.warnings.length > 0) summary.withWarnings += 1;
        summary.terminated += 1;
        done += 1;
        break;
      case "failed":
        summary.failed += 1;
        summary.terminated += 1;
        done += 1;
        break;
      case "cancelled":
        summary.cancelled += 1;
        summary.terminated += 1;
        done += 1;
        break;
    }
  }
  summary.progress = jobs.length === 0 ? 0 : done / jobs.length;
  summary.finished = summary.queued === 0 && summary.running === 0;
  return summary;
}

/** Plain text report of a batch, one block per file. `t` translates keys, `error` a failure. */
export function reportText(
  jobs: Job[],
  translate: (key: string, params?: Record<string, string | number>) => string,
  error: (e: ConversionError) => string,
  fileName: (path: string) => string,
): string {
  const lines: string[] = [];
  for (const job of jobs) {
    const { status } = job;
    lines.push(`${fileName(job.input)}: ${translate(`jobs.${status.state}`)}`);
    if (status.state === "succeeded") {
      for (const path of status.outcome.outputs) lines.push(`  ${translate("jobs.output")}: ${path}`);
      for (const w of status.outcome.report.warnings) {
        lines.push(`  ${translate(`warning.${w.code}`, { detail: w.detail, page: w.page ?? "" })}`);
      }
    }
    if (status.state === "failed") lines.push(`  ${error(status.error)}`);
  }
  return lines.join("\n");
}
