// Fails unless a release tag matches the version in every manifest.
//
//   node scripts/check-version.mjs v0.1.0
//   node scripts/check-version.mjs v0.1.0-rc.1    (a rehearsal of version 0.1.0)
import { readFileSync } from "node:fs";

const tag = process.argv[2] ?? "";
const match = /^v(\d+\.\d+\.\d+)(-rc\.\d+)?$/.exec(tag);
if (!match) {
  console.error(`"${tag}" is not a release tag (expected vX.Y.Z or vX.Y.Z-rc.N)`);
  process.exit(1);
}
const version = match[1];

const json = (path) => JSON.parse(readFileSync(path, "utf8"));
const lock = json("package-lock.json");
const cargo = /\[workspace\.package\][^[]*?^version\s*=\s*"([^"]+)"/m.exec(readFileSync("Cargo.toml", "utf8"));
const found = {
  "src-tauri/tauri.conf.json": json("src-tauri/tauri.conf.json").version,
  "package.json": json("package.json").version,
  "package-lock.json": lock.version,
  "package-lock.json (root package)": lock.packages[""].version,
  "Cargo.toml": cargo?.[1],
};

const wrong = Object.entries(found).filter(([, value]) => value !== version);
for (const [file, value] of wrong) console.error(`${file} has version ${value}, the tag says ${version}`);
if (wrong.length > 0) process.exit(1);
console.log(`${tag}: every manifest is at ${version}`);
