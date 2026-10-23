# Releasing L1 Data

`.github/workflows/release.yml` publishes L1 Data to GitHub Releases on two update tracks, Stable and Nightly. Each published release has a Tauri updater feed that installed apps read. This page tells you how to publish on each track and lists the feeds and secrets the workflow uses.

## How a release runs

Every run starts with the `plan` job. `scripts/release-plan.mjs` decides whether to publish, skip, or dry-run, and it computes the track, version, tag, commit, and the previous tag on the same track. Later jobs read those outputs and compute nothing again.

1. A nightly, and a release from a pushed tag, run the full CI (`ci.yml`) on the commit first. A Stable promotion skips CI, because its commit already shipped as a nightly.
2. Three build jobs run in parallel. They build a universal macOS app and DMG, a Linux AppImage and `.deb`, and a Windows NSIS installer. Each job uploads its bundles and their `.sig` files as workflow artifacts. Build jobs never write to the release.
3. The `publish` job creates a draft release with GitHub's generated notes, uploads every bundle, and builds `latest.json`. `scripts/updater-manifest.mjs` refuses to write `latest.json` unless every platform's bundle is on the release, matches the built file byte for byte, and has a signature that verifies against the updater public key and names the release version.
4. The job then publishes the release and writes the feeds.

Releases run one at a time. A run that starts while another runs waits in the queue and is never canceled. To resume a failed release, rerun its failed jobs. The `publish` job continues from the draft or the published release that the run already made.

The checked-in version in `src-tauri/tauri.conf.json` stays where it is. The build stamps the planned version with a `--config` override, so no release commits a version bump. The same override turns on updater artifacts and sets `plugins.updater.pubkey` to `UPDATER_PUBKEY`, because Tauri refuses to sign updater artifacts without it.

## Publish a nightly

You do not need to do anything. The workflow runs every hour, at 17 minutes past. It publishes a nightly when both of these are true:

- main has commits that the last nightly did not ship.
- At least 6 hours have passed since the last nightly.

A nightly version is `X.Y.Z-nightly.YYYYMMDD.N`. `X.Y.Z` is the latest Stable version with its patch number raised by one, or `0.1.1` before the first Stable. `YYYYMMDD` is the UTC date of the run, and `N` is the workflow run number. The tag is `v` plus the version, and the release is a GitHub prerelease.

To publish a nightly now, run the workflow by hand from main:

```sh
gh workflow run release.yml --ref main -f channel=nightly
```

A manual nightly skips the 6-hour and new-commit checks.

## Cut a Stable release

A Stable release ships the commit of the latest nightly, so Stable only ever ships a build that nightly users already ran. By default its version is that nightly's `X.Y.Z`.

```sh
gh workflow run release.yml --ref main -f channel=stable
```

To choose the version, pass it:

```sh
gh workflow run release.yml --ref main -f channel=stable -f version=0.2.0
```

The plan refuses a version that is not newer than the latest Stable. If a Stable already shipped the latest nightly's `X.Y.Z`, wait for the next nightly or pass a higher version.

## Release a hand-picked commit

To ship a commit that is not the latest nightly, such as a fix on a release branch, push a `vX.Y.Z` tag that points at it:

```sh
git tag v0.1.7 <commit>
git push origin v0.1.7
```

The workflow runs CI on that commit, then publishes it as a Stable release. The tag must be plain `X.Y.Z` and newer than the latest Stable. Tags that contain `-nightly.` do not start a release.

## Read the feeds

| Track | Feed URL | Written by |
|---|---|---|
| Stable | `https://github.com/ImNallen/L1Data/releases/latest/download/latest.json` | Every Stable release. GitHub's `latest` release skips prereleases. |
| Nightly | `https://github.com/ImNallen/L1Data/releases/download/nightly/latest.json` | Every Stable and every Nightly release. The feed is an asset on the rolling `nightly` prerelease. |

A Stable release writes both feeds, so nightly users move up to it. Each feed has four platform keys. `darwin-aarch64` and `darwin-x86_64` both point at one universal `.app.tar.gz`. `linux-x86_64` points at the AppImage, and `windows-x86_64` points at the NSIS installer. Every URL is a `releases/download/<tag>/<asset>` URL.

## Test a change to the release workflow

A pull request that changes `release.yml`, `ci.yml`, or a file in `scripts/` runs the workflow as a dry run. The dry run plans a nightly version for the pull request's head, builds all three platforms with real updater signatures, and builds `latest.json` against the URLs the release would have. It publishes nothing. The run summary shows the bundles, the macOS signing state, and the `latest.json`, and the `latest-json` artifact holds the file.

## Keep the signing keys

The workflow reads these repository secrets:

| Secret | Holds |
|---|---|
| `TAURI_SIGNING_PRIVATE_KEY` | The updater private key. |
| `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | The password of that key. |
| `APPLE_CERTIFICATE` | The Developer ID Application certificate and its private key, as a base64 `.p12` file. |
| `APPLE_CERTIFICATE_PASSWORD` | The password of that `.p12` file. |
| `APPLE_ID` | The Apple ID that submits builds for notarization. |
| `APPLE_PASSWORD` | An app-specific password for that Apple ID. |
| `APPLE_TEAM_ID` | The team ID of the Developer ID certificate. |

The updater private key is `~/.tauri/l1-data-updater.key` on the maintainer's machine, and its password is in `~/.tauri/l1-data-updater.key.password`. Keep an offline backup of both files. Every installed copy of L1 Data trusts only the matching public key, `~/.tauri/l1-data-updater.key.pub`. If you lose the private key, no installed app can accept another update, and every user must download and install a new build by hand. The public key is also `UPDATER_PUBKEY` in `release.yml`, and the `publish` job refuses a signature from any other key.

To set the updater secrets again from those files:

```sh
gh secret set TAURI_SIGNING_PRIVATE_KEY < ~/.tauri/l1-data-updater.key
gh secret set TAURI_SIGNING_PRIVATE_KEY_PASSWORD < ~/.tauri/l1-data-updater.key.password
```

A release signs and notarizes the macOS build, and the `plan` job stops a release before it builds anything if an `APPLE_` secret is missing. A dry run builds macOS unsigned when an `APPLE_` secret is missing and says so in the run summary. To set the Apple secrets:

1. In Keychain Access, export the **Developer ID Application** certificate with its private key as `developer-id.p12`, and choose a password.
2. Create an app-specific password at [Apple Account](https://account.apple.com) under **Sign-In and Security**.
3. Set the secrets:

	```sh
	base64 -i developer-id.p12 | gh secret set APPLE_CERTIFICATE
	gh secret set APPLE_CERTIFICATE_PASSWORD
	gh secret set APPLE_ID
	gh secret set APPLE_PASSWORD
	gh secret set APPLE_TEAM_ID
	```

	Each `gh secret set` without input prompts for the value.

Windows builds are unsigned, so Windows SmartScreen warns on the first launch of each installer.
