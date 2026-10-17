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

4. The workflow first checks the version against the tag (on a mismatch it fails and names the file that differs), then `tauri-apps/tauri-action` builds the app on three OSes and creates a **draft** GitHub Release "stlink-rust-ui vX.Y.Z" with the installers.
5. Review the draft (Releases), add notes and press "Publish release".

`GITHUB_TOKEN` permissions: `contents: write` only for the build job (it creates the release).

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

The workflow imports the certificate into the runner's store and passes its thumbprint to `tauri build` (`bundle.windows.certificateThumbprint`, SHA-256, DigiCert timestamp). EV certificates on a hardware token do not work this way: they need a `signCommand` with a cloud signing service. That is a separate task.

## Locally

- macOS universal: `rustup target add aarch64-apple-darwin x86_64-apple-darwin`, then `npm run tauri build -- --target universal-apple-darwin`; output in `target/universal-apple-darwin/release/bundle/{macos,dmg}`.
- Workflow lint: `actionlint .github/workflows/*.yml` (`brew install actionlint`).
