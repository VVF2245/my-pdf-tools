// Run with `npm test`.
import assert from "node:assert/strict";
import { test } from "node:test";
import { effectiveOutputDir, loadChosenDir, saveChosenDir, type KeyValueStore } from "./output.ts";

function memoryStore(): KeyValueStore {
  const map = new Map<string, string>();
  return {
    getItem: (k) => map.get(k) ?? null,
    setItem: (k, v) => void map.set(k, v),
    removeItem: (k) => void map.delete(k),
  };
}

const broken: KeyValueStore = {
  getItem: () => {
    throw new Error("blocked");
  },
  setItem: () => {
    throw new Error("blocked");
  },
  removeItem: () => {
    throw new Error("blocked");
  },
};

test("the chosen folder wins over Downloads, and Downloads over next to the original", () => {
  assert.equal(effectiveOutputDir("D:\\out", "C:\\Downloads"), "D:\\out");
  assert.equal(effectiveOutputDir(null, "C:\\Downloads"), "C:\\Downloads");
  assert.equal(effectiveOutputDir(null, null), null);
});

test("a chosen folder is remembered and can be forgotten", () => {
  const store = memoryStore();
  assert.equal(loadChosenDir(store), null);
  saveChosenDir(store, "D:\\out");
  assert.equal(loadChosenDir(store), "D:\\out");
  saveChosenDir(store, null);
  assert.equal(loadChosenDir(store), null);
});

test("blocked or missing storage never throws", () => {
  assert.equal(loadChosenDir(broken), null);
  assert.doesNotThrow(() => saveChosenDir(broken, "D:\\out"));
  assert.equal(loadChosenDir(undefined), null);
  assert.doesNotThrow(() => saveChosenDir(undefined, "D:\\out"));
});
