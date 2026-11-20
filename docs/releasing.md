# Releasing windowsRICE

Releases are built by GitHub Actions from a version tag. A person (or Claude Code acting for the maintainer) prepares the version and runs one command; the workflow rebuilds the tagged source on a clean runner, verifies it again, and publishes.

## Pipeline

| Workflow | Trigger | Token permissions | What it does |
|---|---|---|---|
| `CI` (`.github/workflows/ci.yml`) | push to `master`, pull requests, `v*` tags, manual | `contents: read` | `scripts/check.ps1 full` (format, Clippy, workspace tests, release build), Debug build, third-party notices check, `scripts/app-tests.ps1` (end-to-end Apply, Design Studio, UI Automation of bar and popup, Phase 4/5 checks), package and installer verification |
| `Release` (`.github/workflows/release.yml`) | push of a `v*` tag | build job: `contents: read`; publish job: `contents: write` | checks out the exact tag, confirms the tag equals the Cargo version, runs every check above, builds the zip, generates and verifies the SHA-256, writes the notes; the publish job verifies the checksum again and creates the release with the GitHub CLI |

Security choices:
- Only GitHub's own actions (`actions/checkout`, `actions/cache`, `actions/upload-artifact`, `actions/download-artifact`) are used, pinned to commit SHAs. The repository allows only GitHub-owned actions and requires SHA pinning.
- The default token is read-only; only the publish job can write, and it runs no project code.
- Pull requests never reach the release workflow (only people with write access can push tags), and no secrets are used. Checkouts do not keep credentials.
- A failed check stops the build job, so nothing is published.

## Making a pre-release

1. Update the version in `Cargo.toml` (`[workspace.package] version`) and run `cargo build` so `Cargo.lock` follows.
2. Add a `## [<version>] - <date>` section to `CHANGELOG.md` (move the `Unreleased` entries), and update "Known limitations" if needed.
3. Commit on `master` and push; wait for `CI` to pass (`gh run watch`).
4. Run the release command:

   ```
   pwsh -File scripts/release.ps1 -Version 0.1.0-rc.2 -Prerelease
   ```

   It refuses to continue unless the tree is clean, `master` equals `origin/master`, the version matches `Cargo.toml` and has a changelog section, and the tag is new. It then runs the full check, the application checks, builds the package, verifies the checksum and the installer, and only then creates the annotated tag `v0.1.0-rc.2` and pushes it. `-DryRun` does everything except the tag. Without a configured git identity, pass `-TaggerName` and `-TaggerEmail`.
5. Follow the workflow: `gh run list --workflow release.yml --limit 1`, then `gh run watch <id>`.
6. Check the result: `gh release view v0.1.0-rc.2` must show the zip and the `.sha256`, marked as a pre-release. Download both and compare the checksum.

If the release workflow fails, fix the cause on `master`, then delete the unpublished tag (`git push origin :refs/tags/v0.1.0-rc.2` and `git tag -d v0.1.0-rc.2`) and release again, or use the next `-rc` number. Never move a tag that has a published release.

## Stable releases

A tag without a pre-release suffix (for example `v0.1.0`) is created as a **draft** release. Publish it by hand only after the manual checks in the latest phase report (Narrator, high contrast, DPI changes, multiple monitors, the installer in a real profile) have been performed and recorded. `scripts/release.ps1` refuses `-Prerelease` for a stable version and requires it for a pre-release version.

## Visibility

The repository is **private**. A GitHub Release in a private repository is visible only to people with access to the repository; it is not a public download. Making the repository public is a separate, deliberate decision and is never done by the release tooling.

## Code signing

Builds are not code-signed. Signing requires a code-signing certificate and `signtool`; until one is available, the release notes say "not signed" and users verify the SHA-256.
