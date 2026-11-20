# Security policy

## Supported versions

Only the latest pre-release and, once published, the latest stable release receive fixes.

## Reporting a vulnerability

Please report vulnerabilities privately through GitHub's **Report a vulnerability** button on the repository's Security tab (private vulnerability reporting), not in a public issue. Include the version, the steps to reproduce and the impact. You will get an answer as soon as the maintainer can look at it; there is no paid bounty.

## Security model

- windowsRICE runs as a normal per-user application without administrator rights. It does not inject code into other processes, install drivers, or change Winlogon, boot, security or anti-cheat settings.
- It is offline: it makes no network requests and sends no telemetry. Diagnostics are written only when asked for with `--stats <file>`.
- Configuration, themes and presets are declarative data (TOML) and are validated before use. Imported content is never executed.
- Settings are written atomically (temporary file and rename); an older configuration is backed up before migration.
- The installer writes only to the user's profile and refuses unsafe folders; the uninstaller removes only its own files.
- Release builds are produced by the GitHub `Release` workflow from the tagged source, with a SHA-256 checksum. They are **not code-signed** yet; verify the checksum before running a download.

## Scope

Reports about the bar, the Design Studio, the configuration and import formats, the installer and uninstaller, and the release pipeline are in scope. Issues that require an attacker who already controls the user's account or files are generally out of scope.
