// Run with `npm test`.
import assert from "node:assert/strict";
import { test } from "node:test";
import type { Job } from "./ipc/bindings/Job.ts";
import { mergeJob, mergeList } from "./jobs.ts";

function job(status: Job["status"]): Job {
  return { id: "1", toolId: "pdf_to_md", input: "a.pdf", batch: "b", status };
}

const queued = job({ state: "queued" });
const running = job({ state: "running", progress: 0.5 });
const failed = job({ state: "failed", error: { code: "corrupt" } });
const cancelled = job({ state: "cancelled" });

test("an unknown job is taken as it comes", () => {
  assert.equal(mergeJob(undefined, queued), queued);
});

test("normal progress replaces the old state", () => {
  assert.equal(mergeJob(queued, running), running);
  assert.equal(mergeJob(running, failed), failed);
});

test("a late queued snapshot does not undo a job that already ran (a broken PDF fails at once)", () => {
  assert.equal(mergeJob(failed, queued), failed);
  assert.equal(mergeJob(running, queued), running);
});

test("a late progress report does not reopen a finished job", () => {
  assert.equal(mergeJob(failed, running), failed);
  assert.equal(mergeJob(cancelled, running), cancelled);
});

test("a finished job can be replaced by another finished state", () => {
  assert.equal(mergeJob(running, cancelled), cancelled);
});

test("a full list drops removed jobs but keeps newer states and newer jobs", () => {
  const withId = (j: Job, id: string): Job => ({ ...j, id });
  const merged = mergeList(
    [withId(failed, "1"), withId(failed, "2"), withId(running, "3")],
    [withId(queued, "2")],
  );
  // 1 was cleared, 2 keeps the state that arrived first, 3 was created after the answer
  assert.deepEqual(merged.map((j) => [j.id, j.status.state]), [["2", "failed"], ["3", "running"]]);
});
