import { cancelJob, revealInFolder, type ConversionError, type Job } from "../../shared/ipc";
import { useState } from "react";
import { baseName } from "../../shared/files";
import { t } from "../../shared/i18n";

export function errorMessage(error: ConversionError): string {
  const { code, ...params } = error;
  return t(`error.${code}`, params as Record<string, string>);
}

function JobItem({ job }: { job: Job }) {
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
          <button type="button" className="link" onClick={() => cancelJob(job.id)}>
            {t("jobs.cancel")}
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
              <button type="button" className="link" onClick={() => {
                  setRevealFailed(false);
                  revealInFolder(path).catch(() => setRevealFailed(true));
                }}>
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

export function JobList({ jobs }: { jobs: Job[] }) {
  return (
    <section>
      <h2>{t("jobs.title")}</h2>
      {jobs.length === 0 ? (
        <p className="muted">{t("jobs.empty")}</p>
      ) : (
        <ul className="jobs">
          {[...jobs].reverse().map((job) => (
            <JobItem key={job.id} job={job} />
          ))}
        </ul>
      )}
    </section>
  );
}
