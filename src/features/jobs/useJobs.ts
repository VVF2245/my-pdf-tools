import { useCallback, useEffect, useState } from "react";
import { listJobs, onJobUpdated, type Job } from "../../shared/ipc";
import { mergeJob, mergeList } from "../../shared/jobs";

/** Keeps the list of jobs in sync with the engine (initial list + live updates). */
export function useJobs() {
  const [jobs, setJobs] = useState<Job[]>([]);

  const upsert = useCallback((job: Job) => {
    setJobs((current) => {
      const index = current.findIndex((j) => j.id === job.id);
      if (index === -1) return [...current, job];
      const next = current.slice();
      // updates can arrive out of order: see `mergeJob`
      next[index] = mergeJob(current[index], job);
      return next;
    });
  }, []);

  /** Replaces the whole list (after a retry or a clear, the engine sends the full list). */
  const replaceAll = useCallback(
    (list: Job[]) => setJobs((current) => mergeList(current, list)),
    [],
  );

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    let cancelled = false;
    onJobUpdated(upsert).then((fn) => {
      if (cancelled) fn();
      else unlisten = fn;
    });
    listJobs().then((initial) => {
      if (!cancelled) initial.forEach(upsert);
    });
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [upsert]);

  return { jobs, upsert, replaceAll };
}
