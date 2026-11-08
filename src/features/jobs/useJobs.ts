import { useCallback, useEffect, useState } from "react";
import { listJobs, onJobUpdated, type Job } from "../../shared/ipc";

/** Keeps the list of jobs in sync with the engine (initial list + live updates). */
export function useJobs() {
  const [jobs, setJobs] = useState<Job[]>([]);

  const upsert = useCallback((job: Job) => {
    setJobs((current) => {
      const index = current.findIndex((j) => j.id === job.id);
      if (index === -1) return [...current, job];
      const next = current.slice();
      next[index] = job;
      return next;
    });
  }, []);

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

  return { jobs, upsert };
}
