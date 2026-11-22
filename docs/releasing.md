# Releasing windowsRICE

Releases are built by GitHub Actions from a version tag. A person (or Claude Code acting for the maintainer) prepares the version and runs one command; the workflow rebuilds the tagged source on a clean runner, verifies it again, and publishes.

## Pipeline

| Workflow | Trigger | Token permissions | What it does |
|---|---|---|---|
| `CI` (`.github/workflows/ci.yml`) | push to `master`, pull requests, `v*` tags, manual | `contents: read` | `scripts/check.ps1 full` (format, Clippy, workspace tests, release build), Debug build, third-party notices check, `scripts/app-tests.ps1 -Profile GitHubHosted` (all seven application suites; see below), package and installer verification |
| `Release` (`.github/workflows/release.yml`) | push of a `v*` tag | build job: `contents: read`; publish job: `contents: write` | checks out the exact tag, confirms the tag equals the Cargo version, runs every check above, builds the zip, generates and verifies the SHA-256, writes the notes; the publish job verifies the checksum again and creates the release with the GitHub CLI |

Security choices:
- Only GitHub's own actions (`actions/checkout`, `actions/cache`, `actions/upload-artifact`, `actions/download-artifact`) are used, pinned to commit SHAs. The repository allows only GitHub-owned actions and requires SHA pinning.
- The default token is read-only; only the publish job can write, and it runs no project code.
- Pull requests never reach the release workflow (only people with write access can push tags), and no secrets are used. Checkouts do not keep credentials.
- A failed check stops the build job, so nothing is published.

## Verification profiles

`scripts/app-tests.ps1` runs the seven application suites (end-to-end Apply, Design Studio, bar and popup UI Automation, Phase 4/5, module panels, bottom bar, background image) and writes `target/ci/application-check-results.json`. Every check is listed there with its status (`PASS`, `FAIL` or `NOT_RUN`), its reason when not run, the profile and the environment. A GitHub job summary lists the totals and every check that was not run.

| Profile | Where | Rule |
|---|---|---|
| **FullLocal** | The maintainer's Windows machine with its normal interactive desktop and GPU. Always used by `scripts/release.ps1`, `-DryRun` included; it cannot be skipped. | Every check of every suite must pass. A check that did not run is a failure. |
| **GitHubHosted** | GitHub's hosted Windows runner (`CI` and the `Release` workflow). | Every suite runs and every check must pass, except the allowlisted checks below. These are reported `NOT_RUN — UNSUPPORTED HOSTED RUNNER ENVIRONMENT` with their reason when they cannot work there, and never as passed. Any other failure, any other check that did not run, and any suite that aborts fail the workflow. |

### Why the hosted runner differs

GitHub's runner is a 1024×768 session that renders with software (WARP). It has no GPU compositor, no audio device and no user at the keyboard. In it:
- the bar's window does not receive foreground keyboard focus;
- translucent DirectComposition surfaces are captured flat;
- native Save/Open dialogs cannot be driven reliably by UI Automation.

Everything else runs there: unit and renderer tests, the small-screen regression tests, module panels, the bottom bar (with a visible Windows taskbar), image backgrounds, Phase 4/5, the Design Studio's non-dialog checks, the end-to-end Apply checks, and the package and installer checks.

The Apply fade check follows the Windows "Animation effects" setting the application reads (`windows_animations` in `--stats`):
- **On:** Apply must fade.
- **Off:** no fade may run.

It is therefore mandatory in both profiles.

### Hosted-runner allowlist

| Suite | Check | Reason when not run |
|---|---|---|
| `bar-a11y` | hit testing returns the menu; UIA SetFocus (provider view); keyboard: Tab, Left, Home, End; UIA focus-changed events; UIA Name-changed events | Need foreground keyboard focus on an interactive desktop. |
| `bar-a11y` | auto-hide: collapsed to a 2 px strip, work area released | Needs the real mouse pointer away from the top edge (the scripts never move the pointer). |
| `design-e2e` | translucent background shows the backdrop through the real bar; Ocean Blue background on the real bar (#0E2A3F) | Need real DirectComposition translucency; WARP captures do not show it. |
| `studio-verify` | export through the Save dialog; malformed preset rejected with a reason; valid preset imported into the presets folder; Export .ricepack writes a package; Open .ricepack checks the package and its design | Need the native Save/Open dialogs. |
| `panels-verify` | Volume panel | Needs an audio output device. |

All of these are required, and pass, in FullLocal. The list lives in `scripts/app-tests.ps1` (`$HostedAllowlist`).

## Making a pre-release

1. Update the version in `Cargo.toml` (`[workspace.package] version`) and run `cargo build` so `Cargo.lock` follows.
2. Add a `## [<version>] - <date>` section to `CHANGELOG.md` (move the `Unreleased` entries), and update "Known limitations" if needed.
3. Commit on `master` and push; wait for `CI` to pass (`gh run watch`).
4. Run the release command:

   ```
   pwsh -File scripts/release.ps1 -Version 0.1.0-rc.2 -Prerelease
   ```

   It refuses to continue unless the tree is clean, `master` equals `origin/master`, the version matches `Cargo.toml` and has a changelog section, and the tag is new. It then runs, in this order:

   - the full check (format, Clippy, tests, release build);
   - the Debug build;
   - the **complete FullLocal application checks**;
   - the package;
   - the checksum;
   - the installer verification.

   Only then does it create the annotated tag `v0.1.0-rc.2` and push it. There is no option to skip the application checks or to use the hosted profile instead. `-DryRun` runs every check and stops before the tag. Without a configured git identity, pass `-TaggerName` and `-TaggerEmail`.
5. Follow the workflow: `gh run list --workflow release.yml --limit 1`, then `gh run watch <id>`.
6. Check the result: `gh release view v0.1.0-rc.2` must show the zip and the `.sha256`, marked as a pre-release. Download both and compare the checksum.

If the release workflow fails, fix the cause on `master`, then delete the unpublished tag (`git push origin :refs/tags/v0.1.0-rc.2` and `git tag -d v0.1.0-rc.2`) and release again, or use the next `-rc` number. Never move a tag that has a published release.

## Stable releases

A tag without a pre-release suffix (for example `v0.1.0`) is created as a **draft** release. Publish it by hand only after the manual checks in the latest phase report (Narrator, high contrast, DPI changes, multiple monitors, the installer in a real profile) have been performed and recorded. `scripts/release.ps1` refuses `-Prerelease` for a stable version and requires it for a pre-release version.

## Visibility

The repository is **private**. A GitHub Release in a private repository is visible only to people with access to the repository; it is not a public download. Making the repository public is a separate, deliberate decision and is never done by the release tooling.

## Code signing

Builds are not code-signed. Signing requires a code-signing certificate and `signtool`; until one is available, the release notes say "not signed" and users verify the SHA-256.
