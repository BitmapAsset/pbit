# Releasing probbit

The standing procedure for a release `X.Y.Z` (tag `vX.Y.Z`). Each step is verified before the next one runs; the
commands assume `gh` is logged in to an account that can push tags to `BitmapAsset/probbit`.

## The landing-page rule

Every release updates the landing page in its release PR: `README.md`'s line "The current release is vX.Y.Z.", every
release tag the README shows (the GitHub Action example and its `version` input), and the "What's new in X.Y.Z" block
(what changed, whether it is opt-in, the measured numbers with their BENCHMARKS.md section, links to the release notes and
the RFC). `probbit-cli/tests/readme_release.rs` fails when the README's current release disagrees with `Cargo.toml`
(equal on a release, lower on a `-dev` build) or when the README shows a tag newer than it.

## Steps

1. **Release PR** (branch `release-X.Y.Z`, squash subject `Release probbit X.Y.Z (#N)`), from a green `main`:
   - `Cargo.toml`'s `version` and the `version` of the internal path dependencies (`probbit-cli`, `probbit-decide`,
     `probbit-ir`, `probbit-core`, `probbit-persona`: the `probbit-nostd` entries too); `cargo update --workspace --offline` for `Cargo.lock` (CI builds `--locked`);
   - `cargo build --release --locked -p probbit-cli`, then `target/release/probbit man > docs/probbit.1` (generated,
     never hand-edited: `probbit-cli/tests/cli_man.rs` compares them);
   - `npm/package.json`'s `version`; the example tag in `install.sh`, `install.ps1`, `action.yml`,
     `.github/workflows/bench.yml`, `scripts/bench/test_install_ps1.ps1` and `scripts/tests/test_install_contract.py`;
     the title of `docs/CHEATSHEET.md`; the strand header example in `docs/persona.md` §5.7;
   - `CHANGELOG.md`: `## X.Y.Z - <date>`, with an upgrade note wherever an exit code or an output changes;
   - the landing page (above);
   - regenerate `llms-full.txt` after all source-page edits: `python3 scripts/generate_llms_full.py`, then
     `python3 scripts/generate_llms_full.py --check`; include the refreshed bundle in the release PR.

   Before merging: `git grep -n "X.Y.Z-dev"` finds only recorded test fixtures; `python3 bench/individuality/p0.py
   check` with the release build gives 0 differences (its corpus is never regenerated); `cargo publish --dry-run
   --workspace --locked` and `npm pack --dry-run` (in `npm/`) package version X.Y.Z; the four required checks pass.
2. **Tag** the release squash commit on `main`: `git tag -a vX.Y.Z <sha> -m "probbit X.Y.Z"`, then
   `git push origin vX.Y.Z` (the tag only). `release.yml` builds a draft release with 15 assets: 6 archives, the macOS app
   (`probbit-Monitor-vX.Y.Z-macos.zip`, docs/monitor-app.md), 7 `.sha256` files and `probbit.1`.
   Signing of the app asset: with the six `APPLE_*` repository secrets set (docs/monitor-app.md, "Signing and
   notarization"), `release.yml` signs the app with the Developer ID and the hardened runtime, notarizes it and staples the
   ticket, and its notes say "notarized"; without them the asset is signed ad-hoc and the notes say so. A notarization that
   is not accepted fails the job and leaves no app asset.
3. **Check the draft**: download the macOS arm64 archive, check it against its `.sha256`, run `./probbit version`: it
   prints `probbit X.Y.Z`.
4. **Publish**: `gh release edit vX.Y.Z --draft=false --title "probbit X.Y.Z" --notes-file <notes>`. Then the installers
   on a clean directory: the `install.sh` one-liner (latest) and `PROBBIT_VERSION=vX.Y.Z` both install `probbit X.Y.Z`.
5. **npm**, after the release is published (`npm/install.js` downloads its assets): `npm pack --dry-run`, then
   `npm publish --access public`; `npm view probbit version` and `dist-tags.latest` give X.Y.Z, and a clean-prefix
   `npm i -g probbit@X.Y.Z` prints `probbit X.Y.Z`. The token stays out of every file: a throwaway `--userconfig`
   whose one line is `//registry.npmjs.org/:_authToken=${NPM_TOKEN}` (npm expands the variable), with `NPM_TOKEN` set
   only in that shell, `npm --userconfig <file> whoami` first, the file deleted after.
   **The OpenClaw plugin** (`plugins/openclaw`, npm `probbit-stance`), when its version changed: after `probbit` is on
   npm, `npm ci && PROBBIT_BIN=$(command -v probbit) npm test` with the released binary, `npm pack --dry-run` (15 files at 0.2.0:
   `dist/src/*.js`, `openclaw.plugin.json`, `personas/assistant.yaml`, `install/openclaw-entries.json`, `README.md`,
   `package.json`; no tests), then
   `npm publish --access public` with the same userconfig; `npm view probbit-stance version` gives the new version. Its
   `MIN_PROBBIT` (`src/version.ts`) names the oldest probbit it runs.
6. **crates.io**: `cargo publish --workspace --locked` from the tag, which publishes in dependency order (`probbit-nostd`,
   `probbit-core`, `probbit-ir`, `probbit-decide`, `probbit-persona`, `probbit-cli`; `probbit-wasm` has `publish = false`).
   `probbit-nostd` goes first: the engine crates name it as an optional dependency with a version.
7. **Pages front door**, after publication: on the `gh-pages` branch, update `index.html`'s visible release version
   and JSON-LD `softwareVersion` to X.Y.Z. Copy the release's `llms.txt` and regenerated `llms-full.txt` to that branch;
   update the page dates and sitemap date. Review and publish that branch separately, then verify the served page text
   and JSON-LD both name X.Y.Z.
8. **Back to development**: `main` moves to the next `-dev` version (`Cargo.toml`, the path dependencies, `Cargo.lock`,
   `docs/probbit.1`); the README goes on naming the released version until the next release PR.

## Rollback

- Before the tag: a revert PR of the release commit.
- While the release is a draft: `gh release delete vX.Y.Z --yes`, then delete the tag (`git push origin :refs/tags/vX.Y.Z`).
- After publishing: fix forward with the next patch release; `gh release edit vX.Y.Z --prerelease` takes it off "latest",
  which the installers use by default.
- npm: never unpublish. `npm deprecate probbit@X.Y.Z "<reason>"` and `npm dist-tag add probbit@<previous> latest`.
- crates.io versions are permanent: `cargo yank --version X.Y.Z <crate>` for each crate, then the next patch release.
