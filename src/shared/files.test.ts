// Run with `npm test` (Node's built-in test runner; no extra dependency).
import assert from "node:assert/strict";
import { test } from "node:test";
import { baseName, extensionOf, mergeFiles, splitAccepted } from "./files.ts";

test("baseName handles both separators", () => {
  assert.equal(baseName("C:\\Users\\ana\\notes.md"), "notes.md");
  assert.equal(baseName("/home/ana/notes.md"), "notes.md");
  assert.equal(baseName("notes.md"), "notes.md");
});

test("extensionOf is lowercase and only looks at the file name", () => {
  assert.equal(extensionOf("C:\\a\\Notes.MD"), "md");
  assert.equal(extensionOf("notes.v2.markdown"), "markdown");
  assert.equal(extensionOf("folder.with.dots/readme"), "");
  assert.equal(extensionOf("noextension"), "");
  assert.equal(extensionOf("trailing."), "");
});

test("extensionOf only reads the name, so a file:// URI or a percent-encoded name still works", () => {
  // The desktop dialogs return plain paths; this documents that nothing breaks if a URI shows up.
  assert.equal(extensionOf("file:///C:/docs/my%20notes.md"), "md");
  assert.equal(extensionOf("file:///C:/docs/readme"), "");
});

test("splitAccepted separates accepted and rejected files, ignoring case", () => {
  const { ok, rejected } = splitAccepted(["a.md", "b.TXT", "c.Markdown", "d"], ["md", "markdown"]);
  assert.deepEqual(ok, ["a.md", "c.Markdown"]);
  assert.deepEqual(rejected, ["b.TXT", "d"]);
});

test("splitAccepted with nothing accepted rejects everything", () => {
  assert.deepEqual(splitAccepted(["a.md"], []), { ok: [], rejected: ["a.md"] });
  assert.deepEqual(splitAccepted([], ["md"]), { ok: [], rejected: [] });
});

test("mergeFiles appends new files in order and skips known ones", () => {
  assert.deepEqual(mergeFiles(["a.md", "b.md"], ["b.md", "c.md", "a.md"]), ["a.md", "b.md", "c.md"]);
  assert.deepEqual(mergeFiles([], []), []);
});

test("mergeFiles skips a repeat inside the same batch", () => {
  assert.deepEqual(mergeFiles([], ["a.md", "a.md"]), ["a.md"]);
});

test("mergeFiles ignores case, like Windows and macOS volumes do", () => {
  assert.deepEqual(mergeFiles(["C:\\Docs\\Notes.md"], ["c:\\docs\\notes.MD"]), ["C:\\Docs\\Notes.md"]);
  assert.deepEqual(mergeFiles([], ["a.md", "A.md"]), ["a.md"]);
});

test("mergeFiles treats composed and decomposed accents as the same file", () => {
  const composed = "/docs/caf\u00e9.md"; // é as one character (NFC)
  const decomposed = "/docs/cafe\u0301.md"; // e + combining accent (NFD)
  assert.notEqual(composed, decomposed);
  assert.deepEqual(mergeFiles([decomposed], [composed]), [decomposed]);
  assert.deepEqual(mergeFiles([composed], [decomposed]), [composed]);
});
