#!/usr/bin/env node
// Write the version of a tag (v1.2.3, v1.2.3-rc.1) into Cargo.toml
// ([workspace.package]), src-tauri/tauri.conf.json and package.json.
//   node scripts/set-version.mjs v0.1.1-rc.1
// Used by release dry runs so a prerelease build is exercised end to end
// without committing anything; for real releases bump the files by hand
// (docs/release.md).

import { readFileSync, writeFileSync } from "node:fs";

const tag = process.argv[2] ?? "";
const m = /^v(\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?)$/.exec(tag);
if (!m) {
  console.error(`error: tag "${tag}" is not of the form vX.Y.Z[-pre]`);
  process.exit(1);
}
const version = m[1];
const url = (p) => new URL(`../${p}`, import.meta.url);

const cargo = readFileSync(url("Cargo.toml"), "utf8");
const bumped = cargo.replace(/(\[workspace\.package\][^[]*?\nversion\s*=\s*")[^"]+(")/, `$1${version}$2`);
if (bumped === cargo && !cargo.includes(`version = "${version}"`)) {
  console.error("error: no [workspace.package] version in Cargo.toml");
  process.exit(1);
}
writeFileSync(url("Cargo.toml"), bumped);

for (const p of ["src-tauri/tauri.conf.json", "package.json"]) {
  const json = JSON.parse(readFileSync(url(p), "utf8"));
  json.version = version;
  writeFileSync(url(p), JSON.stringify(json, null, 2) + "\n");
}
console.log(`version set to ${version}`);
