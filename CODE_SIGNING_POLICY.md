# Code signing policy

Free code signing provided by [SignPath.io](https://about.signpath.io/), certificate by [SignPath Foundation](https://signpath.org/).

This policy covers the Windows files of [Cedro](https://github.com/realfulvio/cedro) published on the [GitHub releases page](https://github.com/realfulvio/cedro/releases): `cedro.exe` and the installers `Cedro-Setup-<version>.exe` and `Cedro-ARM64-Setup-<version>.exe` (x64 and Windows on ARM). Nothing else is signed with this certificate.

## What gets signed

- Only files built by the GitHub Actions workflow [`windows-arm64.yml`](.github/workflows/windows-arm64.yml) from the source code in this repository, on GitHub-hosted runners. No file built on a developer's computer is ever signed.
- Only release builds (a workflow run started on `main` with a release tag). Every signing request is approved by hand by an approver before it is signed.
- `cedro.exe` is signed first, then packed by Inno Setup into the installer, which is signed in a second request.
- Third-party programs that Cedro downloads on first use (yt-dlp, deno, ffmpeg, whisper.cpp transcription engines) are not signed by us: they keep their authors' own files and signatures.

## Team roles

| Role | Members |
|---|---|
| Committers and reviewers | [realfulvio](https://github.com/realfulvio) |
| Approvers | [realfulvio](https://github.com/realfulvio) |

Changes from outside contributors are merged only through pull requests reviewed by a reviewer. All team members use multi-factor authentication on GitHub and on SignPath.

## Privacy

This program will not transfer any information to other networked systems unless specifically requested by the user or the person installing or operating it.

The only network connections Cedro makes are:

- **Update check:** Cedro asks `api.github.com` for the latest release of this repository and, when the user accepts, downloads the new installer from GitHub. It can be turned off in *Settings*.
- **Components on first use:** when the user starts a feature that needs it, Cedro downloads the component (yt-dlp, deno, ffmpeg, transcription engines and models) from its official release page on GitHub or Hugging Face.
- **YouTube and web video:** only the address the user typed or pasted.

Files are always converted on the user's computer and are never uploaded anywhere. Cedro has no telemetry, no account and no advertising.

## Reporting

Report a problem with a signed file, or a suspected misuse of the certificate, by opening an [issue](https://github.com/realfulvio/cedro/issues).
