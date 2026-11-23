// Run with `npm test`.
import assert from "node:assert/strict";
import { test } from "node:test";
import type { Job } from "./ipc/bindings/Job.ts";
import { groupByBatch, reportText, summarize } from "./batch.ts";

function job(id: string, batch: string, status: Job["status"], input = `C:\\docs\\f${id}.pdf`): Job {
  return { id, toolId: "pdf_to_md", input, batch, status };
}

const outcome = (warnings: { code: string; detail: string; page: number | null }[] = []) => ({
  outputs: ["C:\\out\\f.md"],
  report: { warnings },
});

test("groupByBatch keeps batches and their jobs in arrival order", () => {
  const groups = groupByBatch([
    job("1", "a", { state: "queued" }),
    job("2", "b", { state: "queued" }),
    job("3", "a", { state: "queued" }),
  ]);
  assert.deepEqual([...groups.keys()], ["a", "b"]);
  assert.deepEqual(groups.get("a")?.map((j) => j.id), ["1", "3"]);
});

test("summarize counts every state and the global progress", () => {
  const jobs = [
    job("1", "a", { state: "succeeded", outcome: outcome([{ code: "emptySlide", detail: "", page: 2 }]) }),
    job("2", "a", { state: "failed", error: { code: "corrupt" } }),
    job("3", "a", { state: "running", progress: 0.5 }),
    job("4", "a", { state: "queued" }),
    job("5", "a", { state: "cancelled" }),
  ];
  const s = summarize("a", jobs);
  assert.equal(s.total, 5);
  assert.equal(s.succeeded, 1);
  assert.equal(s.withWarnings, 1);
  assert.equal(s.failed, 1);
  assert.equal(s.cancelled, 1);
  assert.equal(s.running, 1);
  assert.equal(s.queued, 1);
  assert.equal(s.terminated, 3);
  // done: 1 + 1 + 1 + 0.5 + 0 over 5 jobs
  assert.equal(s.progress, 3.5 / 5);
  assert.equal(s.finished, false);
});

test("a batch is finished when nothing is queued or running", () => {
  const s = summarize("a", [
    job("1", "a", { state: "succeeded", outcome: outcome() }),
    job("2", "a", { state: "cancelled" }),
  ]);
  assert.equal(s.finished, true);
  assert.equal(s.progress, 1);
  assert.equal(summarize("empty", []).progress, 0);
});

test("the report lists each file with its result, warnings and errors", () => {
  const text = reportText(
    [
      job("1", "a", { state: "succeeded", outcome: outcome([{ code: "pageRendered", detail: "", page: 4 }]) }, "C:\\docs\\a.pdf"),
      job("2", "a", { state: "failed", error: { code: "corrupt" } }, "C:\\docs\\b.pdf"),
    ],
    (key) => key,
    (e) => `error:${e.code}`,
    (path) => path.split("\\").pop() ?? path,
  );
  assert.equal(
    text,
    [
      "a.pdf: jobs.succeeded",
      "  jobs.output: C:\\out\\f.md",
      "  warning.pageRendered",
      "b.pdf: jobs.failed",
      "  error:corrupt",
    ].join("\n"),
  );
});
