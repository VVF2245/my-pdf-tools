// Adds the npm packages that ship in the interface to the licenses page made by cargo-about.
//
//   node scripts/npm-licenses.mjs THIRD-PARTY-LICENSES.html
//
// It reads the production packages from package-lock.json, and fails if one has a license
// that is not accepted or no license text. No dependencies.
import { existsSync, readFileSync, readdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";

const MARKER = "<!-- npm-packages -->";
const ACCEPTED = new Set(["MIT", "Apache-2.0", "BSD-2-Clause", "BSD-3-Clause", "ISC"]);
// This package ships only an SPDX summary; its MIT text is the one of its sibling package,
// from the same authors.
const TEXT_FROM = { "@tauri-apps/plugin-dialog": "node_modules/@tauri-apps/api/LICENSE-MIT" };

const escapeHtml = (text) =>
  text.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;");

/** `A OR B` is fine if one option is accepted; `A AND B` needs both. */
function isAccepted(expression) {
  return expression
    .replace(/[()]/g, " ")
    .split(/\s+OR\s+/)
    .some((option) => option.trim().split(/\s+AND\s+/).every((id) => ACCEPTED.has(id.trim())));
}

function licenseTexts(name, dir) {
  if (TEXT_FROM[name]) return [readFileSync(TEXT_FROM[name], "utf8")];
  const files = readdirSync(dir)
    .filter((file) => /^(licen[sc]e|copying)/i.test(file) && !file.endsWith(".spdx"))
    .sort();
  return files.map((file) => readFileSync(join(dir, file), "utf8"));
}

const page = process.argv[2];
if (!page) throw new Error("usage: node scripts/npm-licenses.mjs <licenses.html>");
const html = readFileSync(page, "utf8");
if (!html.includes(MARKER)) throw new Error(`${page} has no ${MARKER} marker`);

const lock = JSON.parse(readFileSync("package-lock.json", "utf8"));
const sections = [];
for (const [path, entry] of Object.entries(lock.packages)) {
  if (!path || entry.dev) continue;
  // Optional packages for another platform are not installed and do not ship.
  if (entry.optional && !existsSync(path)) continue;
  const name = path.replace(/^.*node_modules\//, "");
  if (!entry.license || !isAccepted(entry.license)) {
    throw new Error(`${name}: license not accepted: ${entry.license}`);
  }
  const texts = licenseTexts(name, path);
  if (texts.length === 0) throw new Error(`${name}: no license text found`);
  sections.push(`  <section class="license">
    <h3>${escapeHtml(name)} ${escapeHtml(entry.version)}</h3>
    <p class="muted">${escapeHtml(entry.license)}</p>
${texts.map((text) => `    <pre>${escapeHtml(text)}</pre>`).join("\n")}
  </section>`);
}

const block = `<h2 style="margin-top:48px">npm packages</h2>\n${sections.join("\n")}`;
writeFileSync(page, html.replace(MARKER, () => block));
console.log(`${sections.length} npm packages added to ${page}`);
