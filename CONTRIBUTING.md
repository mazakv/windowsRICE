# Contributing to windowsRICE

Thank you for helping. windowsRICE is a native Windows application written in Rust; contributions are accepted under the project's MIT license.

## Before you start

- Open an issue to discuss larger changes first.
- Read `docs/development.md` (verification levels, test conventions) and the architecture decisions in `docs/adr/`.
- Never commit secrets, personal paths, screenshots of your own desktop, or build output. Screenshots in `docs/` are taken only over the pinned test backdrop by the verification scripts.

## Building and testing

Requirements: Windows 10/11 x64, the Rust MSVC toolchain (`rust-toolchain.toml`, minimum Rust 1.100) and the Visual Studio C++ build tools.

```
pwsh -File scripts/check.ps1 quick -Changed     # while working
pwsh -File scripts/check.ps1 full               # before a pull request: fmt, Clippy, tests, release build
pwsh -File scripts/app-tests.ps1                # the running application (needs a desktop session)
```

Automated tests never touch your real configuration: the scripts use isolated configurations under `target\`. Do not change Windows settings, the registry, the Start menu or the Startup folder in tests.

## Pull requests

- Keep each pull request focused, with a clear description of what changed and how it was verified.
- `CI` must pass. Do not weaken or delete tests to make a change pass; fix the cause.
- Mark anything that needs a person or specific hardware (Narrator, DPI changes, multiple monitors) as NOT VERIFIED rather than claiming it works.
- New dependencies need a reason and a license compatible with MIT; run `scripts/third-party-notices.ps1` afterwards.

## Code style

`cargo fmt` and Clippy with `-D warnings`. Every `unsafe` block carries a `// SAFETY:` comment. User-visible text exists in English and Korean.
