# Build and release

## CI (`.github/workflows/ci.yml`)

On every push and PR, matrix macOS / Windows / Ubuntu 22.04:

1. `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`. The hardware test (`#[ignore]`) does not run in CI.
2. `npm ci`, `npm run build`, `npm test`.
3. `npm run tauri build`: universal on macOS (`--target universal-apple-darwin`), AppImage and deb on Linux.
4. Installers are attached to the run (Actions → run → Artifacts): `stlink-rust-ui-macOS` (dmg, app), `-Windows` (msi, nsis `-setup.exe`), `-Linux` (AppImage, deb).

`GITHUB_TOKEN` permissions: `contents: read`.

## Releasing (`.github/workflows/release.yml`)

1. Bump the version in three places (they must match):
   - `Cargo.toml` → `[workspace.package] version`
   - `src-tauri/tauri.conf.json` → `version`
   - `package.json` → `version` (`npm version X.Y.Z --no-git-tag-version`)
2. Check locally: `node scripts/check-version.mjs vX.Y.Z`.
3. Commit, tag and push the tag:

   ```bash
   git tag vX.Y.Z
   git push origin vX.Y.Z
   ```

   `vX.Y.Z-rc.N` or `vX.Y.Z-beta.N` (the files say `X.Y.Z-rc.N`) is published as a **prerelease**.

What the workflow does:

| Job | What |
|---|---|
| `version` | Checks the tag against the three files (`scripts/check-version.mjs`) and fails before any build if they differ; works out prerelease and the numeric MSI version (`X.Y.Z.N`, WiX accepts numbers only) |
| `app` (macOS, Windows, Linux) | `npm run tauri build`: macOS universal dmg, Windows msi and nsis `-setup.exe`, Linux AppImage and deb |
| `cli` (macOS, Windows, Linux) | `stlink-cli` release build: `stlink-cli-<version>-macos-universal.tar.gz` (both architectures via `lipo`), `-windows-x86_64.zip`, `-linux-x86_64.tar.gz`; each with the license files and README |
| `publish` | `SHA256SUMS.txt` over all files; creates the GitHub Release "stlink-rust-ui vX.Y.Z" (notes generated from commits and PRs, with a short header linking to the install section and `docs/windows.md`) or reuses it, uploads the files (`--clobber`), then publishes it |
| `ghcr` | The same files as an OCI artifact in GitHub Packages: `ghcr.io/winkelstein/stlink-rust-ui:X.Y.Z`, plus `:latest` for releases that are not prereleases |

The release is created as a draft and published once every file is uploaded, so nobody sees a half-filled release; there is no manual step. Re-running the workflow on the same tag reuses the release, replaces its assets and pushes the GHCR tag again.

`GITHUB_TOKEN` permissions: `contents: read` everywhere, `contents: write` only for `publish`, `packages: write` only for `ghcr`.

### Dry runs

A dry run builds everything and attaches the files (with `SHA256SUMS.txt`) to the workflow run as artifacts; no Release, no package. It writes the tag's version into the version files of its working copy (never committed), so a prerelease version goes through the real build.

- Automatically on a push to `main` that changes the workflow or the version scripts, as `v<current version>-rc.<run number>`.
- By hand: Actions → Release → Run workflow → `tag` (e.g. `v0.1.1-rc.1`), `dry_run` on. With `dry_run` off a manual run publishes like a tag push (the tag must exist).

### Downloading from GitHub Packages

The package is an OCI artifact (not a container image): pull it with [ORAS](https://oras.land):

```bash
oras pull ghcr.io/winkelstein/stlink-rust-ui:X.Y.Z
```

```bash
sha256sum -c SHA256SUMS.txt
```

`:latest` is the newest release that is not a prerelease. The package is linked to the repository (`org.opencontainers.image.source`) and appears under Packages on its page; it inherits the repository's visibility. If it shows up private, change it once in the package settings.

## Signing

Without secrets:
- **macOS**: ad-hoc signature (`bundle.macOS.signingIdentity: "-"` in `src-tauri/tauri.macos.conf.json`). A downloaded app is not reported as "damaged", but Gatekeeper warns about an unidentified developer: open it the first time via right-click → "Open" (or System Settings → Privacy & Security → "Open Anyway").
- **Windows**: unsigned installers, SmartScreen warns on launch (see [windows.md](windows.md)).

Add secrets in Settings → Secrets and variables → Actions → New repository secret. The signing steps in `release.yml` run only when the secrets are set.

### Apple: Developer ID signing and notarization

Requires the Apple Developer Program and a "Developer ID Application" certificate.

| Secret | What |
|---|---|
| `APPLE_CERTIFICATE` | Certificate with its key, `.p12` in base64: `base64 -i cert.p12 \| pbcopy` |
| `APPLE_CERTIFICATE_PASSWORD` | `.p12` password |
| `APPLE_SIGNING_IDENTITY` | Certificate name, e.g. `Developer ID Application: Name (TEAMID)` (`security find-identity -v -p codesigning`) |
| `APPLE_ID` | Apple ID used for notarization |
| `APPLE_PASSWORD` | App-specific password (appleid.apple.com → Sign-In and Security → App-Specific Passwords) |
| `APPLE_TEAM_ID` | Team ID (developer.apple.com → Membership) |

With the first three the app is signed with Developer ID. With all six Tauri also notarizes it.

### Windows: code signing

Requires a code signing certificate (OV/EV) as a `.pfx`.

| Secret | What |
|---|---|
| `WINDOWS_CERTIFICATE` | `.pfx` in base64: `[Convert]::ToBase64String([IO.File]::ReadAllBytes("cert.pfx"))` |
| `WINDOWS_CERTIFICATE_PASSWORD` | `.pfx` password |

The workflow imports the certificate into the runner's store and passes its thumbprint to `tauri build` together with the MSI version (`bundle.windows.certificateThumbprint`, SHA-256, DigiCert timestamp). EV certificates on a hardware token do not work this way: they need a `signCommand` with a cloud signing service. That is a separate task.

## Locally

- macOS universal: `rustup target add aarch64-apple-darwin x86_64-apple-darwin`, then `npm run tauri build -- --target universal-apple-darwin`; output in `target/universal-apple-darwin/release/bundle/{macos,dmg}`.
- Workflow lint: `actionlint .github/workflows/*.yml` (`brew install actionlint`).
