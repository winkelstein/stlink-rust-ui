#!/usr/bin/env node
// Release guard: the tag (v1.2.3) must match the app version everywhere.
//   node scripts/check-version.mjs v0.1.0
// Exit 1 with a readable message when something differs.

import { readFileSync } from "node:fs";

const tag = process.argv[2] ?? process.env.GITHUB_REF_NAME ?? "";
const m = /^v(\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?)$/.exec(tag);
if (!m) {
  console.error(`error: tag "${tag}" is not of the form vX.Y.Z`);
  process.exit(1);
}
const version = m[1];

const read = (p) => readFileSync(new URL(`../${p}`, import.meta.url), "utf8");
// [workspace.package] version = "..."
const cargo = /\[workspace\.package\][^[]*?\nversion\s*=\s*"([^"]+)"/.exec(read("Cargo.toml"))?.[1];
const found = {
  "src-tauri/tauri.conf.json": JSON.parse(read("src-tauri/tauri.conf.json")).version,
  "Cargo.toml [workspace.package]": cargo,
  "package.json": JSON.parse(read("package.json")).version,
};

const wrong = Object.entries(found).filter(([, v]) => v !== version);
if (wrong.length > 0) {
  console.error(`error: tag ${tag} does not match the app version:`);
  for (const [file, v] of wrong) console.error(`  ${file}: ${v ?? "(not found)"} (expected ${version})`);
  console.error("Bump the version in these files, commit, and tag again.");
  process.exit(1);
}
console.log(`version ${version} matches ${Object.keys(found).join(", ")}`);
