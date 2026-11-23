import { useState } from "react";
import {
  cancelBatch,
  cancelJob,
  clearFinished,
  retryJob,
  revealInFolder,
  toConversionError,
  type ConversionError,
  type Job,
} from "../../shared/ipc";
import { baseName } from "../../shared/files";
import { t } from "../../shared/i18n";
import { groupByBatch, reportText, summarize } from "../../shared/batch";

export function errorMessage(error: ConversionError): string {
  const { code, ...params } = error;
  return t(`error.${code}`, params as Record<string, string>);
}

/** Retry is possible for jobs that ended without a result. */
function canRetry(job: Job): boolean {
  return job.status.state === "failed" || job.status.state === "cancelled";
}

interface Props {
  jobs: Job[];
  /** Replaces the whole list: retry and clear answer with the full list. */
  onReplace: (jobs: Job[]) => void;
}

type Act = (action: Promise<unknown>, replacesList?: boolean) => void;

function JobItem({ job, onAction }: { job: Job; onAction: Act }) {
  const { status } = job;
  const running = status.state === "running";
  const cancellable = status.state === "queued" || running;
  const [revealFailed, setRevealFailed] = useState(false);

  return (
    <li className={`job job-${status.state}`}>
      <div className="job-head">
        <span className="job-name" title={job.input}>
          {baseName(job.input)}
        </span>
        <span className="job-state">{t(`jobs.${status.state}`)}</span>
        {cancellable && (
          <button type="button" className="link" onClick={() => onAction(cancelJob(job.id))}>
            {t("jobs.cancel")}
          </button>
        )}
        {canRetry(job) && (
          <button type="button" className="link" onClick={() => onAction(retryJob(job.id), true)}>
            {t("jobs.retry")}
          </button>
        )}
      </div>

      {running && (
        <progress max={1} value={status.progress} aria-label={t("jobs.running")} />
      )}

      {status.state === "failed" && <p className="job-error">{errorMessage(status.error)}</p>}

      {status.state === "succeeded" && (
        <>
          {status.outcome.outputs.map((path) => (
            <p key={path} className="job-output">
              {t("jobs.output")}: <code>{path}</code>{" "}
              <button
                type="button"
                className="link"
                onClick={() => {
                  setRevealFailed(false);
                  revealInFolder(path).catch(() => setRevealFailed(true));
                }}
              >
                {t("jobs.reveal")}
              </button>
            </p>
          ))}
          {revealFailed && <p className="job-error">{t("output.revealFailed")}</p>}
          {status.outcome.report.warnings.length > 0 && (
            <details className="job-warnings">
              <summary>
                {t("jobs.warnings")} ({status.outcome.report.warnings.length})
              </summary>
              <ul>
                {status.outcome.report.warnings.map((w, i) => (
                  <li key={i}>{t(`warning.${w.code}`, { detail: w.detail, page: w.page ?? "" })}</li>
                ))}
              </ul>
            </details>
          )}
        </>
      )}
    </li>
  );
}

export function JobList({ jobs, onReplace }: Props) {
  const [notice, setNotice] = useState<string | null>(null);
  const batches = [...groupByBatch(jobs)];
  const anyFinished = jobs.some((job) => job.status.state !== "queued" && job.status.state !== "running");

  /** Runs an action on the engine and shows a notice if it fails. `replacesList` means the
   * action answers with the full list of jobs. */
  const act: Act = (action, replacesList = false) => {
    setNotice(null);
    action
      .then((result) => {
        if (replacesList) onReplace(result as Job[]);
      })
      .catch((reason) => setNotice(`${t("jobs.actionFailed")}: ${errorMessage(toConversionError(reason))}`));
  };

  return (
    <section>
      <div className="jobs-head">
        <h2>{t("jobs.title")}</h2>
        {anyFinished && (
          <button
            type="button"
            className="link"
            onClick={() => act(clearFinished(), true)}
          >
            {t("jobs.clearFinished")}
          </button>
        )}
      </div>
      {notice && <p className="notice">{notice}</p>}

      {jobs.length === 0 ? (
        <p className="muted">{t("jobs.empty")}</p>
      ) : (
        [...batches].reverse().map(([batch, group], index) => (
          <Batch
            key={batch}
            number={batches.length - index}
            batch={batch}
            jobs={group}
            onAction={act}
            onNotice={setNotice}
          />
        ))
      )}
    </section>
  );
}

function Batch({
  number,
  batch,
  jobs,
  onAction,
  onNotice,
}: {
  number: number;
  batch: string;
  jobs: Job[];
  onAction: Act;
  onNotice: (message: string | null) => void;
}) {
  const s = summarize(batch, jobs);
  const copy = () => {
    const text = reportText(jobs, t, (e) => errorMessage(e), baseName);
    // inside a promise chain, so a missing clipboard API is caught too
    Promise.resolve()
      .then(() => navigator.clipboard.writeText(text))
      .then(() => onNotice(t("batch.copied")))
      .catch(() => onNotice(t("batch.copyFailed")));
  };

  return (
    <div className="batch">
      <div className="batch-head">
        <strong>{t("batch.title", { n: number })}</strong>
        <span className="muted">
          {s.finished
            ? t("batch.finished")
            : t("batch.progress", { done: s.terminated, total: s.total })}
        </span>
        {!s.finished && (
          <button type="button" className="link" onClick={() => onAction(cancelBatch(batch))}>
            {t("batch.cancel")}
          </button>
        )}
        {s.finished && (
          <button type="button" className="link" onClick={copy}>
            {t("batch.copy")}
          </button>
        )}
      </div>
      <progress max={1} value={s.progress} aria-label={t("batch.title", { n: number })} />
      {s.finished && (
        <p className="muted">
          {t("batch.summary", {
            succeeded: s.succeeded,
            withWarnings: s.withWarnings,
            failed: s.failed,
            cancelled: s.cancelled,
          })}
        </p>
      )}
      <ul className="jobs">
        {jobs.map((job) => (
          <JobItem key={job.id} job={job} onAction={onAction} />
        ))}
      </ul>
    </div>
  );
}
