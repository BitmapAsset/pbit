# probbit Monitor for macOS

`probbit monitor STRAND --open` serves the monitor page on 127.0.0.1 and opens it in the browser. **probbit Monitor** is the
same page in its own Mac window: an app you open from Finder, the Dock or Launchpad, with the probbit binary inside it. The
page itself is the one described in [persona.md §5.8](persona.md#58-monitor-watch-an-individuals-internal-state); what it looks
like: [probbit-monitor.png](probbit-monitor.png).

## Get it

From the [release page](https://github.com/BitmapAsset/probbit/releases): `probbit-Monitor-<version>-macos.zip` and its
`.sha256`. Check it (`shasum -a 256 -c probbit-Monitor-<version>-macos.zip.sha256`), unzip it, move **probbit Monitor** to
Applications. It runs on macOS 13 or later, Apple silicon and Intel (one universal app). No installer, nothing else to get.

### The first launch

The release notes say which kind of app a release carries.

- **Notarized** (the release notes say "notarized"): the app is signed with the project's Apple Developer ID and the
  hardened runtime, Apple's notary service has checked it, and the ticket is stapled to the app. It opens on double-click,
  like any downloaded Mac app; macOS may ask once whether to open an app downloaded from the internet.
- **Signed ad-hoc** (`codesign --sign -`, not notarized: a release built without the project's signing credentials, or a
  copy built by someone else): macOS refuses the first launch of a downloaded copy ("Apple could not verify ..."). Allow it
  once:
  - macOS 15 and later: open the app once (it is refused), then System Settings → Privacy & Security → **Open Anyway**, and
    confirm.
  - macOS 13 and 14: right-click (or Control-click) the app → **Open** → **Open**.
  - From a terminal, instead, before the first open: `xattr -dr com.apple.quarantine "/Applications/probbit Monitor.app"`.
    If macOS already refused the app at that path, removing the mark there was not enough on macOS 26.5 (the next launch
    stopped before its window and started no engine): after the `xattr` line, move the app to another folder (for example
    from Downloads to Applications) and open it there, or use **Open Anyway** as above.

To see which one you have: `spctl --assess --type execute -v "/Applications/probbit Monitor.app"` prints
`accepted` and `source=Notarized Developer ID` for a notarized app, `rejected` for an ad-hoc one. After the first launch
either kind opens like any app. A copy you build yourself (below) has no quarantine mark and opens directly.

## Use it

- **Open a strand**: double-click a `.strand` file (the app registers the `dev.probbit.strand` type for the `.strand`
  extension), drag it onto the Dock icon, use File → Open… (⌘O), or "Open a strand…" in the first window. The dialog takes
  several files, or a folder.
- **Try the demo**: the first window's second button, or File → Try the Demo: the tutor's synthetic week
  (`probbit monitor --demo week`), no strand needed.
- **Several strands, a folder**: the app passes every file you open, and a folder as `--dir FOLDER`, to its probbit. The
  bundled probbit (0.13.0) serves them all on one page, with a rail on the left to switch between strands and each strand's
  labels ([persona.md §5.8](persona.md#58-monitor-watch-an-individuals-internal-state)). A probbit that watches one strand at a
  time (0.12.1 and earlier, if you build the app around one) refuses that; the app then shows the first strand you chose
  (for a folder, its most recently changed `*.strand` file) and says so under the window title.
- **Integrity** (engine 0.14.0+, a persona with the identity self keys, [persona.md §5.9](persona.md#59-integrity-the-drift-from-the-declared-self-rfc-0014)):
  the instrument panel shows the integrity dial (the drift from the declared self as a share of the certificate's allowance)
  where "room left in the bound" stood, and the Mirror tab adds the per-trait drift table of the event on view.
- The window title is the page's title. When the page's title starts with a glyph, the Dock icon shows that glyph as its badge.
- File → Open Recent lists what you opened; the app reopens the last selection when it starts with nothing else to open.
- View → Reload (⌘R) reloads the page; **Follow Latest** (⌘L) loads the page's root again, the latest event with no event
  held; Actual Size (⌘0) undoes a pinch zoom. Appearance (dark, light) follows the system, as the page does.
- From a terminal: `open -a "probbit Monitor" pip.strand`, or `open -a "probbit Monitor" --args --demo week`.

## What it runs

The app is a small Swift program (AppKit and WebKit, no third-party code, no Xcode project) in
`apps/macos/ProbbitMonitor/`. The bundle:

```text
probbit Monitor.app/Contents/
  MacOS/ProbbitMonitor     the window (universal: arm64, x86_64)
  MacOS/probbit            the engine, the same binary as the release archives (universal in the release zip)
  Resources/AppIcon.icns   made from docs/brand/logo.svg
  Info.plist               dev.probbit.monitor, version = the probbit version
```

On a strand the app runs `Contents/MacOS/probbit monitor <files> --serve --port 0`, reads the `http://127.0.0.1:PORT/` line
the engine prints, and loads that page in its window. Opening something else stops that engine and starts another.

**The engine never outlives the app.** It is started with `posix_spawn` and `POSIX_SPAWN_SETPGROUP` (a process group of its
own) and `POSIX_SPAWN_CLOEXEC_DEFAULT` (it inherits no descriptor of the app beyond stdin, stdout and stderr). Quitting the
app (⌘Q, closing the window, `SIGTERM`, `SIGINT`, `SIGHUP`) sends `SIGTERM` to the group, then `SIGKILL` after one second.
A crash or a `SIGKILL` of the app runs no code, so the group is led by a few lines of `/bin/sh` whose stdin is a pipe only the
app can write to: when the app ends, for any reason, the kernel closes that pipe, the shell's `read` returns, and it stops
the engine. CI checks both cases (`smoke.sh --quit term` and `--quit kill9`).

**Network and data.** The engine listens on 127.0.0.1 only (`probbit monitor` has no option to listen elsewhere). The window
loads only that address; a link on the page to anywhere else opens in your browser. The web view keeps no cookies, cache or
storage on disk. The app sends nothing anywhere, has no telemetry, no updater and no preferences window. It stores, in its
own user defaults (`dev.probbit.monitor`), the last selection, the recent list and the window frame.

## Build it

Needs the Xcode command-line tools (`xcode-select --install`: `swiftc`, `lipo`, `codesign`, `ditto`) and a probbit binary.

```sh
cargo build --release -p probbit-cli
apps/macos/ProbbitMonitor/build.sh                      # bundles target/release/probbit
apps/macos/ProbbitMonitor/build.sh --binary PATH        # or any probbit binary
apps/macos/ProbbitMonitor/smoke.sh "apps/macos/ProbbitMonitor/dist/probbit Monitor.app"   # launch, check, quit
```

`build.sh` compiles the shell for arm64 and x86_64 with `swiftc -O -warnings-as-errors`, joins them with `lipo`, assembles the
bundle, signs it (ad-hoc unless `--identity` is given, see below; `codesign --verify --deep --strict` must pass), and writes
`dist/probbit-Monitor-<version>-macos.zip` and its `.sha256` (`--asset-version` names the zip, `--out` moves the output). The
zip carries no extended attributes, so any unzip tool gives a bundle whose signature verifies. `smoke.sh` launches the app,
checks that its engine answers `GET /` with 200 on 127.0.0.1 and nowhere else, quits it (`--quit term`, `osascript`, or
`kill9`) and checks that no engine process is left. `make-icon.sh` remakes `Resources/AppIcon.icns` from the logo with
headless Chrome; the `.icns` is committed, so a build needs no browser.

CI (`ci.yml`, job `macos-app`) runs `build.sh --identity -` (ad-hoc, with the hardened runtime) around the probbit built from
each commit, then `smoke.sh` twice. On a `v*` tag, `release.yml` (job `macos-app`) joins the two macOS release binaries into a
universal one with `lipo`, runs the same `build.sh` (signed and notarized when the secrets below exist) and `smoke.sh`, and
adds `probbit-Monitor-<tag>-macos.zip` and its `.sha256` to the draft release.

## Signing and notarization (maintainers)

A notarized app needs a Developer ID Application certificate (with its private key) and an App Store Connect API key for
Apple's notary service, both from the Apple Developer account that publishes the app.

`build.sh` options:

- `--identity ID`: sign with that identity (a `Developer ID Application: ...` name or its SHA-1), with the hardened runtime
  (`--options runtime`) and a secure timestamp. The bundled `probbit` is signed first, then the app. No entitlements: the
  page is loaded by WebKit's own processes and the engine is a plain child process, so the hardened runtime needs no
  exception (CI checks this on every commit with `--identity -`, an ad-hoc signature with the hardened runtime).
- `--keychain FILE`: look for the identity only in that keychain.
- `--notarize-profile NAME`: notarize with credentials stored by `xcrun notarytool store-credentials NAME`, or
  `--notarize-key KEY.p8 --notarize-key-id KEY_ID --notarize-issuer ISSUER_ID`: notarize with the API key itself.

With a notarize option, `build.sh` zips the signed app, runs `xcrun notarytool submit ... --wait --timeout 30m`, and on
`Accepted` staples the ticket (`xcrun stapler staple`, then `stapler validate`) and zips the stapled app. It writes the
submission's result and Apple's log beside the zip (`probbit-Monitor-<version>-macos.notarization.json` and
`.notarization-log.json`). Any other result prints the log and fails, and no zip is left under the asset's name.

A local notarized build, once the certificate is in the login keychain and the notary credentials are stored:

```sh
xcrun notarytool store-credentials probbit-notary --key KEY.p8 --key-id KEY_ID --issuer ISSUER_ID   # once
apps/macos/ProbbitMonitor/build.sh --identity "Developer ID Application: NAME (TEAM_ID)" --notarize-profile probbit-notary
spctl --assess --type execute -v "apps/macos/ProbbitMonitor/dist/probbit Monitor.app"   # accepted, source=Notarized Developer ID
```

`release.yml` reads six repository secrets (Settings → Secrets and variables → Actions):

| secret | what it holds |
| --- | --- |
| `APPLE_DEVELOPER_ID_P12` | the Developer ID Application certificate and its private key, a `.p12` file, base64 |
| `APPLE_DEVELOPER_ID_P12_PASSWORD` | the password of that `.p12` |
| `APPLE_TEAM_ID` | the team identifier, to pick the identity in the `.p12` |
| `APPLE_NOTARY_KEY_P8` | the App Store Connect API key, the `.p8` file, base64 |
| `APPLE_NOTARY_KEY_ID` | that key's id |
| `APPLE_NOTARY_ISSUER_ID` | the issuer id of the account's API keys |

With the first three, the job imports the certificate into a keychain it creates (a unique name under the runner's
temporary directory, deleted with the decoded files in a step that always runs) and signs with it; with all six it also
notarizes, staples, and checks the result with `spctl`. The secrets reach only the steps that use them. With any of the first
three missing, the job signs ad-hoc as before and says so in one line of its log, and the release notes say the app is not
notarized. The notarization record is kept as the run's `notarization-record` artifact, not attached to the release.

## Linux and Windows

There is no app for them: `probbit monitor STRAND --open` opens the same page in the browser, `--serve` prints its address.
