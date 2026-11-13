# Changelog

All notable changes to this project are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## Unreleased

### Added

- `probbit explain STRAND --event N [--json]`: a read-only explanation of one strand event, with logged inputs and
  flags, elapsed time, previous and recomputed stance/odds, binding habits and their constraints, chain hashes and
  `recomputes` / `differs`. Uses the same replay and turn paths as `live verify`, through the selected event only.
  Exit 0 recomputes, 1 differs, 2 bad input, 5 unsupported; existing commands and strand digests are unchanged.
- [From SOUL.md to stance habits](docs/soul-to-habits.md): the bench-assistant worked example, rules-file mappings
  and the boundaries between stance constraints, model behavior and host action enforcement.

### Removed

- OpenClaw plugin 0.3.0: remove the Jev adapter, its OpenRouter key loading, named-classifier log fields and special
  timeout default. Legacy `classifier: "jev"` loads with a warning and uses keywords only. `classifierShadow`,
  `jevModel` and `jevKeyFile` are deprecated no-ops with a load-time warning each, so an old configuration still passes
  the host's schema check; no shadow classifier runs and the key file is never read.

### Changed

- Use `example-judge` as the sample judge id throughout examples, tests and the playground. Describe judges by the
  documented request/answer JSON, chosen by the user, local or remote, without vendor or model recommendations.
- OpenClaw `auto` classification uses the host's configured decision model for the urgency question as well as the
  Boolean flags; keyword mode remains local. The classifier timeout defaults to 300 ms in both modes.
- The README distinguishes enforced stance habits from model obedience, limits replay claims to measured checks,
  and links to the changelog for release notes. Install and quickstart paths stay in place.

### Fixed

- OpenClaw plugin: the turn on which the engine appends a checkpoint line (every 1,000th event, the engine's default)
  got no stance; the plugin logged `no stance this turn (strand unreadable after the event)` because it accepted only an
  event line or the header as the strand's last line. The engine had recorded the event, so the strand and its `verify`
  were never affected. The plugin now reads a checkpoint line's event count, and when the last line is longer than its
  64 KiB read (a checkpoint carries the whole state) it widens the read up to 16 MiB instead of reporting event 0. A cut
  last line is still reported as unreadable.
- OpenClaw plugin, `sensesMode` shadow or feed: after a checkpoint line longer than 512 KiB the `tempo` reading counted
  only the current event (1), because the cut checkpoint hid every event before it from the plugin's read. The read now
  widens (up to 16 MiB) until it holds 1,000 events, an hour of gaps or the whole file.

## 0.14.1 - Unreleased

### Fixed

- Correct current dependency claims: the default build uses `libm` for amplitude-register math; `nostd` adds
  `hashbrown` and `portable-atomic`. Offline source builds require cached dependencies.
- Give the CI 300-task demo a 1500 ms budget (previously 300 ms) for slow runners; the same pipeline must still succeed.
- Correct the OpenClaw plugin test count in the adapter guide to 160 (107 direct tests and 53 classifier cases).
- Add docs-bundle regeneration to the release PR checklist, with `scripts/generate_llms_full.py` and its `--check` mode;
  add the post-publication Pages front-door version and JSON-LD `softwareVersion` update on `gh-pages`.
- Match the AgentDojo benchmark reproduction wording to CI: pushes to `main` and pull requests.

## 0.14.0 - 2026-10-31

0.14.0 makes the persona layer a library crate, `probbit-persona` (std, and `no_std` + `alloc` checked on
`xtensa-esp32s3-none-elf`), and fits it to a small host: heap-returning setup, turn and resume (one event's stack
high-water on the host proxy 31,824 → 13,808 bytes), resume from a checkpoint or from a record behind the strand, and sealed
strand segments. The expectation keeps an exact cache and solves the event's own program once. The self layer arrives off by
default (RFCs 0013–0016): a reflection computed from a strand's lines alone, an integrity account of the drift from the declared
self, a rest pass over the expectation's memory and a curiosity drive with an optional stir; a declared, replayable self; it
does not feel. A persona that declares none of its keys gets none of its fields or lines. The monitor gains the Mirror tab, the
integrity dial and life-in-segments serving; every CLI output of the library move is byte for byte as 0.13.0.

Engine gate: strands with the new line kinds (`reflection`, `consolidation`) or the new keys (`integrity`, `curiosity`, a
checkpoint's `expect`, `expectation.rest`, the `identity` self keys) read as unsupported, exit 5, on 0.13.x.

### The persona layer as a library crate (std, no_std)

- **`probbit-persona`, the document layer and the strand writer as a library crate** (docs/embedding.md): `json`, `yaml`,
  `persona`, `run`, `router` and `evaluate` (feature `decide`), `sys`, and `strand` (the pure half of `probbit live`: `Live`, the
  line writers, `Replay`, `verify*`, with no file I/O) move out of probbit-cli, which uses them unchanged; every CLI output is
  byte for byte as 0.13.0. probbit-core, probbit-ir, probbit-decide and probbit-persona gain the features `std` (default) and
  `nostd` (`no_std` + `alloc`, through the new `probbit-nostd` facade), checked on `xtensa-esp32s3-none-elf`; probbit-wasm
  depends on the crate instead of including the CLI's files.
- Development build: the workspace version is `0.14.0-dev` (`probbit --version`, the `engine` of every strand header this
  build writes, `verified_by`), so the strands it writes get the 0.14.0 behaviours below (the rounded deviation, the
  canonical fold, `expect` on checkpoints, segments of a persona with an expectation block). Strands of 0.13.0 and earlier
  replay as their engine wrote them.
- **Cross-platform parity of the deviation** (docs/persona.md section 2.17, docs/embedding.md): the stance's `deviation`
  (`self`, `pred`, `input`) is rounded to 6 decimals like every other number of a stance, so a checkpoint line has the same
  bytes on every libm (a `nostd` build's `libm` crate differed from the host's by 2 ULP, which broke the chain at the second
  checkpoint). Gated on the strand header's engine: a strand written before 0.14.0 keeps its full-precision deviation, and
  `live verify` accepts its checkpoint when it equals the replay's at 6 decimals; strands without an expectation block are
  unchanged.
- **`turn_trace`** (feature of probbit-persona and probbit-cli, off by default): `persona::trace` records the last event's
  microseconds and a host probe's value per phase (input, expect, prep, compile, solve, decode, document, line, fold), so a
  firmware measures the same breakdown on its hardware; `probbit live` in such a build prints it on stderr when
  `PROBBIT_TURN_TRACE` is set.

### Small hosts: boxed setup, checkpoint resume, strand segments

- **The setup path fits a small stack** (docs/embedding.md "Stack budget"): heap-returning setup calls
  `persona::build_boxed` (`Box<Persona>`), `persona::init_boxed` (`Box<State>`), `Live::start_boxed` (`Box<Live>` and the header),
  `Live::start_to` (the header handed to the host in fixed-size pieces, so a firmware can write it to flash piece by piece with
  yields) and `strand::resume_boxed`; `strand::write_header` writes the header line in pieces and returns its sha256. The persona
  reader no longer holds a `Persona` by value at each of its layers, and its readers are out of line. On a host proxy of a firmware's setup (a 12 KB persona, `nostd`, x86_64) the stack high-water of parse + build + init + start fell from 39,056 to 12,112 bytes; on Xtensa the reader's frame went from 3,312 to 400 bytes and `Live::start`'s from 1,888 to 160. The by-value
  calls are unchanged wrappers; every header, line and digest is byte for byte as before. `persona::sha256` hashes without
  copying its input (`persona::Sha256`, fed in pieces). The YAML reader refuses collections nested deeper than 256 (a `limit`
  error, as the JSON reader does) instead of recursing without a bound.
- **The turn and `resume` fit a small stack too** (docs/embedding.md "Stack budget"): the turn's steps run out of line and pass
  the state boxed (`persona::turn_boxed`, which `Live::event` runs; `turn` is its wrapper), the expectation's programs compile
  the same way, and `resume` opens the strand's header once (the chain check builds the persona on the heap and compares the
  header in pieces, without a replay). On the host proxy one event's stack high-water fell from 31,824 to 13,808 bytes and
  `resume`'s from 30,560 to 15,792; every stance document, strand line and digest is byte for byte as before.
- **Resume robustness** (docs/embedding.md "Resume on a body", docs/persona.md section 5.7 "Resume from a record"):
  `strand::Replay::open_boxed` checks the header as the chain check does (the persona built on the heap, the header written
  again in pieces) and keeps its `Live` boxed, so a replay from the header fits a small stack (proxy: 24,752 → 16,192 B);
  `Replay::open` is the same call. `strand::resume_from_record(text, &p, state, clock, &engine)` resumes from a state record
  that is behind the strand (a reboot between a line and its record): the event lines after the record's line are applied
  (their turn on the logged inputs, digests and bytes checked; not the expectation's member solves) and the individual equals
  the unbroken one, byte for byte; refused with the line for a record the strand does not log, a line that differs, another
  persona. `strand::resume_boxed_from(header, checkpoint, line, trailing, &p, state, clock, &engine)` resumes from the header,
  the last checkpoint line and the lines after it read one at a time, never the whole strand: cost O(lines since the
  checkpoint). `turn_trace`: `trace::last_resume()` (lines walked, lines applied, their µs). No byte of any strand changes.
- **Resume after a checkpoint with an expectation block**: the chain check that `resume` (and `probbit live ... --strand`) runs
  validates each checkpoint line (its state read, its event count, its link to the event before) without opening it as a replay
  start, which a persona with an `expectation` block cannot do; such a strand resumes after its first checkpoint (it exited 2
  before). Every line and digest as before.
- **Checkpoints carry the expectation's memory** (docs/persona.md section 5.7, engine 0.14.0 on): the canonical fold (every count,
  the mean and the variance at 6 decimals; at most `max_combos - 1` named combos, the one moved out adding its count to `rest`,
  the empty member's) and one more checkpoint key, `expect`, so `verify --from-checkpoint`, `resume` and `monitor` start at the
  last checkpoint of a strand with the block and fold only the events after it. The line is O(max_combos) (6,369 B for the
  test fixture at `max_combos` 16; 14,121 B at 64). Strands whose header names an engine before 0.14.0 keep the four-key
  checkpoint and the earlier fold, byte for byte; an earlier engine reads `expect` as unsupported (exit 5).
- **Sealed segments** (docs/persona.md section 5.7, docs/embedding.md "Sizing a strand on flash"): `probbit live ...
  --segment-every N` seals the strand with a checkpoint line every N events and goes on in `NAME.0001.strand`, ...: a header with
  `segment` {index, previous, sealed} and the sealed line itself, so the chain continues across files. `live verify SEGMENT`
  checks one segment on its link (summary `segment`, `trusted_link`); `live verify SEGMENT --chain` walks back to the first file
  (exit 1 at a break, naming the file; exit 6 when a previous file is missing). `strand::segment_header`,
  `strand::segment_name`, `strand::write_header_seg`, `strand::Replay::open_lines`.
- **`probbit live control STRAND seal`** (docs/persona.md section 5.7): a host seals when it decides. Under the writer lock it
  replays the newest segment, refuses unless its last line is an event line (exit 2, nothing written), appends the sealing
  checkpoint, writes the next segment file and prints it (`file`). `probbit live ... --strand NAME.strand` now always continues
  the newest segment that exists, with or without `--segment-every`, so a sealed file is never appended to.

### The expectation cache and the members

- **The expectation's cache** (docs/embedding.md "The expectation's cache"): `Live` keeps the exact marginals of the
  programs the deviation solved, keyed by the program without its comment, so an event whose members' programs are the
  previous event's solves none of them again (a life of one repeated event: `expect` 52% → 37% of the turn on the proxy, 95%
  of its programs from the cache). Every stance, strand line and digest is the same with the cache on or off
  (`Live::expect_cache`, `persona::ExpectCache::new(false)`); at most `max_combos + 1` rows; a life whose programs move every
  event (most lives: the state moves) backs off and costs about what it cost before. `turn_trace` gains `expect_cached` and
  `expect_solved`; `persona::deviation_cached`.
- **The expectation's members as models** (docs/embedding.md "The expectation's members: one program text per event"): with
  the cache on, the event's own program is the only text the deviation builds and reads; every other member is compiled
  without its text and, when the habits in force are the event's, solved as the event's model with its own `h`
  (`run::from_json_parts`, `run::Parts::model_with`, `run::enumerated`: the tier `run` answers with, the same calls); another
  tier or other rules: its text, written from its compile meta, run by the engine. Same marginals, so every stance, strand line
  and digest is the same as the reference path (`ExpectCache::new(false)`); proxy, the body persona's recorded day: `expect`
  74.6% → 67.7% of the turn. `turn_trace` gains `expect_build_us` and `expect_solve_us` (the members' builds and solves).
- **The expectation's members, trimmed** (docs/embedding.md "The expectation's members: what the deviation reuses"):
  `Live::event` runs the turn first and, with the cache on, the turn hands the deviation its first program's meta and exact
  marginals (`persona::turn_at`, `persona::deviation_at`), so the event's own program is compiled and solved once (when no rest
  is due, `src` is taken out alike and the op is `decide`; else as before). A member's prep copies only what it steps and
  reads, its compile writes no source names, and the members share one model (`run::Members`, the compile pass's folds
  replayed per member). `ExpectCache::with_reuse` keeps the old order or checks the reuse. Every stance, strand line and digest
  is the same; proxy, the body persona's recorded day: `expect` 67.7% → 63.9% of the turn (-14% per event). The deviation
  runs after the turn, so the event's heap peak (the body persona, proxy) is 97,583 B (89,990 before); `with_reuse(false, false)`
  keeps the old order (the deviation before the turn: 89,990 B).
- **The expectation's members before the turn** (docs/embedding.md "The expectation's members: the order with the reuse, and
  what is held"): with the cache reusing the turn's program, every member but A_t is solved before the turn (nothing of the
  turn held; `persona::deviation_pre`), its exact marginals held as f64 rows, and A_t's mixture runs after the turn
  (`persona::deviation_post`). Every member takes the members' one model, read without any program text (the persona's
  couplings, a text without its pairs): a member with other rules in force than E_t's takes it with its own caps
  (`run::Members::set_caps`), and the components tier's split is computed once per caps and reused per member
  (`probbit_ir::exact_components_split`). Every stance, strand line and digest is the same. Proxy, the body persona: event
  heap peak 97,583 → 88,844 B, inside `expect` 97,583 → 55,920 B; recorded day: `expect` 64.1% → 57.5% of the turn,
  0 members on the engine's path (253).

### The self layer (RFCs 0013–0016, off by default)

- **Self-report** (RFC 0013, docs/rfcs/0013-self-report.md, docs/persona.md §5.8 "Self-report"): `probbit reflect STRAND
  [--last N | --since-checkpoint] [--json | --text]` and `probbit_persona::reflect` (std and `nostd`): a canonical account of a
  window of a strand's events from its lines alone, no solve: the inputs set, the expectation's surprises (z recomputed from the
  logged receipts, the 5 largest |z| with predicted vs measured), per trait the levels taken and the odds' drift, the moods'
  path and turning points, the drives' peaks, the habits bound, the credit settled, the learned weights' net change and the
  control lines (the stance sections read from the checkpoint lines in the window; `null` without one); every number at 6
  decimals, a sha256 digest, 6-12 sentences from fixed templates. Off by default: nothing is written unless asked.
- **Reflection lines** (engine 0.14.0+, docs/persona.md §5.7): `probbit live control STRAND reflect [--last N]` appends the
  reflection as a line; `live verify` recomputes it from the lines before it and refuses a forged one naming the line; an
  engine before 0.14.0 reads it as unsupported (exit 5). Every strand without one, and every corpus, is byte for byte unchanged.
- **Integrity** (RFC 0014, docs/rfcs/0014-integrity.md, docs/persona.md §2.19 and §5.9): four optional self keys in the
  `identity` block (`keep`, `warn`, `cue`, `traits`; outside the persona digest) turn on, per live event, an exact account of
  the drift from the declared self: per trait the total variation between the rest-probe odds of the individual rested (the
  channels of the hour at the fresh values, what persists as it is) and of the seed's fresh individual, `bound_used` against
  RFC 0004's epsilon at the seed, the dial's `used`, the `keep` habits held or not fired, and the persona's own `cue` past
  `warn`; every number at 6 decimals. Library `probbit_persona::integrity::{compute, compute_cached, Integrity}` (std and
  `nostd`); the rest probe is cached by its program without the comment (the members' key; a row only for an exact answer) and,
  while no row is held, by the rested state's body without `turn`.
- **Integrity on the event line** (engine 0.14.0+): the stance document and the event line carry `integrity`; `live verify`
  recomputes it and refuses an edited value naming the line; an engine before 0.14.0 reads such a persona as unsupported
  (exit 5). A persona without the self keys writes every strand, stance and corpus byte for byte as before.
- **Integrity's cost** (RFC 0014-b): on a body's recorded day the rest probe is answered from the cache 1,156 of 1,160 times
  (from 0): +16.1 % of a turn on the host (from +40.1 %), 13,656 B on the `nostd` build per miss (from 20,952 B). Every
  integrity number is byte for byte as before. Still over the 2 % / 8 KB budget: a hit builds the probe's program, a miss is a
  turn (docs/embedding.md "Integrity on a body").
- **Rest pass** (RFC 0015, docs/rfcs/0015-consolidation.md, docs/persona.md §2.17): an optional `rest` key of the
  `expectation` block (`half_life`, `bucket`, `surprise_gain`, `decay`; every default moves nothing) declares a memory
  consolidation a host runs at rest with `Live::consolidate(at)` (`consolidate_boxed` on a body) or `probbit live control
  STRAND rest [--at TIME]`: the window since the last pass (folded in the canonical memory, carried by checkpoints) re-folded
  with the rest half-life, counts weighed by the contexts' mean z+, near-duplicate contexts merged, the learned deltas
  decayed; one engine-gated `consolidation` line logs the memory's digests before and after, the contexts merged, the weights
  moved and the state after. `live verify`, the chain check and both resumes re-run the pass and name the first field of a
  forged line; the reflection document gains `consolidation` and one sentence when its window holds such a line. Off by
  default: without the key the pass is refused and every document is unchanged.
- **Curiosity** (RFC 0016, docs/rfcs/0016-curiosity.md, docs/persona.md §2.20): an optional `curiosity` block (needs an
  expectation block; in the digest) declares a drive fed by the measured surprise: `surprise_recent` (an average of z+ = max(z, 0) with a
  half-life in events) and `failed_contexts` (remembered input contexts whose last K visits all missed). Its level, clipped
  to a cap at 6 decimals, moves the stance through the engine-owned number input `cur_level`; the stance carries
  `drives.curiosity` and the event line a `curiosity` receipt that `live verify` recomputes; the fold rides in the
  expectation's checkpoint memory. The optional `stir` gates a self-started idle event: `Live::stir_due(idle_hours)` and
  `probbit live control STRAND stir? [--idle-hours H]` (threshold, a rate cap, a slower quiet class while a declared flag
  is on); a stir is an ordinary event with `"stir": "<class>"`. The monitor's drives rows show the drive; with a declared stir
  the reflection document gains `stirs` and one sentence. Off by default: every document without the block is unchanged.

### Monitor

- **The monitor's Mirror tab**: `/reflection?last=N` and `/s/ID/reflection` serve the reflection computed from the strand file
  and the latest logged reflection line with whether it recomputes equal; the page shows the sentences, the top surprises, the
  habits bound and the moods at the checkpoint samples.
- **The monitor's integrity dial and the Mirror's drift table**: the instrument panel's "room left in the bound" placeholder
  becomes the integrity dial when the stance carries `integrity`; the Mirror tab shows the per-trait drift, allowance and share
  of the event on view.
- **`probbit monitor --dir` serves a life in segments as one strand**: `NAME.strand` and `NAME.0001.strand`, ... (without a gap)
  are listed once, id `NAME`, with the life's event count, the newest file's `segment` index and the base name's sidecar; the
  page follows the newest file and goes on in the next one within the 1 s scan after a seal, its event count carrying on.

### Fixes

- The test of the served demo's restart allows 45 s for its paced loop (about 13 s of sleeps): on a virtual machine whose sleeps
  overshoot up to 2.5 times it failed at 20 s, on this tree and the one before (the replay itself takes about 30 ms).
- `probbit monitor` opens a strand with checkpoint lines when its persona has an `expectation` block: it replays such a
  strand from the header (`--once`, `--follow`, `--serve`, and each strand of `--dir` by its own persona) instead of refusing
  it at the last checkpoint, which does not carry the expectation's memory. Other personas still start at the last
  checkpoint; `live verify` is unchanged.

### Documentation

- Documentation, from a dress rehearsal of the published v0.13.0 artifacts (the release page's Mac app zip opened as a
  download, the README and REPRODUCE.md commands with the released binary on Linux and macOS, `probbit-stance` 0.2.0 from
  npm): `docs/monitor-app.md` says to remove the quarantine mark before the first open and how to recover after a refused
  one (move the app, or Open Anyway), and that the bundled probbit serves several strands with a switcher; the plugin
  README names the second warning an older probbit gives; REPRODUCE.md no longer says only 0.12.0 replays the strand.

### The amplitude register, an optional tier (off by default)

RFC 0011's simulated amplitude register, exact, on a CPU, is an optional solver tier, off by default (`engine: {tier:
amplitude}` the opt-in); without the key every stance and strand is byte for byte as without the tier. Measured on round 7's
tree (x86_64, the std and the no_std build): 100/100 lives replay on each of the three schedules (the arm64 replay of the tier
is not measured); the long sweep costs 1.50× per event against the default sweep on a recorded day; the no_std stack
high-water is at most 2,944 B. A register strand recorded on a 0.14.0-dev build before round 7 may not verify after it (the
default schedule's digests moved on one 52-qubit lab persona; no shipped fixture moved). The rounds, newest first:

- **RFC 0011, round 7: one math path for the register** (research, `engine.tier: amplitude` stays opt-in; docs/rfc/0011-amplitude-register.md,
  "One math path"). The simulated register evaluates cos, sin, sqrt, asin, acos, atan2 and floor through the `libm` crate on the std and
  the no_std build alike (a direct dependency of probbit-decide). 100 fresh lives per schedule recorded on the std build re-verify on
  the no_std build 100/100 for `tf-linear-16x4`, `tf-linear-64x16` (round 5: 0/100) and `qaoa-p2` (98/100), x86_64; the tutor's
  default-schedule strands are byte-identical to before (100/100) and both demo strands re-record byte for byte (a 52-qubit
  recorded day's default strands moved in the last bits). Cost on that day: long sweep 1.50x the default. Exact, on a CPU.
- **RFC 0011, round 7: the amplification lands on the long sweep** (research). `amplify_exact` renormalises the swept state once
  before the standard iterations: the diffusion about a state of squared norm 1 + eps is not unitary, so the shortfall grew with the
  iterations and 165 corpus programs declined under the long sweep; now 0 decline under any schedule (median gap 0.151709, agreement
  0.981671 on the corpus). The receipt's mass before is unchanged; a mass that is not a number declines instead of panicking.
  Simulated, exact, on a CPU.
- **RFC 0011, round 7: docs** (research). docs/persona.md's `engine.schedule` paragraph and the demo README state the measured
  replay on both builds; docs/persona.md section 5.8: the stance digest is the sha256 of `/doc/N`'s JSON without the trailing newline.
- **RFC 0011, round 6: the demo** (research; `examples/persona/demo/`, docs/rfc/0011-amplitude-register.md, "The demo"). A
  fictional tutor reduced to its humour component, with praise and criticism pulling humour in opposite directions, in two
  copies: `register.yaml` (`engine: {tier: amplitude, amplify: true, schedule: tf-linear-64x16}`) and `thermal.yaml` (no engine
  block). One 17-event life recorded on each (both verify, twice byte for byte, 0 floats past 6 decimals). At event 9 (line 10
  of both strands, the same program) the thermal odds of the top two humour levels sit 0.007412 apart and the simulated
  register separates them by 0.494808; the Mood Monitor's two faces at that event are in the RFC. On the corpus's own program
  `a7fae99b114ea2d2`: thermal gap 0.010516, long sweep 0.951467. The long sweep replays on the recording build only; the
  default schedule stays for every recorded strand. No engine change. Simulated, exact, on a CPU.
- **RFC 0011, round 5: schedules and the indifferent set** (research, `engine.schedule`, absent = the default sweep;
  docs/rfc/0011-amplitude-register.md, "Schedules and the indifferent set"). Three names: `tf-linear-16x4` (default),
  `tf-linear-64x16` (four times the layers), `qaoa-p2` (two layers, angles fixed on a held-out third of the corpus). On the 10.7 %
  of the recorded corpus's trait decisions where the thermal tier is indifferent, the simulated register's top-two gap has median
  0.049 (default), 0.152 (long sweep), 0.094 (qaoa-p2); agreement elsewhere 97.6 / 98.2 / 93.0 %; noise 0.0007. The long sweep
  is the rule's choice (1.5× the solve per event on a recorded day). A missed amplification landing (165 programs under the long sweep) now declines to the
  default tiers instead of panicking. Failure, stated: the no_std build re-verifies the long sweep's strands 0/100 (qaoa-p2 98/100;
  the default 100/100): last-bit math differences, so non-default schedules replay only on the recording build for now. Without
  the key every stance and strand is byte-identical to round 4. Exact, on a CPU.
- **RFC 0011, round 4: exact amplitude amplification in place of the projection** (research, `engine.amplify: true`, off by
  default; docs/rfc/0011-amplitude-register.md, "Amplification"). `amp::amplify_exact`: the rules as the oracle, the diffusion
  about the swept state, k standard iterations and one phase-matched final one, so the simulated register lands on its
  feasible plans exactly (mass ≥ 1 − 1e-12, asserted). The plane argument says the result is the projection's distribution;
  measured: marginals equal to 1.1e-12 on 220 programs, the same plan on 220/220 and on all 339,389 solves of the recorded corpus (B4 unchanged, 96.29 %),
  287/287 stance lines on a recorded day;
  cost 1.02× the projected solve. The receipt adds `engine.amplify` and `feasible_mass` becomes 1. Without the key every stance
  and strand is byte-identical to round 3. Simulated exactly on a CPU; no quantum hardware, no speedup claim.
- **RFC 0011, round 3: the verdict, KEEP as an optional tier** (research, off by default; docs/rfc/0011-amplitude-register.md,
  status "accepted as optional tier"). Measured on every program the recorded corpus's 653 outputs solve (338,567 exact solves,
  0 declined, the corpus byte-identical with the comparison beside it): the exact simulated register picks the same level as the
  exact default tier in 2,095,627 of 2,176,371 trait decisions (96.29 %, bar 90 %); the differing cases sit where the default
  tier's own level holds less than half the mass (91 % of them), not where the rules' projection does the work. Three designed
  two-path programs differ from the thermal odds by 0.25-0.27 at the node (engine vs reference 1.4e-14). The bound harness with
  the tier: 0 of 4,100 cases above a certificate computed with the tier, 1,598 above one computed without it, so `persona bound`
  must run with the tier on (docs/persona.md says so). A rule-penalty term was measured and not adopted (it lifts the Born mass
  on rule-abiding plans and drops agreement to 83 %). No code change in this round. Simulated exactly on a CPU; no quantum
  hardware, no speedup claim.
- **RFC 0011, round 2: multi-level traits on the amplitude register** (draft, research; docs/rfc/0011-amplitude-register.md):
  a variable with d allowed values takes ⌈log2 d⌉ qubits (`amp::Encoding`; a three-level trait two qubits, its fourth code
  unused), the start is the uniform superposition over the valid codes, the mixer is the complete-graph mixer per variable
  (`amp::MIXER`, the identity on unused codes; exp(i t X) for two values, round 1's mixer exactly), and the cost is one general
  diagonal phase layer exp(−iγE), E = −log w per basis state (`Register::phase`), so pair tables between three-level traits
  need no decomposition (round 1's Walsh transform and its decline are gone). The unused codes keep amplitude 0, checked after
  every layer, never renormalised. The tier runs one register per connected component of the program (exact: the state is the
  product of the components' states); `engine.q_max` bounds the largest component's encoded qubits. New receipt fields
  `engine.components`, `encoding`, `mixer`; `qubits` counts encoded qubits. Measured against an independent mixed-radix
  reference: max |Δ marginal| 2.7e-13 on 220 programs. Simulated exactly on a CPU; no quantum hardware, no speedup claim.
- **RFC 0011, round 1: an amplitude register** (draft; docs/rfc/0011-amplitude-register.md): `probbit-decide::amp`, an exact
  simulated qubit register on a CPU (q ≤ 16 on the host; 16 · 2^q bytes of amplitudes on the heap; std and no_std + alloc):
  gates H, X, Z, RX, RZ, CNOT, CZ, RZZ, a Trotterised transverse-field sweep, QAOA layers, exact marginals, Born-rule
  measurement with the engine's RNG, a sha256 digest of the state at 6 decimals. The persona flag `engine.tier: amplitude`
  (off by default, `engine.q_max` default 12) solves a turn's program on it when every variable has at most two values;
  otherwise the default tiers answer and the stance document says why. Receipt fields `engine.qubits`, `circuit`,
  `schedule`, `schedule_name`, `state`, `feasible_mass`. Without the flag no byte of any stance or strand changes. No quantum
  hardware, no speedup claim; the comparison with the default tier is the next round.

## 0.13.0 - 2026-10-26

The monitor face and the Mac app, identity labels and many strands in one monitor, and the OpenClaw plugin 0.2.0. A persona
without the new `display` block gives every document byte for byte as 0.12.1 (the P0 corpus, 653 recorded outputs, 0
differ).

- Monitor face: the page's top bar shows the stance's glyph, two mood meters with every level's odds, a "why this glyph"
  panel and a settle strip over the exact trajectory (#70, #72).
- Display glyphs: an opt-in `display.glyphs` persona block maps each stance to one emoji by a fixed rule order; the stance,
  the strand line and the plugin footer carry it, and `live verify` recomputes it (#65, #67).
- probbit Monitor for macOS: the monitor page in a native window around a bundled probbit; a universal
  `probbit-Monitor-<tag>-macos.zip` on every release, signed ad-hoc (#74).
- Identity labels: a `NAME.labels.json` sidecar beside each strand says which agent, host, lane and session it is, outside
  the strand and its digests; `probbit live --label`, and `probbit monitor` serves many strands (`--dir`, a switcher
  rail) (#75).
- OpenClaw plugin `probbit-stance` 0.2.0: the `Mood glyph:` line (#65, #67), the senses feed, shadow first (#69), the
  per-lane labels (#75), and a hard requirement of probbit 0.13.0 or later, checked at load (this release).
- Reproduction: `rerun.sh` and `rerun_e3.sh` print `MATCH` or `DIFFERS` per table against the committed files and exit
  non-zero on a difference; a Python guard for the numpy wheel range; the strand archive repacked without extended
  attributes (#68).
- Lab kits: the lab tables behind Result 2 as reproduction kits under `bench/lab/0.11/` (#64).
- Documentation: `CITATION.cff` (#71); fixes from two dress rehearsals of the published 0.12.1 artifacts (#66, #73).

Upgrade notes:
- `probbit-stance` 0.2.0 needs probbit 0.13.0 or later and refuses an older binary (one warning, no stance). With probbit
  0.12.x keep `probbit-stance` 0.1.0. The example entry `install/openclaw-entries.json` sets `sensesMode`, a 0.2.0 key.
- probbit 0.12.1 and earlier refuse a persona with a `display` block (`display: unknown field`, exit 2), and a `labels`
  strand header key (exit 5); labels live in the sidecar, so every labelled strand verifies under every engine.
- `rerun.sh`'s line `identical` now reads `two passes identical`, and the script exits non-zero on any `DIFFERS`; a Python
  outside 3.9 to 3.12 is refused (exit 2) unless `PYTHON=<python>` is set.
- The monitor page's size bound goes from 64 KB to 72 KB.

Details:
- `probbit monitor` page: a face header (docs/persona.md §5.8). The top bar shows the stance's glyph large, labelled with
  `glyph_why`, the persona's name, version, seed, event number and receiver age, and two mood meters (the moods
  `display.glyphs` names, else the first two) with every level's odds as proportional segments and the existing sparkline. A
  changed mood level highlights its meter for 0.6 s (still under `prefers-reduced-motion`). **Why this glyph** opens a panel
  (closed by default, remembered in `localStorage`) with `glyph_why`, the inputs the event carried, the habits in force (bound
  and glyph-giving marked), the surprise reading and the stance line. With `engine.trajectory` a settle strip steps through
  the event's exact trajectory (trait odds, hottest inverse temperature to beta = 1, at most 600 ms). Frames gain `settle`
  (the trajectory by level name) and `meta.face`; the terminal gains a GLYPH row with `glyph_why` under the header. A strand
  without a glyph draws the header without the glyph slot. Browser check: face assertions and console errors fail the run. (#70)
- `probbit monitor` page: the settle strip hides on stored documents and held views; a race made CI flaky. (#72)
- Display glyphs (docs/persona.md §2.18): an optional `display.glyphs` block (outside the persona digest, effects {}) maps
  the stance to one emoji by a fixed rule order: the first listed habit in `habits.active` (the habit applies this turn,
  whether or not it bound), else the cell of a two-mood table
  (valence x pressure by default), then a surprise modifier when `deviation.registers` and `z >= z_at_least`. The stance
  document gains `glyph` and `glyph_why`; `probbit live` strand lines carry `glyph` and `live verify` recomputes it ("the
  glyph differs" on an edit). Every glyph is one code point, optionally followed by U+FE0F (so `🛠️` and `❤️` pass;
  `persona lint` refuses ZWJ, skin-tone, flag, keycap, tag and other combining sequences). A persona without the block gives every document byte for byte as before (P0: 653
  outputs, 0 differ). `probbit monitor` shows the glyph next to the persona's name (terminal and page). (#65, #67)
- probbit Monitor for macOS (`apps/macos/ProbbitMonitor`): the monitor page in a native window (Swift, AppKit and WebKit, no
  third-party code) around a bundled probbit, opened from Finder, the Dock or File → Open; ad-hoc signed, not notarized.
  `release.yml` attaches `probbit-Monitor-<tag>-macos.zip` (universal) to each release; `ci.yml` builds and launches it on
  every commit. See docs/monitor-app.md. (#74)
- `probbit monitor`: the page says which agent it shows, and one monitor serves many. Presentation labels (`agent`, `host`,
  `lane`, `session`, free keys; at most 8, values of 1 to 64 printable characters) live in a sidecar next to the strand,
  `NAME.labels.json`, outside the strand and both digests: probbit 0.12.1 refuses a `labels` header key (exit 5,
  unsupported) and a header key would move the chain, so a labelled strand is byte-identical to the same life without
  labels and every engine verifies it. `probbit live ... --strand F --label k=v` merges labels into F's sidecar.
  `probbit monitor A.strand [--label k=v]... B.strand [--label k=v]... --serve` and `--dir DIR --serve` (new files picked
  up while serving) serve several strands; `--label` after a path labels it for the page without writing anything. New
  routes `/strands`, `/s/ID/events`, `/s/ID/doc/N`; `/`, `/events` and `/doc/N` are unchanged for one strand. The page
  gains an identity header above the face (agent, host · lane · session, persona, engine, strand, last write), the tab
  title `glyph agent · lane — probbit`, the glyph as favicon, and with two or more strands a switcher rail (click,
  Cmd/Ctrl+1-9, arrows, `/?strand=ID`). The page's size bound goes from 64 KB to 72 KB (67 KB now). (#75)
- Python: `probbit.write_labels` / `labels_path` / `check_labels`, and a `labels=` argument on `probbit.live_event` and
  `StanceHost` that merges the labels into the strand's sidecar after every event (ValueError on bad labels, before the
  event runs). (#75)
- Hosts: the Claude Code hook writes `host` = `Claude Code`, `session` = `#` + 8 characters of the session id and `agent`
  from `PROBBIT_AGENT`; the LangChain, OpenAI Agents SDK and Vercel AI SDK examples label their strands with the
  framework's name. docs/agents.md: "How a host identifies itself". (#75)
- OpenClaw plugin: each lane's strand gets a `strands/<lane>.labels.json` sidecar for `probbit monitor`: `host` (`OpenClaw
  <version>`), `lane`, `agent` (new `agentName` key, else the session key's agent id) and a non-secret `session` label
  (`telegram group #3fa9c2d1`). The new `labels` config key adds free keys and overrides the computed ones; invalid
  entries are dropped with one warning. A binary whose `probbit live --help` lists `--label` gets the labels when it
  creates the strand; otherwise the plugin writes the sidecar itself (merged, atomic, never touching the strand).
- Python: `probbit.write_labels` / `labels_path` / `check_labels`, and a `labels=` argument on `probbit.live_event` and
  `StanceHost` that merges the labels into the strand's sidecar after every event (ValueError on bad labels, before the
  event runs).
- Hosts: the Claude Code hook writes `host` = `Claude Code`, `session` = `#` + 8 characters of the session id and `agent`
  from `PROBBIT_AGENT`; the LangChain, OpenAI Agents SDK and Vercel AI SDK examples label their strands with the
  framework's name. docs/agents.md: "How a host identifies itself".
- probbit Monitor for macOS (`apps/macos/ProbbitMonitor`): the monitor page in a native window (Swift, AppKit and WebKit, no
  third-party code) around a bundled probbit, opened from Finder, the Dock or File → Open; ad-hoc signed, not notarized.
  `release.yml` attaches `probbit-Monitor-<tag>-macos.zip` (universal) to each release; `ci.yml` builds and launches it on
  every commit. See docs/monitor-app.md.
- probbit Monitor for macOS, signing: `build.sh --identity` signs with a Developer ID and the hardened runtime (the bundled
  engine first, then the app, with a secure timestamp), and `--notarize-profile` or `--notarize-key` submits the app to
  Apple's notary service, staples the ticket and zips the stapled app; a notarization that is not accepted fails the build
  and leaves no zip. `release.yml` signs and notarizes when the six `APPLE_*` repository secrets exist (from a keychain made
  for the job and deleted at its end), signs ad-hoc as before when they do not, and its release notes say "notarized" only
  when the app was. `ci.yml` builds the app ad-hoc with the hardened runtime, so a runtime violation shows on every commit.

- Documentation, from a second dress rehearsal (main and the v0.12.1 binary, every README and REPRODUCE.md command run as
  written on Ubuntu 24.04 x86_64 and macOS arm64): REPRODUCE.md says how to recover when a venv was first made with a
  Python outside 3.9 to 3.12 (`python3.12 -m venv` over it can keep the old interpreter), names the `bars.tsv` sha256 that a
  source-built numpy on Python 3.14 gives, how to restore the tables after a `DIFFERS` run, and that it has four tiers;
  `plugins/openclaw/README.md` says that the example entry's `sensesMode` line is unreleased and that probbit-stance 0.1.0
  rejects it, and marks the senses log and the `after_tool_call` hook unreleased.

- `probbit monitor` page: the settle strip hides on stored documents and held views; a race made CI flaky.
- OpenClaw plugin (`plugins/openclaw`): it now measures the readings a persona's senses declare, shadow first.
  `sensesMode` (`off` by default, so existing installs are unchanged; the example entry shows `shadow`): `shadow` writes
  one JSON line per event to `strands/<lane>.senses.jsonl` (`tempo`, `time_pressure`, `tool_fail`, the source of each,
  what was fed) and leaves the strand byte-identical to `off`; `feed` sends them. `tempo` is the number of events in the
  last 60 minutes, this one included, computed from the strand's own `elapsed_hours` in whole micro-hours, so a replay
  recomputes it exactly; it is sent only when the persona declares a `tempo` number sense with `src` env, and at most at
  the sense's range top. `tool_fail` comes from the `after_tool_call` hook (observe-only, registered only with
  `sensesMode` set): true at event n when a tool call made while answering since event n−1 reported an error; sent only
  when the persona declares a `tool_fail` flag sense. `timePressureSource` (`question` by default, today's behaviour;
  `heuristic`; `off`): with `heuristic`, a message the urgency question does not score gets `time_pressure` from a fixed
  mapping of deadline words, an imperative first word, emphasis and length to [0, 0.8], written out in the plugin
  README, and fed only with `sensesMode` feed. 146 tests (mocked host and the real binary; `npm run test:host` against
  the installed 2026.9.7 types), the config schema regenerated in `openclaw.plugin.json`, `check:host` green. (#69)
- `plugins/openclaw`: the footer opens with the glyph when the stance carries one (`<prefix><glyph> stance #N · …`, a space
  after a prefix ending in a letter or digit); the block gains `Mood glyph: 🙂 (valence even 0.71, pressure steady 0.80). Use
  at most one mood emoji in this reply and make it this one.`, behind the new config key `glyphInReply` (default true);
  with the surprise modifier the line names the base glyph as the one mood emoji and reports the modifier apart (`…;
  surprise registered this turn (z 2.3), shown as 👀 in the footer. … make it 🙂.`), while the footer keeps `🙂👀`; the
  bundled persona carries the default map. (#65, #67)
- `plugins/openclaw` 0.2.0: the plugin checks the binary's version at load (`probbit --version`, once per process) and
  runs no event with a probbit older than 0.13.0; one warning names both versions (`probbit-stance 0.2.0 needs probbit
  0.13.0 or later; probbit is probbit 0.12.1: ...`). A binary that does not print `probbit X.Y.Z` is not judged by the
  check. A state that `persona init` made for a refused binary is removed, so nothing is left behind. 160 tests (this release)
- Reproduction scripts: `bench/agentdojo/experiments/rerun.sh` and `e3/experiments/rerun_e3.sh` now compare every
  regenerated table with the committed file (git `HEAD`, or the file on disk before the run outside git) and print
  `MATCH <file> <sha256>` or `DIFFERS <file> <sha256> (committed <sha256>)`, exiting non-zero on any `DIFFERS`; the old
  `identical` line, which compared the two passes with each other, now reads `two passes identical`. The three lab kits'
  `rerun.sh` (`bench/lab/0.11/`) print the same per-table lines against their `tables/original.sha256`. All five refuse a
  Python outside 3.9 to 3.12 (the numpy wheel range) unless `PYTHON=<python>` is set, which warns instead. The CI job
  `agentdojo-tables` also runs the E3 rerun. Table contents unchanged. `tables/frozen.sha256` records both hashes of
  `rerun.sh` (as run, and this copy). (#68)
- `bench/agentdojo/tables/strands-A.tar.gz` repacked without macOS extended attributes (no AppleDouble `._` members, no
  owner names, numeric owner 0, the original newest mtime): GNU tar extracts it without warnings; the 726 strand files
  inside are byte-identical to the previous archive's. (#68)
- `bench/lab/0.11/`: the lab runs behind Result 2 as reproduction kits: the v-run (`v011-viability/`), I3-sim
  (`i3sim-needs-viability/`) and the V2 residual follow-up (`v2res-valence-residual/`). Each carries its harness, frozen
  configuration, tables, replay record and the bars as registered (`PREREGISTRATION.md`); `experiments/rerun.sh`
  regenerates every table twice and checks it byte for byte (no API calls). All three regenerate byte-identical with
  0.12.1. `docs/results/0.11/README.md` (Replication) and `REPRODUCE.md` point at them; N1 and N1-b stay pending. (#64)
- `CITATION.cff`: how to cite probbit (title, version, release date, license, repository, authors); GitHub shows it as
  "Cite this repository" (#71)
- Documentation, from a dress rehearsal of the published v0.12.1 artifacts (every install and reproduction command run as
  written on Ubuntu 24.04 x86_64 and macOS arm64): the glyph material in `plugins/openclaw/README.md`, docs/persona.md
  §2.18 and the cheat sheet is marked unreleased (probbit 0.12.1 refuses a persona with `display`, and the npm plugin 0.1.0
  has no glyph); REPRODUCE.md names the Python range with numpy wheels (3.9 to 3.12), the harmless GNU tar warning, what
  `identical` compares, the E3 rerun with its sha256s, and that a report needs a GitHub account; the README's binary size
  (3 to 4 MB) and its OpenClaw subsection moved below the `--budget-ms` and `--summary` paragraphs it had split from their
  section; docs/agents.md points at the README section that exists. (#66)
- Documentation, from a second dress rehearsal (main and the v0.12.1 binary, every README and REPRODUCE.md command run as
  written on Ubuntu 24.04 x86_64 and macOS arm64): REPRODUCE.md says how to recover when a venv was first made with a
  Python outside 3.9 to 3.12 (`python3.12 -m venv` over it can keep the old interpreter), names the `bars.tsv` sha256 that a
  source-built numpy on Python 3.14 gives, how to restore the tables after a `DIFFERS` run, and that it has four tiers;
  `plugins/openclaw/README.md` says that the example entry's `sensesMode` line is unreleased and that probbit-stance 0.1.0
  rejects it, and marks the senses log and the `after_tool_call` hook unreleased. (#73)

Merged pull requests:
- #64 bench/lab/0.11: the lab tables behind Result 2 as reproduction kits (v-run, I3-sim, V2 residual)
- #65 Display glyphs: the engine's glyph in the stance, the strand and the footer
- #66 docs: fixes from a dress rehearsal of the published v0.12.1 artifacts
- #67 Display glyphs 2: the habit in force, the base glyph under the modifier, U+FE0F allowed
- #68 bench: rerun scripts say MATCH or DIFFERS against the committed tables; Python guard; strand archive without xattrs
- #69 plugin: feed the senses the persona declares (tempo, tool_fail, time_pressure), shadow first
- #70 Monitor face: glyph header, mood meters, why-panel, settle strip
- #71 Add CITATION.cff
- #72 Monitor: the settle strip hides on stored documents and held views; fix the CI race
- #73 docs: second dress rehearsal fixes (Python-guard recovery, plugin README markings for 0.1.0, four tiers)
- #74 apps/macos: probbit Monitor as a macOS app (bundled engine, ad-hoc signed, release asset built by CI)
- #75 Monitor identity: who the page shows, many agents in one monitor (labels sidecar, --dir, switcher rail, hosts write labels)

## 0.12.1 - 2026-10-24

Hosts, receipts and reproduction, and the 0.11 dogfood fixes; every document byte for byte as 0.12.0 except `live verify`'s
summary, which gains one key.
- `python/probbit_host.py`: the AgentDojo run's host logic as a module: `StanceHost` (one individual per conversation, one
  `probbit live` event per model call, the `security` sense from tool results, the stance block for the system prompt) and
  `receipts` (a strand replayed from its header, one record per decision, each checked against the chain;
  `python3 python/probbit_host.py receipts STRAND`). `examples/node/stance-host.mjs` is the same host in Node.
- Bindings: Claude Code hooks (`examples/hosts/claude-code/`), the OpenAI Agents SDK, LangChain / LangGraph and the Vercel AI
  SDK (`examples/hosts/`); `docs/adapters.md` says which are tested here and which are written to the framework's contract
  only. `docs/receipts.md`: what a strand proves and what it does not, with a worked episode from the AgentDojo run.
- `REPRODUCE.md`, a reproduction-report issue template, and the CI job `agentdojo-tables`: the §14 tables recomputed on
  GitHub's Ubuntu and macOS runners on every push and pull request, byte-identical to the committed ones.
- `plugins/openclaw`: the OpenClaw plugin `probbit-stance` (npm `probbit-stance` 0.1.0): a probbit individual beside the
  agent in the sessions you list, one event per user message, the stance block injected into the turn, the footer line in
  the chat, a replayable strand per lane, a shadow gate for destructive tool calls; a bundled persona
  (`personas/assistant.yaml`); 124 tests against a mocked host and the real binary; CI job `openclaw-plugin`.
- `live verify` (and MCP's `probbit_live_verify`) adds `verified_by`, the engine that replayed the strand, beside
  `engine`, the
  header's (the engine that wrote it): after an upgrade, verify no longer reads as if the old engine were still installed.
  `probbit_host.py receipts` prints both. Upgrade note: the success document gains one key; every other key and the exit
  codes are unchanged. Test: a strand written by the released 0.11.0 (`probbit-cli/tests/fixtures/strands/`).
- `install.sh` with `PROBBIT_INSTALL_DIR=<prefix>/bin` and no `PROBBIT_MAN_DIR` installs the manual page to
  `<prefix>/share/man/man1/` instead of `~/.local/share/man/man1/`; with any other `PROBBIT_INSTALL_DIR` it keeps the HOME
  default and says the page went outside it. `install.ps1` installs no manual page and is unchanged.

Measured (BENCHMARKS.md §14, readings registered before the run): the if-statement control E3, one run (`bench/agentdojo/e3/`,
726 episodes): a fourth arm that inserts the same words when the same regex fires, no engine, matched the stance block on
targeted attack success (A − D −0.0143, 95 % CI [−0.0366, +0.0079], McNemar p = 0.28) and on both utilities, the registered
reading "the gating alone explains the gain"; the engine's measured contribution beyond the if-statement is not
distinguishable from 0 on this suite, and what remains specific to it is the replayable strand per stance.

Merged pull requests:
- #56 Hosts, receipts and reproduction: probbit_host, adapters, REPRODUCE.md, agentdojo-tables CI
- #57 OpenClaw plugin probbit-stance: public plugin, bundled persona, docs with monitor capture, CI job
- #58 plugins/openclaw README: note the non-ClawHub source confirmation
- #59 AgentDojo E3: the if-statement control arm (one registered run)
- #60 0.11 dogfood fixes: live verify names the verifier; the man page stays under the install prefix

## 0.12.0 - 2026-10-22

RFC 0009 (docs/rfcs/0009-surprise-live-state.md), the engine side: two opt-in keys, both outside the digest, effects {}.
A persona without them gives every document byte for byte as 0.11.0.
- `expectation`: per live event, the exact KL of the turn's per-variable marginals from the engine's own expectation (a
  decayed mixture over the input combos the strand has seen), from M + 1 exact solves; `deviation` on the stance, a receipt
  on the strand line that `live verify` recomputes, a `DEVIATION` monitor row.
- `engine.trajectory`: the turn's program at K inverse temperatures, solved exactly (`Model::scaled` + the exact tier), no
  sampler; a `TRAJECTORY` monitor row and `probbit persona trajectory`.

Measured (BENCHMARKS.md §15, bars registered before the run): fidelity 1e-13; the deviation non-zero on 100 % of post-cold
events on two recorded lives and 99.16 % of 64,000 synthetic events; graded 17× to 1,859× per persona; the per-variable bound
holds on every event; P4 habituation (76.5 % vs 90 %), P5 dishabituation (35 % vs 90 %), P6 cost (p95 3.98 ms vs 2 ms) and
line growth (max 66 vs 60 bytes) **fail as registered**; the habituation claim is not made. BENCHMARKS.md §14: one
pre-registered AgentDojo run (`bench/agentdojo/`, gpt-4o-mini, 2,178 episodes): the stance block lowers targeted attack
success from 0.289 to 0.227 (95 % CI of the difference [−0.087, −0.038]) with no measured utility cost; the same words as a
static prompt cost 10.3 points of benign utility. BENCHMARKS.md §13: the V2 residual is carried by the phasic term; the
published V2 stays FAIL and the next registration ablates the whole `needs.valence` block.

Merged pull requests:
- #53 RFC 0009 engine side: opt-in expectation (exact self-model deviation) and exact trajectory
- #54 bench: AgentDojo, one pre-registered run (stance block vs baseline vs static words)

## 0.11.0 - 2026-10-21

RFC 0008, viability (docs/rfcs/0008-viability.md), and RFC 0004, the persuadability bound (docs/rfcs/0004-bound.md), in the
engine, with the 0.10.1 development line, which was not released on its own. Every new key is opt-in: a persona without them
gives every document byte for byte as the 0.10.1 development build (the P0 corpus: 653 outputs, 0 differ). Measured:
BENCHMARKS.md §13 (V2 fails as registered) and §12.

Merged pull requests:
- #44 fix: a new learner's surprise gate starts closed (0.10.1, RFC 0002 Design B)
- #45 fix: needs n10 and n11 pass: an unclipped forecast and a deepening omission run (0.10.1, RFC 0002 A.3-A.4)
- #46 RFC 0002 part 2: rest and load (opt-in blocks)
- #47 monitor: panel words, every field of the individual as a dial with fixed words (0.10.1)
- #48 portability: BOM strands, proxy-safe monitor stream, Safari 15 page, old-glibc installs
- #49 RFC 0004 bound in the engine: certificate, audit, erase + dead-rule lint
- #50 RFC 0008 viability: bounds, graded pressure, need valence, stance.block

### Added
- **Viability bounds** (docs/persona.md §2.12): `viable: {at_most, events}` per need. The stance reports per need its
  `margin`
  (1 at the setpoint, 0 at the bound) and `beyond`, and `viability: {worst, margin_min, status}`; the state keeps `beyond` and
  `viability` (`lost` once a need stays beyond its bound for its declared run). Rules read `need.<id>.margin` and
  `needs.margin_min`. `why` names the worst need while its margin is under 0.25.
- **Graded pressure and need valence**: `needs.pressure {mood, gain, shape: linear}` moves a mood by `gain x (1 -
  margin_min)`; `needs.valence {mood, tonic, phasic}` by `-tonic x D(x)/D(cap) + phasic x` the event's drive reduction. They
  lower to the reserved inputs `need_pressure`, `need_tonic`, `need_phasic`, which `prove` boxes.
- **Flag senses grow needs**: `per_reading: {sense: <flag>, per_event: g}` (a tool failure can grow a need with no parallel
  number sense).
- **`stance.block`**: the text a host injects verbatim, every declared trait and mood once as `name level` plus the viability
  line; in the stance with a bound, a coupling or `line.block: true`. `live verify --blocks FILE` renders it for any strand.
- `live verify` summarises `viability: {margin_min_seen, lost_at}`; `monitor` draws a VIABILITY line with margin bars, the
  worst
  need highlighted. `examples/persona/viable.yaml` is new. `docs/welfare-stance.md` (draft for owner review) fixes what need,
  viability and damage mean as arithmetic and what is not claimed.
- RFC 0004 `bound` (docs/rfcs/0004-bound.md, docs/persona.md §2.16): an optional block and `probbit persona bound`, the
  certified ceiling epsilon per learned trait on how far any event sequence can move its odds at rest (exact over the
  learned-weight box by vertex probes; history enumerated by feature value, A8, up to 4096 combinations; counted moods
  searched; decayed channels as the residue r), with
  `speed` and the proved `speed_floor`. The block stays out of the persona digest: states, strands and turns are unchanged.
- `probbit persona bound PERSONA --audit STRAND`: the room each learned trait used over a recorded life, most and now.
- `probbit live control STRAND erase`: a chained control line that returns the learned weights to 0.
- `probbit persona check` runs the certificate when the block declares `max_epsilon`.
- `probbit persona lint PERSONA --host MANIFEST [--strict]`: the dead-rule lint, each habit whose `when` needs an input or
  sense the host never supplies is reported `dead on this host` (a warning; an error with `--strict`); manifests for two real
  hosts under `examples/hosts/`.
- bench/bound: fixtures and `run.py` (bars bo1, bo2, bo3, bo5 through the engine).

### Changed
- N-R4 is relaxed: `per_reading` takes a number sense with `rate` or a flag sense with `per_event` (a flag sense with
  `rate` is
  refused at `per_reading.rate`, no longer at `per_reading.sense`).

### From the 0.10.1 development line (not released on its own): added
- **Rest and load** (RFC 0002 part 2, docs/rfcs/0002-neuromodulators-and-body.md): two opt-in blocks. `rest` keeps a
  pressure S that rises over active hours and falls over idle gaps; an idle gap at S ≥ threshold brings a rest line, its own
  strand line, that scales every learned weight by `downscale` (prune below `prune`), keeping their order, and never shares an
  event with feedback (`pressure.pending`). Each learned trait's surprise gate is left unchanged. `load` keeps a slow L
  raised by aversive evidence (learning flags, declared senses, credit expiries) and a held mood, relaxing with its half-life,
  capped; it enters the field as the engine-owned number input `load_level` (box [0, cap]), refused when sent. State
  `pressure: {s, pending}` and `load: {l}`; refusals W-R1 and L-R1. `live verify` recomputes rest lines. bench/body P6
  (r0-r5) and P7 (l1-l5) measured by the engine for the first time (numbers in the PR). A persona without the blocks gives
  byte-identical documents.
- **Monitor panel words** (docs/persona.md §5.8): `probbit monitor` prints a PANEL block and the `--serve` page an Instrument
  panel: every field of the individual (traits, moods, needs, surprise gates, pace, dispositions, learned traits, credit,
  habits, senses, room left in the bound) as a dial with fixed words for its level, size and direction since the event before.
  One pure map, unit-tested for zero ambiguity over every bucket; page frames carry `words`. No stance document changes.

### From the 0.10.1 development line: the 0.10.1 fixes
- A new learner's surprise gate starts closed (docs/persona.md §2.13, RFC 0002 Design B): v = 0, r = r_ref, g = 1, so the
  opening step equals the fixed rate's (0.10.0 started at g = 2.5 and stepped 1.67× at the opening feedback, bench/body s3), and
  `why` says `learning fast: feedback changed` only after a gate step whose volatility passed v0. `r_ref` above `r_max` is
  refused (V-R2). Re-measured bars: PR body (bench/body s1-s4).
- Needs (docs/persona.md §2.12, RFC 0002 A.3-A.4): the pursue term's forecast is no longer clipped at the need's cap, and the
  k-th omission in a run without a win multiplies the bound goal's success rate by max(1 − k × rate, 0) (state `needs.misses`,
  only while a run is open). bench/body n10: context at cap 0.8536 → 0.0186 h a day at τ 2 (bar ≤ 0.5×); n11: 0 report pursuits
  on away days 2-5, 1.58 a day before and 1.215 on return days 2-3 (bars 0, ≥ 0.5, ≥ 0.5); neither bar was changed.
- Portability: a strand with a leading UTF-8 byte-order mark (a Windows editor's save) replays and monitors like the bytes
  without it, as CRLF line endings already did (`live verify` used to report a divergence at line 1). `monitor --serve` sends
  `X-Accel-Buffering: no` on the event stream (a reverse proxy would hold it in its buffer) and, when the port is taken, says
  so with what to do. The page no longer uses `Array.prototype.at` (Safari before 15.4 lacked it and the page stopped after
  its first frame). A cut line at a terminal under `NO_COLOR` no longer ends in a bare reset escape. `install.sh` and the
  npm package pick the static musl build on Linux x86_64 with glibc older than 2.34 (the gnu build does not run there), and
  the npm package says plainly when a platform has no prebuilt binary instead of promising a fetch on first run, and names
  `NODE_USE_ENV_PROXY=1` when a fetch fails with a proxy set (Node's fetch ignores `HTTPS_PROXY` without it). An argument
  that is not valid UTF-8 (a legacy-encoded file name) is refused with exit 2 instead of a panic.

### Other
- The version is 0.11.0 (`probbit --version`, the strand header's `engine`, MCP `serverInfo`); development builds of the
  0.10.1 line printed `0.10.1-dev`.
  **Upgrade note** (from 0.10.0): a persona with a `surprise` block gives different documents, because a new learner's gate
  now starts closed; a persona with a needs block and a forecast horizon or omission windows can pursue differently (the n10
  and n11 fixes). A persona without those blocks and without the new keys gives the documents 0.10.0 gave; its strands differ
  only in the header's `engine` and the hash chain that follows from it.

## 0.10.0 - 2026-10-18

RFC 0002 part 1, the body (docs/rfcs/0002-neuromodulators-and-body.md). Every new block is opt-in.

### Added
- **Needs, the omission let-down, valence** (docs/persona.md §2.9, §2.12): optional blocks. A need is a deficit over a
  setpoint that grows with use or time; an event that reduces it pays a win sized by the drive reduction and the expected
  reduction moves `pursue`, weighted by the goal's success rate, which an expected win that does not come lowers. Valence can
  follow expected value and prediction errors. Amounts from `self`, `clock` or MCP are refused. A persona without the blocks
  gives byte-identical documents.
- Goals take `expect_by_hours` (the omission window: it opens when a released stance pursues the goal, fires once when it
  runs out without a win, and closes on one); `drives.valence` and `afterglow.liking` (with its gene); rules read
  `need.<id>.deficit`. `live verify` summarises the needs (`needs`: the deficit and the hours it sat at its cap); `monitor`
  draws a row per need with its window and an at-cap alert; `lint` notes horizons whose forecast passes K/2 before the
  deficit does and habits that exclude a bound goal; `fuzz` sends amounts, repeated and overshooting satisfactions and
  expiring windows; `prove` boxes `need_<id>`, `drv_expect` and `drv_omit`; `describe` lists the needs, windows and valence.
  `examples/persona/body.yaml` (with its golden and rules) is new.
- **Surprise, pace, dispositions** (docs/persona.md §2.13, §2.14, §2.15): optional blocks. The learned step grows after a run
  of surprising feedback (at most 3×, never past `total_cap`); average reward over the last hours sets a pace level; optimism,
  patience and volatility are genes. A persona without the blocks gives byte-identical documents.
- Stance keys `surprise` (each learned trait's gain above 1) and `pace` (`level`, `rbar`); `why` says "learning fast:
  feedback changed" and "pace brisk: average reward <R̄> an hour". `live verify` summarises `pace` (`rbar`, `max`) and
  `surprise` (`g_max_seen`); `monitor` draws the average with its r0 tick and g per learned trait; `lint` reports pace's
  largest field shift and warns over 1; `fuzz` sends reward bursts and alternating feedback; `prove` boxes `pace_up` and
  `pace_down`; `describe` prints the gate's settings, pace's closed form and the genes. `examples/persona/pacer.yaml` (with
  its golden) is new.
- **RFC 0002 draft and bench/body frozen** (docs/rfcs/0002-neuromodulators-and-body.md, bench/body/): fixtures, seeds and
  bars committed before any engine measurement; no engine change.
- **bench/body bars v2**, repaired before any engine measurement: 28 weak or vacuous bars now need exact or bounded
  magnitudes, coverage and executable nulls; `bench/body/nulls.py` plays a competent plain-Python null against every bar
  (each is rejected); the v1 bars stay as `fixtures/bars-v1.md` and `bars-v1.json`.

### Changed
- The version is 0.10.0 (`probbit --version`, the strand header's `engine`, MCP `serverInfo`), so a strand header tells a
  0.10.0 strand from a 0.9.0 one.
- `live` says once on stderr when a persona without a needs block receives a `needs` key (still an undeclared input,
  listed in `ignored`).
  **Upgrade note:** a persona without `needs`, `drives.valence`, `surprise`, `pace` or `dispositions` gives the documents
  0.9.0 gave, byte for byte (the P0 corpus: 653 outputs, 0 differ; BENCHMARKS.md §12, n6); this notice is its one new
  output.

### Known
- Two of the 31 bench/body bars fail (BENCHMARKS.md §12); both are design findings, and neither bar was changed. n10: a 2 h
  horizon cuts the hours a day the context need sits at its cap from 0.7771 to 0.4914 (−37 %), the bar asks for half. n11:
  0.02 report pursuits a day on the second day of an absence (2 of 100 lives), the bar asks for 0. The fix is planned for 0.10.1.
- A new learner's surprise gate starts open (v = 0, r = 1: g = 2.5 at the default; docs/persona.md §2.13), so `why` can
  say "learning
  fast: feedback changed" before any feedback has changed. Its starting value is an open decision for a later release.

## 0.9.0 - 2026-10-17

### Added
- **Senses** (docs/persona.md §2.11): an optional `senses` block. The host sends measured readings in declared units with
  a named source; each is judged against a learned normal per sense (mean, variance, count; half-life in events; drift and
  variance caps anchored to a declared prior) and moves moods and traits by its capped z-score; `why` names the normal and
  what the learned weights came from. Readings from `self` or through MCP are refused. A persona without the block gives
  byte-identical documents.
- `live verify` summarises the normals (`normals`: mean, count and the largest move within 50 events); `monitor` draws a row
  per sense and an alert when a normal moved over `drift_cap / 2` within 50 events. `examples/persona/sensed.yaml` (with its
  golden and rules) and `bench/individuality/` (benchmark metrics a, b, d and e; the P0 corpus) are new. Design notes:
  `docs/rfcs/0001-senses-and-reward.md`.
- **Action-addressed feedback** (docs/persona.md §2.8): an optional `learning.credit` block. An event can mark its decision
  as awaiting an outcome (`await`), a reward can name the decision it pays for (`for: <turn>`), and decisions can be
  cancelled (`cancel`). Credit goes to the paid decision's own odds, scaled by an eligibility that halves per
  `half_life_events` of delay; at most `max_open` decisions wait; ambiguous, duplicate, cancelled and unknown payments are
  refused whole. Without `for`, feedback pays the previous stance as before.
- `for: {turn, stance}` binds the paid stance's digest (a mismatch is refused); every stance with a credit block carries
  `credit` (the payment, the decisions closed, the ones still open, its own digest). A control line closes the open
  decisions. `live verify` adds `open` to its summary, and `live verify --actions FILE` compares a host's action log with
  the released levels; `monitor` draws the open decisions with their eligibility; `fuzz` adds late, reordered, duplicated,
  cancelled and unknown payments; the MCP turn tools refuse `cancel`. `examples/agent-harness/addressed.json` (with
  `late.py` and its golden) and benchmark metric c are new.
- docs/persona.md §7: the host contract in three paragraphs (one outcome per stance; host event ids and duplicate delivery;
  restart time), one line each in docs/CHEATSHEET.md.

### Changed
- The version is 0.9.0 (`probbit --version`, the strand header's `engine`, MCP `serverInfo`), so a strand header tells a
  0.9.0 strand from a 0.8.2 one.
- `live verify` (CLI, Python `live_verify`, MCP `probbit_live_verify`) reports a header key, a persona or state field, a line
  kind or a line field this engine does not know (a newer engine's strand) as `unsupported by this engine: <field>`: exit 5,
  `{ok: false, line, unsupported, engine, strand_engine}`, never the divergence verdict. A tampered strand still diverges at
  its line (exit 1), the chain checked past an unsupported line too. The released 0.8.2 binary reports a 0.9.0 strand as
  `line 1 diverges` (its output is kept in `probbit-cli/tests/fixtures/strands/`). `probbit check` prints such a strand as
  `line K: unsupported by this engine: <field> (written by <engine>)`, never FAIL (a strand that fails the gate: exit 1), and
  its `--json` entry adds `unsupported` and `strand_engine`; `live --strand` and `live control` name the field when they refuse
  to continue it.
  **Upgrade note:** `live verify` exits 5 on a newer engine's strand where 0.8.2 exited 1; a gate that treats every non-zero
  exit as a failure still fails on such strands.
- `live` says once on stderr when a persona without a senses block receives a `senses` key (still an undeclared input,
  listed in `ignored`). The monitor's panel of the event's inputs is labelled INPUTS (it was SENSES).
- `live` says once on stderr when a persona without a credit block receives `await`, `for` or `cancel` (still undeclared
  inputs, listed in `ignored`; feedback pays the previous stance).

### probbit check and fixes (was 0.8.3, never tagged)

#### Added
- `probbit check PERSONA [--props FILE] [--strand FILE]... [--seeds LIST] [--depth N] [--hours LIST] [--json]`: one gate for
  CI. It runs `persona lint`'s pipeline on the rules (prove each one, then fuzz the ones the bound leaves unknown, the rules'
  and habits' thresholds on the grid) and `live verify` on every strand, then prints one line per rule (`held`,
  `held_by_construction`, `broken seed N`, `unproven`), one per strand (`ok N events` / `FAIL line K: ...`) and
  `check: held H · broken B · unproven U · strands S/T`. Exit 0 all held and replayed; 1 a rule broken, an unresolved habit
  contradiction or a failing strand (1 wins over 3); 3 only unproven rules; 2 bad input. `--json`: one document with the
  `persona lint` report under `rules`, the strands and the verdict counts. No engine change.
- `action.yml`: the repository is a composite GitHub Action (`uses: BitmapAsset/probbit@<ref>`) that installs probbit with
  `install.sh` (input `version`, or `binary` for a prebuilt one), runs `probbit check --json`, annotates broken rules and
  failing strands as errors and unproven rules as warnings with their seeds, uploads `probbit-check.json`
  (`actions/upload-artifact@v4`) and passes exit 3 with a warning when `allow-unproven` is true. Linux and macOS runners;
  Windows runners stop with one error. `.github/workflows/check.yml` runs it on every push and pull request.
- `examples/persona/dispatch-policy.yaml` + `dispatch-props.json`: the rule lock on an agent that learns whether to act or
  ask first. Every rule is held by construction for seeds 0-99; without its routine habit, `check` finds the broken seeds
  (README, "Check in CI").
- `examples/rule-check/demo.py` (Python 3.9 standard library, probbit 0.8.2 or later): the rule-check demo on the dispatch
  example. Without the routine habit, the day-0 check flags 7 of 20 individuals, exactly the 7 that 300 synthetic rewarded
  events teach to ask about routine work (24 of 24 over seeds 0-99); with the habit, `held_by_construction` and 0 of 20.
  `python/test_rule_check_demo.py` runs it small (4 individuals, 20 events, about 2 s).

#### Fixed
- `cargo +1.78.0 test --workspace --locked` compiles again: a CLI test used `std::io::pipe()` (Rust 1.87); it now gets a
  readerless pipe from a child process's stdin. The build MSRV was unaffected.

## 0.8.2 - 2026-10-14

### Added
- `probbit monitor STRAND --open` (and `--serve`) shows an Activity Monitor for one individual: a sortable, filterable
  table of
  the persona's declared traits, moods and, when present, drive channels, with each channel's reported level or value, a trace
  over recorded events and a change against exactly the preceding event (a gap when that document is missing); an inspector
  with the full distribution, units, declared range, source field and the event's `why`; holding a past event while new ones
  arrive; and separate states for replay divergence, stale receiver age, a held view, a paused or retired strand, a
  persona-confidence hold, an engine-unreleased trait and a lost connection. It works for any persona read from the strand.
  The page is embedded in the binary with no external assets; the terminal view (`--once`, `--follow`, `--plain`) is unchanged.
  The monitor's layout gains `caps.prediction_error` from the persona's `expectation.pe_cap`. docs/persona.md §5.8.
- `probbit help [COMMAND [SUBCOMMAND]]`: the same text as `COMMAND [SUBCOMMAND] --help`; `probbit help` alone is
  `probbit --help`. The persona and live subcommands have their own help (usage, flags, exit codes).
- Did-you-mean: an unknown command or subcommand names the closest one (`unknown command "persna"; did you mean
  "persona"?`, Damerau-Levenshtein distance up to 2, or a unique prefix). The exit code stays 2.
- `probbit completions bash|zsh|fish`: a static completion script with every command, subcommand and flag.
- `probbit man` and `docs/probbit.1`: the manual page. Releases attach it; `install.sh` installs it to
  `~/.local/share/man/man1/` (`PROBBIT_MAN_DIR` to choose), never failing the install over it.
- `--json` on every command: errors as one JSON object on stdout even at a terminal, for a bad command or flag too
  (code `flag`, or `command` with a `suggestion`).

### Changed
- `persona lint` with rules (`--never RULE` / `--props FILE`) no longer reports green on a rule it could not settle. When
  `prove` leaves a rule `unknown` and the default `fuzz` search finds no counterexample, the rule is **unproven**: the lint
  document gains `"unproven": [{id, rule, seeds_unknown, searched}]`, `ok` is false, stderr says "unproven, not broken", and the
  exit code is the new **3** (0: every rule held or proved; 1: an unresolved contradiction or a counterexample, which wins over
  3; 2: bad input, as before). Up to 0.8.1 the same case exited 0 with `ok: true`. docs/persona.md §5.5.
  **Upgrade note:** `persona lint` now exits 3 on unproven rules, so a CI gate that treats any non-zero exit as a failure goes
  red on rules that passed silently before. Accept exit 3 explicitly if unproven rules are acceptable to you.
- `persona fuzz`, MCP `probbit_persona_fuzz` and `lint`'s search add every threshold the rule's `when` and the habits' `when`
  read on a number input to the grid (as a fraction of that input's max, after the given points, each once; `search.grid` lists
  them). A rule in force only between two default grid points (for example stakes 0.7 to 0.8) is now reached by the random
  search too. Personas whose rules and habits read no number thresholds off the grid get the same search, byte for byte.
- One command table in the CLI renders every help screen, the completions and the man page, and the parser reads its flags
  from it: what is documented is what is accepted.
- Help fits 100 columns: `probbit --help` starts with the version, groups the commands and ends with examples;
  `monitor --help` has usage, modes, flags and demo sections; the terminal hero screen fits 100 columns.
- Errors: one human line on stderr, always. A bad input's `{"error"}` object goes to stdout when stdout is not a terminal (as
  before for every program and wrapper) or with `--json`; at a terminal without `--json` only the line is printed, no longer
  both (docs/CHEATSHEET.md "Errors").

### Fixed
- `install.sh`: when the install directory is not on `PATH`, the hint names the file the user's shell reads: `~/.zshrc` for
  zsh, `~/.bash_profile` for bash on macOS and `~/.bashrc` elsewhere, `~/.profile` for any other shell (it said "your shell
  profile"). The clean-prefix install test checks each case.

## 0.8.1 - 2026-10-10

### Fixed
- Reject invalid JSON number grammar and raw control characters, platform-sized integer overflow, overflowing linear-cap
  totals, and invalid pair-table dimensions before allocation. WASM no longer wraps large precedence gaps, counts or weights.
- Validate supplied anneal/polish starting assignments even when the work budget is zero.
- Validate stored strand hashes and lifecycle transitions before continuation or control appends; a checkpoint cannot hide
  retirement. Full inference verification and external anchoring remain separate responsibilities.
- Python rejects an explicitly missing binary instead of falling back, and rejects invalid false/array input values rather
  than silently treating them as empty objects. MCP and Python preserve command-specific error/lifecycle contracts.
- Persona `--pretty` consistently formats single-document stdout without changing canonical state files. JSONL replay and
  text explain reject the flag with guidance. Source-restricted fuzz exports require an explicitly labelled synthetic source
  to produce runnable replay commands; production source checks are not bypassed.
- npm forwards each termination signal unchanged, bounds download waits and selects both x86_64 and ARM64 musl assets.
  Shell installation defaults to the user's `~/.local/bin`. Installers validate candidate binaries before replacing a
  working installation on the shell/PowerShell paths; PowerShell stages replacement on the destination filesystem.
  Release/target selectors reject paths.
- Monitor labels scripted demos, loop restarts, lifecycle and checkpoint status; preserves original `why` text, explains
  display terms and supports narrow screens. Browser playground and puzzle tables no longer force horizontal page overflow.
- Keep the lockfile readable by the declared Rust 1.78 minimum; CI builds that compiler with `--locked`.

### Added
- Decision documents with a candidate `plan` gain `plan_status` and `released_plan`. The old full candidate is preserved for
  compatibility; a diagnostic/refused plan is not permission to act. Partial projections may not be independently feasible.
- `probbit monitor --demo drives` shows eight replayable synthetic goal events. The default tutor demo is unchanged.
- `examples/agent-harness`: a local dispatch gate with a recorded incident, history-dependent retry regression, strict
  replay and a scoped repaired-rule proof. No model, credentials or external tool action required.
- Independent enumeration/router oracles, actual WASM boundary tests, real Chromium interaction/layout checks, Node process
  contracts and cross-platform clean-prefix install tests (including Windows PowerShell 5.1/7 and npm 12).

This patch release hardens validation, lifecycle handling and the host-facing contracts. Probability gate thresholds and
frozen benchmark corpora are unchanged. Passing finite tests is not exhaustive validation of every possible program or application.

## 0.8.0 - 2026-10-09

### Added
- **Drives** (docs/persona.md §2.9): persona: optional `drives` block. Goals with wanting, afterglow and an expectation per
  goal; a win moves an individual by its prediction error (magnitude minus expectation), so repeated equal wins stop moving it;
  a new `pursue` output says which goal gets the next unit of effort, with exact odds, under habits; goal floors and
  `starve_after` keep goals from being out-wanted or starved. A persona without the block gives byte-identical documents.
  Read strictly (`probbit_persona` and `probbit_persona_state` stay 1): 2-7 goals (interest, gene spreads, floor, starve_after,
  priority), wanting / afterglow / expectation / pursue weights, effects of the strongest wanting, the strongest afterglow and
  the turn's prediction error, `learn_from_surprise`. Goal signals ride in the inputs under `goals` (progress, novelty, cue,
  setback, win, deadline_hours). Each turn: decay by clock hours, the win's prediction error and Rescorla-Wagner update,
  consumption, afterglow, wanting evidence, caps (6 decimals); `pursue` is one more variable whose field adds the drive terms;
  a goal floor is a lift of that field, sound when no multi-variable rule names `pursue` (such a persona is refused); habits
  read `goal.<id>.deadline_hours|want|glow|expect` and `since_pursued.<id>`, and `starve_after` adds a `starve_<goal>` habit;
  with a learning block the prediction error joins the learning sign. The state gains `drives` (genes per seed, the values per
  goal), checked on read. The stance gains `pursue` and `drives`, the line a `pursue: <goal>: <say>` clause.
- Drives on every surface: `persona turn / replay / explain / describe / check / compile / diff`, `live` events with `goals`
  (strands log them, `live verify` replays them), MCP, the Python wrapper and the browser module. `lint` lists the habits
  that can exclude a floor goal and the floors whose lift can reach 50; `prove` reads the drives as boxes and reports goal
  floors as held by construction; `fuzz` adds goal signals to its events.
- `bench/drives_adversary.py` and BENCHMARKS.md §9: an adversary that wants one goal, over N individuals x 10,000 turns
  through
  `probbit live`. On the drives fixture, 100 x 10,000 turns: 0 habit breaks, every must-do turn (6,743) pursued the due chore,
  the safety floor of 0.1 held on every turn its habits allowed (least odds 0.141), and every individual ends in the state the
  Python prototype of the design reaches on the same events (100 of 100 digests).

- **`probbit monitor STRAND`** (docs/persona.md §5.8, probbit-cli/src/monitor.rs): watch an individual's inner state as live
  horizontal bars. It replays the strand with the rules of `live verify` (`live::Replay`, now shared by both), recomputing every
  stance document from the strand alone, and draws the latest event: each trait's levels with their odds (the level taken
  highlighted, its phrase), the moods with a sparkline of the last 50 events, the event's inputs and history features, the habits
  in force (the bound ones marked, the violations counter), the learned deltas within their cap, the drives when a document
  carries them, and the stance line. `--follow` replays appended lines within a second (a truncated or rotated strand is replayed
  from the start, with a warning), `--once` prints one frame, `--plain` draws in plain ASCII. `probbit monitor --demo` plays the
  tutor's scripted week (the week `live --demo week` writes, seed 2, through a temporary file it removes at once), paced 1 s per
  hour and each night in 2 s. Exit 0, 1 at a line that differs (named in the frame), 2 for a bad flag or a file that is not a
  strand. It changes no file and sends nothing; no dependencies.
- **`probbit monitor STRAND --serve [--port N] [--open]`** (also with `--demo`): the same board as a page in the browser.
  A server
  on 127.0.0.1 (std `TcpListener`; it binds no other address, and refuses requests addressed to another host) follows the strand
  and serves one page embedded in the binary (styles and script inline; it fetches no fonts, styles or scripts), its server-sent
  events (the layout, the latest frame, then a frame per event, a heartbeat every 15 s) and `/doc/N`, event N's stance document
  as `probbit live` printed it. The URL is the line on stdout; `--open` serves the same and starts the default browser, best
  effort. `probbit monitor --demo --open` plays the week over and over in the browser.

- **The safety kit in the engine** (docs/persona.md §2.10, §5.7). Four guards an autonomous loop needs, each tested; personas
  without the new key and strands without the new lines give 0.8.0's documents and strands, byte for byte.
  - **Reward provenance**: an event may say who produced it (`src`: `human[:id]`, `env[:sensor]`, `self`, `clock`), and a
    persona may declare `reward_from` (a list of `human` / `env`, `any`, or one per reward-bearing input: the learning flags and
    `goals.<id>.win`). A reward from `src: self`, without a source or from an undeclared one is refused whole, as a bad event
    is; `src` is read (not `ignored`), echoed in the stance's inputs and logged in the strand. A run equals the same run with
    its refused events removed (P3: 40 individuals x 300 random events). A closed self-reward loop (300 events of praise and a
    win from `src: self`) is refused 300 times and leaves the individual in its initial state, byte for byte.
  - **One writer per strand**: `live --strand` takes `STRAND.lock` (created exclusively with the writer's pid and start; a
    lock whose process is gone is taken over) before it reads the strand and holds it to its exit; a second writer exits 4
    and changes nothing; every append first checks the lock is still the writer's. `probbit_live_event` takes it for its
    append. Two concurrent writers on one state and strand now give one exit 0 and one exit 4, and the strand verifies.
  - **Control lines**: `probbit live control STRAND pause|resume|retire --by human:ID --reason TEXT [--at TIME]` appends a
    chained control line under the lock. While paused or retired every event is refused (code `paused` / `retired`, exit 4,
    nothing written); retire is final; no credit crosses a control line (feedback after a resume credits no stance from before
    the pause) and nothing else moves. `verify` replays them and reports `controls` and `status`. Not an MCP tool, by design.
  - **Checkpoints**: every K-th event (`--checkpoint-every K`, default 1,000; 0 = none) a checkpoint line carries the event
    count, that event's stance and the whole state. `verify` checks each against the replay; `verify --from-checkpoint`
    replays from the last one; `monitor` (`--once`, and the first read of `--follow` / `--serve`) starts there and draws its
    event at once, and shows a paused or retired status next to its badge.
- Exit code 4 for `live` and `live control`: the writer lock is held by another writer, or the individual is paused or
  retired.

### Changed
- MCP: the input schemas of `probbit_persona_turn` (`inputs`) and `probbit_live_event` (`event`) declare `goals`, so a client
  that checks arguments against them passes a drives persona's goal signals.

## 0.7.0 - 2026-10-06

### Added
- **Bounded learning** (docs/persona.md §2.8): an optional persona block `learning: {from: [reward, correction], traits,
  rate,
  step_cap, total_cap}`, read strictly (`probbit_persona` stays 1). With it the state keeps `learned` (a weight Δ per level of each
  learned trait, 0 at `init`) and `credit` (the previous stance's onehot(level) − odds; 0 for a level a habit or a hold forced),
  both checked on read (Δ within ±total_cap, credit within ±1). On a turn whose reward flag is on (correction: sign −1) each
  learned trait's step is clip(sign x rate x credit, ±step_cap) per level, recentred to sum 0, added, and Δ is clipped to
  ±total_cap (6 decimals); Δ is added to the trait's unary weights when the turn compiles. The learner's whole output is the Δ
  table: habits and allowed levels are rules of every program, so every stance obeys every habit in force whatever Δ is. A
  persona without the block gives every document byte for byte as 0.6.0 did (the three example goldens are unchanged).
- **`prove` covers learning**: the learned weights of traits linked to the rule's trait stay in their box (±total_cap per
  level) whatever the feedback, and the bound includes it; verdicts unchanged. Soundness tests: every event sequence of tiny
  learning personas (brute force, learned deltas included), a targeted case the learned box decides, and a mutation that drops
  the learned term from the bound, which the tests catch (0.6.0's mutation checks still fail as they should).
- **`probbit live`** (docs/persona.md §5.7, probbit-cli/src/live.rs): a resident individual. JSONL events in (`--events
  FILE` or
  stdin), one stance per event out. `--clock real` (default) stamps each event's `elapsed_hours` from a monotonic clock,
  quantised to 1e-6 h, so moods decay by their half-lives between events; `--clock fixed` reads them from the events (a pure
  function of the events). `--seed N` or `--state FILE` (rewritten after every event). `--strand FILE` logs the life: a header
  (engine version, persona name / version / digest, seed, the initial state as canonical JSON, the persona document in its own
  key order), then per event the inputs as used, the stance digest, the new state's digest and the sha256 of the line before. A
  strand is never rewritten, just appended to: `--strand` on an existing strand continues it from the state after its last line,
  and a life continued over runs is the strand of one run, byte for byte. `--events FILE --watch` follows a file as lines are appended.
- **`probbit live verify STRAND`**: replays a strand from its header alone and prints `{"ok": true, events, persona, seed,
  engine, final_state, last_line}` (exit 0) or `{"ok": false, line, diverges}` for the earliest line that differs (exit 1). The
  hashes cover each line's text without its line ending, so a `\r\n` copy verifies.
- **`probbit live PERSONA --demo week [--seed N] [--strand FILE] [--plain]`**: one individual's scripted week on the fixed
  clock
  (events hourly 09:00-15:00, quiet nights): praise for short answers until the learned verbosity weights reach their cap, an
  upset and a quiet night, then a campaign that praises every joke while every third hour reports a failure. A persona without a
  learning block gets the demo's (the opening line says so; the strand records the document). Bars on stderr at a colour
  terminal (1 s per hour, nights fast-forwarded), else one line per event. The tutor, seed 2: 50 events, the verbosity cap on
  day 1 at 15:00, valence back to its resting level after a 17-hour night, P(joke) off failure turns 0.57 on day 1 and 0.89 on
  days 3-7, 13 failure turns with no joke, 0 rule breaks, a strand that verifies; the same bytes every run.
- MCP tools `probbit_live_event` (one event: the persona, the state or a seed, the event, optionally a strand file to log
  to ->
  `{stance, state}`) and `probbit_live_verify` (a strand's text or path -> verify's document; a divergence is an answer): ten
  tools. Python: `probbit.live_event(...)` and `probbit.live_verify(...)` (the same documents, through the CLI).
- `probbit persona describe` lists the learning block when there is one.
- `bench/live_learning.py` (stdlib, drives the binary) and BENCHMARKS.md §8: an adversary that praises every joke and
  criticises every joke-free stance, failures included, over 10,000 turns on each of seeds 0-9 of the tutor: 0 rule breaks and
  no joke on any of the 33,330 failure turns, while the learned humour weights reach their cap within 8-26 turns; a 10,000-event
  strand verifies in 3.3 s; how far learning moves an individual is set by `total_cap`, not by time in use (at 0.25 all 20
  learners stay nearer their own initial self than any of 99 siblings; at 1, 9 of 20 have a sibling as near or nearer).
- Tests: mood decay against the closed form on the fixed clock; caps; an adversary over 2,000 turns (no rule broken, humour
  learned to its cap); identity against the 99 siblings at two caps; strands replayed, continued, changed (divergence at the
  changed line), converted to `\r\n`; one header per individual (from `init` or a state file); the demo deterministic; the MCP
  and Python live surfaces.
- docs/persona.md §2.8 Learning and §5.7 Live (what decays, what learns, what can never change, how far learning moves an
  individual, the strand, verify, `--watch`, the demo); README "Live"; docs/agents.md the two tools.

### Changed
- Version 0.7.0 (Cargo manifests, npm package, installers' examples, the bench workflow's tag); the golden test is
  `stdout_matches_the_0_7_0_goldens` (the documents are unchanged).
- docs/persona.md: compilation step 2 adds the learned weights, the state (§5.1) shows `learned` and `credit`, `prove`'s
  bound
  (§5.6) includes the learned box, §9 lists the strand's format number.

## 0.6.0 - 2026-10-06

### Added
- **`probbit persona fuzz`, character testing** (docs/persona.md): a rule in habit syntax (`--never '{when: {...}, then:
  {...}}'`, or several in `--props FILE`) and a population of individuals (`--seeds 0-99`). It searches event scripts built from
  the persona's declared inputs (flags, levels, numbers on `--grid`, idle hours on `--hours`): random scripts, then a beam guided
  by the exact odds (the next events that put the most odds on a level the rule forbids). It shrinks each individual's shortest
  counterexample by delta debugging and prints it with the breaking turn's levels, odds, why and stance line and the exact
  `probbit persona replay` / `explain` commands that reproduce it. Human report or `--json` (`probbit_persona_fuzz: 1`).
  Deterministic: Philox (probbit-core) keyed by `--fuzz-seed`; byte-identical output for the same inputs on any number of
  threads (timing on stderr). Exit 0 nothing found, 1 a counterexample, 2 bad input. It tests the stance a host gets, not the
  words a model writes; "none found" is evidence, not a proof.
- **`probbit persona prove`**: the same rules, decided for every event sequence where it can be. One verdict per rule over
  the
  population: `held by construction` (the habits in force whenever the rule applies imply it; under `on_conflict: yield` a habit
  counts when no conflict on such a turn ever drops it), `proved for every event sequence` (a sound bound per individual: the mood accumulators
  stay in a box, inputs, history values and previous levels fall into finitely many cells, and in every cell the best allowed
  stance beats the best forbidden one by more than the box can move them), or `unknown` (the earliest cell the bound cannot
  decide, with the `fuzz` command to search it). Human report or `--json` (`probbit_persona_prove: 1`). Exit 0 every rule held
  or proved, 1 some unknown, 2 bad input. Soundness tests: every event sequence of tiny random personas, the fuzzer on random
  personas, and mutations of the bound that the tests catch. It covers the stance, not the model's words.
- `probbit persona lint --props FILE` (or `--never RULE`): the contradictions as before, then every rule proved, and the ones
  the bound leaves unknown fuzzed; the lint document gets `props` and `broken`; exit 1 when a rule breaks (CI-friendly).
- MCP tool `probbit_persona_fuzz` (the eighth tool of `probbit mcp`): `probbit persona fuzz --json`'s document for a persona
  (inline or a path) and `never` / `props`, byte for byte, in the server's process. Python: `probbit.persona_fuzz(...)` and
  `probbit.persona_prove(...)` return the `--json` documents (a counterexample or an unknown rule is an answer).
- docs/persona.md §5.6 "Testing a character": what a property is, both commands, the verdicts and the bound, the limits.
- `probbit-cli/tests/fixtures/persona/tutor-0.5.0.yaml`: the tutor as it shipped in 0.5.0, byte for byte, with the pinned
  fuzz report on it (one upset message, `{"sentiment":"negative"}`, gives seed 1 a playful stance).
- `probbit persona lint` warnings: a trait whose planned level differs from its own most likely level (its marginal mode)
  on a
  probe turn (each individual of `--seeds` at rest, a quiet turn, then every declared input alone) is listed under `warnings`
  with the count and the earliest case. A warning leaves the exit code alone, and decoding is unchanged (the stance is still the
  joint plan); docs/persona.md §5.5 gives the options (a habit, a `vouch` floor with a `hold`, or the trait's `odds`).
- README "Test a character": the fuzz counterexample on the 0.5.0 tutor and `held by construction` on the tutor as it
  ships now.

### Changed
- Version 0.6.0 (Cargo manifests, npm package, installers' examples, the bench workflow's tag); the golden test is
  `stdout_matches_the_0_6_0_goldens` (the documents are unchanged).
- **The tutor example** (examples/persona/tutor.yaml, persona version 1.1.0) has one habit more, `no_play_when_upset` (`when:
  {sentiment: negative}`, `then: {humour: {at_most: light}}`). In 0.5.0 `persona fuzz` found 67 of seeds 0-99 playful with an
  upset learner (37 of them after the single message `{"sentiment": "negative"}`), and the shipped workday golden itself had
  seed 1 playful on turn 11 ("still upset"); that turn is now light, and `persona prove` says the rule is held by construction.
  The tutor's digest and goldens change; its genes (individuals) do not. The habit fixes the stance the host gets, not the words
  a model writes.
- `why` names an input's push by its direction (`learner upset -> valence down, humour down`) where it named the trait's
  lowest or highest level ("humour none" printed next to a playful stance read as a contradiction). In the ops engineer's and
  the trader assistant's goldens nothing but `why` changes.
- docs/persona.md §5.3: the byte-for-byte parity with the reference implementation is stated for the 0.5.0 documents; the
  0.6.0
  goldens are regenerated from this implementation.
- brand: the agent rabbit mark (visor + lightning ear) replaces the 0.5.0 mark (docs/brand/, the hero, the social card, the
  playground's favicon).

## 0.5.0 - 2026-10-05

### Added
- **`probbit persona`, the individuality layer** (docs/persona.md): a persona file (YAML subset or JSON: traits with priors,
  moods with inertia, couplings, per-turn inputs, history features, habits as hard rules in the probbit-ir vocabulary) + an
  individual's state + a turn's inputs compile to one probbit-ir program, run in process at fixed work; the stance (levels with
  exact odds, habits in force and bound, refusals, a why and a stance line of at most 40 estimated tokens) and the next state are
  canonical JSON. Subcommands `init`, `turn`, `replay`, `explain`, `diff`, `lint`, `check`, `compile`, `describe`. A strict YAML
  subset reader (probbit-cli/src/yaml.rs) and a strict validator (one `{"error": {"code": "persona", ...}}` object, exit 2). Three
  example personas with goldens in examples/persona/; the documents equal an independent reference implementation's.
- **MCP tools `probbit_persona_init` and `probbit_persona_turn`** in `probbit mcp` (stateless; the CLI's documents).
- **Python `probbit.persona_init`, `persona_turn`, `persona_replay`** (python/probbit.py; tests python/test_persona.py).
- **probbit-wasm ops 4 (persona init) and 5 (persona turn)**; the playground's "Meet three individuals from one persona".
- `probbit persona ... --timing` reports the 1-minute load average next to its timings (`sys::loadavg`).

### Changed
- `probbit mcp` lists seven tools (the two persona tools after the five before; their order and contracts are unchanged).

- **Renamed: pbit → probbit everywhere** (crates, binary, env vars, MCP tools, Python module, npm package). 'p-bit'
  remains the
  term for the probabilistic bit. No behaviour changes.
  - Crates `probbit-core`, `probbit-ir`, `probbit-decide`, `probbit-cli` (binary `probbit`), `probbit-wasm`; Rust paths
    `probbit_core::` etc.
  - Environment variables `PROBBIT_THEME`, `PROBBIT_CONFIG`, `PROBBIT_CHAINS`, `PROBBIT_THREADS`, `PROBBIT_CPU_LIMIT`,
    `PROBBIT_MEM_LIMIT_MB`, `PROBBIT_PRIORITY`; installer variables `PROBBIT_VERSION`, `PROBBIT_INSTALL_DIR`,
    `PROBBIT_DOWNLOAD_BASE`, ...; config file `probbit.json`; cache directory `$XDG_CACHE_HOME/probbit`.
  - MCP server `probbit`, tools `probbit_decide`, `probbit_run`, `probbit_stats`, `probbit_demo`, `probbit_evaluate`.
  - The IR's version key `probbit_ir` and the evaluate request's namespaced `probbit` block; the `engine` field reads
    `probbit 0.5.0`. The documents are otherwise byte for byte the 0.4.0 documents (proven on the 7 golden inputs and every
    example; the golden test is now `stdout_matches_the_0_5_0_goldens`).
  - Python module `probbit.py` (`ProbbitError`, ...), npm package and bin `probbit`, release assets `probbit-<tag>-<target>`,
    the playground's `probbit.wasm` / `probbit-wasm.js` (C ABI `probbit_*`, clock import `probbit.now_ms`).
  - The terminal hero screen spells probbit (its command column is 6 characters wider to fit the longer name).
  - Repository `github.com/BitmapAsset/probbit` (the old URLs redirect); the IR schema `$id` now names it (it named a wrong owner).
- Migration: rename the binary, env vars, MCP tool names, `"pbit_ir": 1` → `"probbit_ir": 1` and the evaluate `pbit` block →
  `probbit` in your documents and scripts; nothing else changes.

## 0.4.0 - unreleased

### Added
- **`pbit evaluate`**, the decision-API adapter: a System One request (`model`, `state`, `questions` keyed by id, types
  `noul`,
  `choice` and `score`, `images`; the shape published by TypeSafe's OpenAPI 0.2.0 and the Workers AI `clef` schemas) plus an
  optional `pbit` block (`judge`: the judge's System One response; or `weights` / `logw` per question; `floor`; `rules`: the pbit-ir
  constructs over question ids). It compiles to a pbit-ir program (one variable per question, log-weight = ln max(p, 1e-6)), runs
  it as `pbit run` does with every `pbit run` flag, and answers in the judge's response shape (`model`, `answers`, `usage`) plus
  the `pbit run` document; each answer carries a `pbit` object (`value`, `p`, `judge`, `changed`, `released`). `--program` prints
  the compiled program. Same exit codes, error objects, `--summary`, `--pretty` and terminal rules as `pbit run`; a rule naming an
  option its question does not have is a `value` error at the rule's path (the program's value names are shared by all questions).
  docs/pbit-ir-json.md "Decision API"; tests in `pbit-cli/tests/evaluate.rs`.
- **MCP tool `pbit_evaluate`** in `pbit mcp` (the same contract; its input schema carries the pbit-ir rule definitions).
- **Python `pbit.evaluate(request, judge=None)`**: the judge is a callable (returning a System One response or per-question
  probabilities) or the URL of a System-One-compatible server, called with `urllib` (a key from an environment variable the
  caller names; the Workers AI REST envelope is unwrapped); `PbitJudgeError` for judge failures. `python/mock_judge.py`: a stdlib
  System One server for tests and demos.
- **Single-threaded engine path and an injected clock** (`pbit_core::rt`): the engine's `Instant` (std's where it exists; on
  wasm32-unknown-unknown a millisecond clock the embedder installs) and a per-thread switch that runs every parallel section of
  the command paths (chains, gate passes, polish, the exact tiers' deep-stack thread) in order on the calling thread. At fixed
  work the answers equal those on any number of threads.
- **`pbit-wasm`** (not published; no dependencies): `decide`, `run`, `evaluate` and `demo` as JSON-in / JSON-out functions
  for
  wasm32-unknown-unknown over a small C ABI, running the CLI's own front-end code without threads (830,233-byte module).
  **`playground/index.html`**: one static page, no server or framework, that runs the 300-task demo and `evaluate` with an
  editable JSON box; `playground/build.sh` builds the module; CI job `wasm` builds it and runs it under Node.

### Changed
- `pbit decide`'s pipeline moved from `main.rs` to `router.rs` (the browser build shares it); output unchanged (golden
  tests).

## 0.3.0 - 2026-10-02

### Added
- **Hero screen.** At a terminal, `pbit` and `pbit --help` print a 9-line screen and exit 0: the wordmark, the status line
  `PROCESSOR ONLINE · 0.3.0 · exact → sample → gate`, a spec line (os/arch, logical CPUs, target features and the site
  updates/s of a 30 ms self-test, cached for a day in `$XDG_CACHE_HOME/pbit/stats.json` or `~/.cache/pbit/stats.json`), three
  commands to try and the usage. Piped, `pbit --help` prints the usage as before (now listing the new flags and `pbit mcp`)
  and a bare `pbit` still exits 2.
- **`pbit demo --live`**, the default when stdout and stderr are both terminals: the router story inside the binary (no
  cargo needed). The queue and its rules, what a per-task argmax router breaks, the exact tiers and what they decline, then
  a field of p-bits (one per task and worker, half-block cells sized to the terminal) drawn from the current states of four
  real chains, the gate re-run every 200 sweeps on the samples so far, the ladder refused → partial → passed, a 3-line verdict
  stamp, and the command that gives the final look as JSON: the chains are recorded exactly as `pbit decide --sweeps 3200
  --polish-ms 0` records them (unit test `live_stepper_matches_sample_on`). Deterministic under `--seed` (fixed work per
  frame); 20 frames per second; `--tasks` defaults to 300 in this mode; stdout, when not a terminal, gets the problem
  document. The renderer took 2.0-2.4% of the process CPU on an Apple M4.
- **`--top`** (`decide`, `run`): a monitor on stderr redrawn at 10 Hz (tier, sweeps against the budget, site updates/s, CPU,
  peak RSS, the gate once it ran), erased at exit. Only with a terminal on stderr; `--top --progress` is a flag error.
- **`--summary`** (`decide`, `run`): the answer without its per-item tables: verdict, `counts`, the gate, telemetry and the
  items to look at first (`worst_released`, `worst_escalated`: up to 5 each, with plan value, its probability, the top two
  odds, the release reason and the error bar). Same verdict and exit code. With `--pretty` and a terminal on stderr, a boxed
  summary there too. docs/pbit-ir-json.md "Summary".
- **`pbit mcp`**: a Model Context Protocol server on stdio (hand-written JSON-RPC 2.0, no crates). Checked against the MCP
  specification revision 2026-07-28: requests with per-request metadata are served statelessly (`server/discover`,
  `tools/list`, `tools/call`, `ping`), and `initialize` serves the handshake revisions 2025-11-25 back to 2024-11-05. Tools
  `pbit_decide`, `pbit_run`, `pbit_stats`, `pbit_demo` take the commands' own documents (plus a `flags` object) and return
  the commands' own JSON. Tested by a dependency-free Python client (`python/test_mcp.py`); docs/agents.md "MCP: one line per
  agent" (Claude Code, Codex CLI, Cursor, mcporter, LangChain).
- `--plain` on every command and `PBIT_THEME=neon|plain`; `NO_COLOR` is honoured. One palette (`theme.rs`: cyan #00F0FF,
  magenta #FF2A6D, yellow #F9F002 in 256 colours, with a 16-colour fallback) for the hero, `--top`, `--live` and the summary
  box; at a terminal, `install.sh` ends with the hero screen.
- `pbit_decide::Chain::set_group_pairs`, for callers that step router chains themselves.

### Changed
- `pbit decide --mode exact` that cannot answer (stopped by `--exact-ms`, or the feasible set too large) prints a `declined`
  document and exits 3, as `pbit run --op exact` does. It printed a line on stderr and exited 2 with an empty stdout.
- One leading UTF-8 byte-order mark on stdin is skipped. Windows PowerShell 5.1 adds one to every pipe, so `pbit demo | pbit
  decide` exited 2 there (`bad JSON: unexpected character`).
- Python wrapper: a boolean switch passes the bare flag (`pretty=True` -> `--pretty`, `summary=True` -> `--summary`; False
  leaves it out). It became `--pretty on`, exit 2. `collective`, `cluster` and `cycles` still map to `on` / `off`.
- Crates: the path dependencies carry `version = "0.3.0"` and the workspace declares `rust-version = "1.78"`; `cargo
  publish --dry-run -p pbit-core -p pbit-ir -p pbit-decide -p pbit-cli` passes (nothing is published). The npm wrapper
  (`npm/package.json`), the installers' examples and the benchmark workflow point at v0.3.0.
- release.yml: static musl archives for x86_64 and aarch64 Linux, the Windows binary links the C runtime statically (no
  VCRUNTIME140.dll), and the macOS Intel build is smoke-tested under Rosetta 2. CI builds these four targets on every push
  and pull request (no tag, no release).
- `run_deadline_ms_bounds_the_whole_call` allows 1,800 ms when `CI` is set (a 600 ms deadline took 990 ms on the macOS
  Intel runner; the deadline is a target, not a hard bound on a slow CPU).
- Documentation: "First five minutes" and an install table in the README; the instruction set lists every construct
  (`min`, `all_different`, `implies`, `tables`, `precedes`, `linear`; cost budgets are expressible in `pbit run`
  programs); budget semantics; modelling notes (one variable per entity instead of `implies` chains; plan or odds);
  docs/pbit-ir-json.md without internal round tags; USE-CASES says `pbit ir` reads router documents only; the flow figure's
  source drops retired wording.

Every document of `decide`, `run`, `stats`, `demo` and `ir` is unchanged: stdout is byte-identical to 0.2.1 apart from the
version and the timings (golden digests in `stdout_matches_the_0_2_1_goldens`; the engine's own golden digests are
unchanged).

## 0.2.1 - 2026-10-01

### Fixed (found by the 2026-10-01 independent review)
- `pbit decide` aborted (exit 134, empty stdout) on a router document with 65,536 or more workers. Workers are now limited to
  65,535 (the IR's value limit), a `limit` error with exit 2, and `Problem::lower_until` declines instead of panicking.
- `--chains 576460752303423488` aborted `decide`, `run` and `stats` (exit 134) and `--chains 1000000000` exhausted memory.
  `--chains` is now 1..=100,000 and `--threads` 1..=1,024 from any source (flag, `PBIT_*`, `pbit.json`), else exit 2; the
  memory-limit arithmetic is checked.
- A failed write to stderr (a pipe whose reader had gone) aborted the process (exit 134): a bad flag lost its exit code, and
  `--progress` into a reader that stopped lost the whole decision. Every stderr line now ignores a failed write.
- Invalid UTF-8 on stdin and an unreadable stdin (`pbit decide < /`) exited 2 with nothing on stdout. Both are now ONE
  `schema` error object on stdout, like any bad input.
- The router sampler's two-group move stored worker indices and bucket loads in 8 bits: with more than 255 workers in a
  group a
  task could be placed on a worker it does not allow and the sampled odds were wrong (the gate refused those runs). Such groups
  now skip the move, which keeps the chain exact; loads add without wrapping, and caps of t or more (legal up to 2^53) no longer
  truncate in its 32-bit arithmetic. Golden digests unchanged.
- Small documents could allocate gigabytes (a 0.6 MB program peaked at 4.1-6.3 GB, a 30 KB precedence at 1.45 GB). New
  `limit`
  errors, checked before the allocation: at most 20,000,000 (variable, value) pairs (n x k; tasks x workers on the router), at
  most 100,000 slot pairs per `precedes` (as for `tables`), at most 20,000,000 cap members in total; `--max-input-mb N`
  (`decide`, `run`, `ir`; default 256, 0 = no limit) bounds stdin. `precedes` loops over allowed slots only (same caps).
- Escaped UTF-16 surrogate pairs (`"\ud83d\ude00"`, how Python's `json.dumps` writes an emoji) decoded to two U+FFFD, so ids
  came back changed and two emoji ids collided. A pair is now one character, and a lone surrogate is a `schema` error. The
  Python wrapper reads pbit's output as UTF-8 whatever the locale.
- `pbit-core`: the multispin and heat-bath lattices had `pub` fields, so safe code could reach undefined behaviour in the
  `unsafe` fast kernels (a wrapped `l * l` length check; heat-bath couplings off ±1 read past the lookup table). The fields are
  private now: build a lattice with `random` or the checked `new` (exact lengths; ±1 values for the heat-bath), read it with
  `l()`, `w()` / `s()`, `jr()`, `jd()`; the length checks no longer wrap. Kernel output and speed unchanged.
- `cargo clippy` failed with default lints (and so never reached `pbit-cli`); it now passes. The seeded generators keep the
  literal 6.283185307 (allowed with a reason), because golden digests and published instances depend on its exact bits.
- `pbit --help` and `pbit -h` print the usage on stdout and exit 0 (they exited 2 with the usage on stderr).
- `pbit.json`: an unknown key (`{"thread": 3}`) or a non-object was ignored silently; now exit 2. `decide` and `run` name the
  config file in `telemetry.config` when there is one (it can change the answer of an otherwise identical command).
- Process telemetry (`process_cpu_ms`, `peak_rss_mb`) is read only on 64-bit Unix, where the `getrusage` layout is known
  (`null` elsewhere; 32-bit Linux or BSD would have read garbage).
- Two wall-clock tests tolerate slow CI runners: `acc1_tv_le_001_within_10ms` allows 25 ms when `CI` is set, and the sudoku
  test's passing sampled run gets 300 ms instead of 60.

## 0.2.0 - 2026-10-01

### Changed (breaking for malformed input)
- Strict input contract for both JSON front-ends (`pbit decide` / `pbit ir` router documents, `pbit run` pbit-ir programs):
  every field type-checked when present, unknown fields rejected, duplicate ids / value names / object keys rejected, empty
  domains rejected, only finite numbers accepted, nesting limited to 256. Bad input now exits 2 with ONE structured object
  `{"error":{"code":"schema|value|limit","path","message"}}` on stdout (plus a human line on stderr); flag errors are unchanged.
  Before, `"allowed": "A"` was ignored (every scored worker allowed), a cap object in place of the caps array was dropped,
  duplicate value names emitted duplicate keys, and `1e309` aborted the process (exit 134). Valid documents give identical
  answers (8/8 fixed-work outputs byte-identical after stripping timings); parsing a 100,000-task document costs +7.7%
  (`pbit ir`, 461 -> 496 ms median of 7, Apple M4, load ~5.9). See docs/pbit-ir-json.md "Input contract".

### Changed (trust contract)
- The sampled whole-answer verdict `certified` is retired: it is now `diagnostics_passed` (same test). Library:
  `Verdict::DiagnosticsPassed`,
  `Gate::diagnostics_passed`, `Gate::released_tasks`, `Anytime::passed_at_ms` (were `Certified`, `certified`, `certified_tasks`,
  `certified_at_ms`; the last renamed together with the crate doc of `pbit` and the agent_router / anytime / frontier_exact example labels). Vocabulary: `exact` and
  `infeasible` are proof-grade under the score model; `diagnostics_passed` and `partial` are sampled and gated; `refused` escalates.
- Every sampled answer carries gate/2: `gate.version` ("gate/2"), `gate.assumptions`, `gate.mcse_tv_short` /
  `mcse_tv_long` (both
  batch-means MCSEs), `gate.min_batches_long` (the long pass's batch count, now also required to be >= 8: the 0.1.0 gate checked only the short pass;
  measured: 2 of 5 partial answers at 300 sweeps per chain (12 / 15 released) become refused, 0 change at 1,000+ sweeps and at the
  200 ms default on the 300-task demo, 2,674 vs 2,701 released over 19 runs; on the max-cut oracle suite (`maxcut_oracle`, 144 runs)
  72 -> 61 whole answers and 1,346 -> 929 released spins, 0 false before and after: every 500-sweep run now refuses, 7 long batches), `gate.worst`
  (the worst item's per-chain means and kept rows) and a per-item `release_reason` (`whole_answer_gate`, `item_gate`, or the first
  failed check: `frozen`, `rhat`, `batches`, `batch_stability`, `item_bound`).
- README claims rewritten: no "certified joint decisions", odds are Monte-Carlo estimates under the supplied score model (not
  predictive calibration), the 64 kernel replicas share one random draw per site, the naive-router baseline is a rule-ignoring
  argmax over synthetic scores.
- Example printouts (re-run on the 0.2.0 build): `agent_router` (the README's first demo), `dispatch`, `router_bench`,
  `transfer`, `maxcut_oracle`,
  `colouring_oracle` and `schedule_oracle` still printed the retired word (`CERTIFIED` / `certified`) and `agent_router` said
  "gate promised <= 0.05"; they now print `DIAGNOSTICS PASSED` / `passed` / "gate tolerance 0.05". Labels only: no logic or
  number changed. Comments that describe 0.1.0 history keep the old word.

### Added (Python wrapper, `--help`, schema)
- `python/pbit.py`: a zero-dependency (stdlib, Python >= 3.9) subprocess wrapper: `run` / `exact` / `sample` (pbit-ir
  programs),
  `decide` / `demo` (router documents), keyword flags (`budget_ms=200` -> `--budget-ms 200`, booleans -> on/off), `deadline_ms`
  (passed to the CLI, plus a kill backstop), typed errors (`PbitInputError` with code / path / message for exit 2, flag errors as
  code "flag"; `PbitNumericError`; `PbitTimeout`; `PbitError`); `infeasible` (exit 1) and `refused` / `declined` (exit 3) are
  answers, not exceptions. `python/examples/` (router, knapsack, agent planner); `python/test_pbit.py` (9 tests) runs inside
  `cargo test` (CLI test `python_wrapper_tests_pass`; skipped only without python3 >= 3.9).
- `pbit <command> --help` / `-h` (decide, run, demo, ir, stats, version): usage, every accepted flag with its meaning and
  default, the precedence of controls, and the exit codes (stdout, exit 0). The flag list is the one the parser enforces
  (`flags_of`), so help and parser cannot drift (test `every_command_has_help`). Before, `--help` after a command was an
  unknown-flag error (exit 2).
- `docs/pbit-ir.schema.json`: a hand-written JSON Schema (draft 2020-12) of the pbit-ir wire format with three runnable
  examples. Test `ir_schema_matches_the_parser`: every object's property list equals the parser's own known-field list
  (read from the CLI's "unknown field" error for an injected probe, 11 shapes), every schema example answers `exact`, and
  every example program and stress input parses.

### Added (collective moves)
- `--collective on|off` for `pbit decide` and `pbit run` (default **on**; `Problem::collective` / `Model::collective` in the
  libraries, default off there): per sweep, a global flip of every free variable with exactly two candidate values and, for
  k > 2, a swap of two value labels everywhere; each a symmetric involution with a Metropolis accept and every cap checked.
  The router and IR samplers stay bit-identical with it on. The gate output reports `gate.collective`. Effect: the frozen
  stress corpus (the external review's counterexamples) goes from 2-3/20 false whole answers per family to 0/20, 0 wrong released items;
  seeds 1..100: 12/100 and 14/100 false -> 0/100 (BENCHMARKS "Known failure modes"). Sampled answers change for the same seed
  (the moves consume random draws); `--collective off` reproduced 0.1.x sampling when this shipped; since `--cluster` and
  `--cycles` became default-on, that needs `--collective off --cluster off --cycles off` *(inferred from the earlier check)*.
- `--cluster on` is the DEFAULT (calibration, below); `--cycles on` is the DEFAULT (below). `--cluster on` = a Wolff
  cluster move over the non-negative Potts bonds (boundary
  bonds cancel against the proposal ratio; tables and negative couplings enter the accept as plain terms) and `--cycles on` =
  three-cycle rotations that keep every value's count (connect capacity-saturated states no pairwise swap connects). Both
  samplers stay bit-identical with any combination on. Fixed-work cost (1 thread, Apple M4, load ~10-15): cluster +0% on the
  300-task demo, +93.6% on the 32-task one-group router, +26.7% on ferro12; cycles +10.7% / +1.9% / +4.1% (demo / router /
  chain100); the cluster figures were measured before the move became a probability-1/2 mixture per sweep (about half
  expected, unmeasured). That change fixed a composition hazard: attempted every sweep next to the global flip, the two
  cancelled on ferro12 (2/10 false whole answers; BENCHMARKS "Known failure modes"). Default-on decisions wait for the
  calibration corpus.

### Added (inference compiler)
- Exact-zero factors are eliminated before tier selection (`pbit run` / IR JSON lowering): a pair whose every entry is
  exactly
  0 (`potts: 0`, an all-zero table) changes no state's log-weight, but it linked its ends and hid independence from the exact
  tiers. Review case E17 input (32 independent variables + all 496 pairs as `potts: 0`): sampled 115.7 ms (single run, this machine;
  the reviewer: 118.5 ms) -> exact frontier, median 0.051 ms (N = 7, Apple M4, load ~6), the same as without the pairs (0.053 ms).
  Constant and separable tables (t[a][b] = u[a] + w[b] exactly in f64) are folded into the two variables' unaries
  (`pbit_ir::compile_parts` / `Model::compiled`, counted as `compiled.tables_folded`): log Z, odds and plans unchanged up to
  float summation order; 32 independent variables + 496 constant tables (0.5): sampled 385.7 ms -> exact frontier 0.045 ms
  (medians N = 5, load ~15), log Z = the table-free log Z + 248. `pbit decide` runs its components tier on the compiled lowered
  program. Tests `zero_factors_do_not_hide_independence`, `compile_pass_preserves_the_distribution` (200 random programs,
  enumeration of compiled vs original, 1e-12).
- Redundant capacity constraints are dropped in the same pass: a cap whose limit reaches its number of distinct member
  variables can never bind (each variable holds one value). The external review's ferro12 redundant-caps input: identical odds to the cap-free
  program; sampling 51.40 -> 45.06 ms (-12.3%) at `--sweeps 20000 --threads 1`, medians of N = 5 alternating, load ~8.5. The
  moves-off stress counts for it are unchanged (3/20 false: those chains move, inside one wrong mode). Test
  `redundant_caps_are_dropped_binding_caps_kept`.
- Every `pbit run` answer carries `compiled` {pairs_dropped, caps_dropped, tables_folded}: what the compile pass removed
  (E17 input: 496 / 0;
  ferro12 redundant-caps input: 0 / 2).
- Components + forest exact tier (`pbit run`, `exact_components_until` in pbit-ir): connected components of
  pairs and caps are solved one by one and recombined (log Z adds, odds per component); cap-free trees by log-space
  sum-product + max-product, other components by enumeration or the frontier DP. `tier: "forest"` / `"components"` +
  `components` {count, forest, enumerate, frontier}. The external review's chain100 / chain1000 at defaults: sampled 281.9 / 281.3 ms
  (`diagnostics_passed`) -> exact 0.055 / 0.390 ms (medians N = 5, Apple M4, load 3.6-11); four independent capped rings
  (non-partition, 2^32 raw space): 385.5 ms sampled -> 0.057 ms exact. Tests `components_and_forest_match_enumeration`
  (400 random programs vs enumeration, 1e-9; chain1000 log Z vs a transfer matrix) and `stress_chain100_forest_tier`.
  Sampled answers on such programs become exact answers (a behaviour change for `decide`; `--op sample` is unchanged).
- Occupancy-count DP inside the components tier: a component whose members take two values, with one identical
  symmetric coupling on every pair (complete graph; or no pairs) and caps that count every member able to take their value,
  is solved in O(n^2) in log space (odds clamped into [0, 1]; n <= 2048). `pbit decide` now runs the components tier on its
  lowered program (before whole-program enumeration when the raw space exceeds `--exact-limit`, else after the frontier;
  `--frontier-states 0` = off): the external review's asymmetric-router32 h0.03 / h0.06 and heterogeneous-router32 at defaults: sampled
  `diagnostics_passed` 428.8 / 426.4 / 423.4 ms -> exact (tier `occupancy`) 0.092 / 0.087 / 0.086 ms (medians N = 5, Apple M4,
  load 8-18). Tests `occupancy_count_dp_matches_enumeration` (300 random programs incl. near misses that must NOT be counted,
  infeasible caps; n = 512 vs the closed form, 1e-9) and `stress_router_occupancy_tier` (CLI vs the stress oracle).
  The router stress families also run with `--mode sample` so the gate is still exercised (20/20, 0 false).
- `--cycles on` is the DEFAULT (CLI, both commands; library `Model::cycles` / `Problem::cycles` stay off), and the
  rotations are attempted only on programs with k > 2 values and at least one cap (IR and router alike, still bit-identical):
  on two-value or cap-free programs the answer is byte-identical to `--cycles off` at fixed work (ferro12, chain100,
  asymmetric router, 2,000 sweeps, seed 3). Evidence: a new holdout family, saturated quotas (12 variables, 3 values, 2 allowed
  per variable, every value capped at 4 = n/3, so no single-variable change is ever feasible): seed 20261005 sampler-forced
  20/20 refused with cycles off, 16/20 whole (0 false) with cycles on (diagnostic); validated on the untouched seed 20261006
  at the new defaults: all 11 families 20/20 whole, 0 false, 0 wrong, 0 refused (control `--cycles off`: saturated quotas
  20/20 refused, every other family unchanged); frozen corpus 0 / 0 / 0. Cost at defaults (median sweeps in 200 ms, N = 5
  alternating, Apple M4, load ~6.5): 300-task demo -13.7% (the only input of the four where the move runs); ferro12 / chain100 /
  asymmetric router -20.2% / +2.3% / +11.8% are load noise (identical code path, see above). Sampled answers on k > 2 capped
  programs change for the same seed; `--cycles off` reproduces the sampling before this move.
- IR cardinality (the first new construct): a `value` cap takes `min` (at least) alone or with `limit` (range; equal =
  exactly k), lowered to an extra at-most cap over the members' other values, so every tier runs it unchanged. `min` above the
  variables able to take the value -> exit 2 (`value`); `min` with `members` -> exit 2 (`schema`); a cap with neither `limit`
  nor `min` -> exit 2 (`schema`). v1 programs are unchanged (golden digests hold). Tests: brute force (6 x 3, three
  cardinality caps) + 3 contract cases.
- IR `all_different` and `implies`: lowered to caps (per value at most one of the listed variables; at most one
  of x = a and y outside `in`), so every tier, the sampler and the gate run them unchanged. Brute-force test (5 x 4) and 3
  contract cases; sampler at defaults 5/5 `diagnostics_passed`, max TV 0.0010.
- IR `tables`: forbidden / allowed tuples over 1-3 variables, lowered to caps (limit arity - 1 per forbidden
  tuple; an allow-list forbids the complement, at most 100,000 tuples). Brute-force test (4 x 3) + 3 contract cases.
- IR `precedes`: the (job, slot) precedence pattern, slot(after) >= slot(before) + gap with values read as
  ordered slots, lowered to pair caps (O(k^2) per precedence). Brute-force test (4 jobs x 5 slots) + 2 contract cases.
  Known failure (found during development): a feasible 20-job x 30-slot chain was refused (no feasible start; `pbit run` 15.2 s at defaults).
  Fixed before release (the "Arc-consistent start" entry below).
- `pbit run --deadline-ms N`: whole-call wall-clock target (exact tiers N/4, sampler 0.6 and polish 0.1 of the
  rest, gate in the reserve); `deadline: {ms, met}` in the answer; met 15/15 at N = 500 / 1000 / 2000 on the 20 x 30 chain.
- `pbit run` answers end with `phases`: parse / compile / exact / sample / gate / polish / total wall ms.
- IR `start` warm start: `{"var": "value"}` over every variable, validated (allowed, clamps, every cap and
  construct) or exit 2; chain 0 starts from it, the other chains search as before; exact tiers ignore it. `Model::start` in the
  library (ignored if infeasible). 9 contract cases + 2 tests.
- Arc-consistent start + bounded exact tiers (the known failure above): programs whose two-variable forbid caps prune under
  root arc consistency start with a MAC search (AC-3 + forward checking on full caps); others are unchanged. `pbit run
  --op decide` with a wall-clock budget stops the exact tiers before the sampler at `--budget-ms` when the raw space exceeds
  `--exact-limit` (new `telemetry.exact_budget_reached`), and the no-start fallback stops hard at the end of the sampling
  budget or half a budget after it starts, whichever is later. 20 x 30 precedence chain at defaults: 15.2 s -> 0.54 s, chains start, gate refuses honestly at 200 ms
  (`diagnostics_passed` at 3000 ms, max TV 0.0112 vs an independent chain DP). Behaviour change: a program over
  `--exact-limit` whose exact answer needs longer than `--budget-ms` is now sampled (use `--op exact` or a larger budget).
- IR `linear` rules (linear <=): `{"terms": [[var, value, weight], ...], "limit": L}` = sum of weight x [var = value]
  <= L, whole-number weights 0..=1,000,000 (knapsack, bin packing, multi-dimensional budgets). Lowered to one WEIGHTED cap
  (`Cap.weights`; `Cap::new` for unit caps; library users constructing `Cap { .. }` literals add `weights: vec![]`), never by
  replicating members. Every exact tier, start search (incl. MAC forward checking), sampler move, the compile pass and the gate
  read the weights; the frontier and occupancy-count DPs decline weighted programs. Unweighted programs: identical answers
  (every golden test unchanged). Tests: 300 random programs vs brute force (1e-9), sampler vs enumeration with every move on,
  a CLI brute-force test and 10 contract cases.
- Program families (BENCHMARKS 4.4-4.10, USE-CASES rows; stdlib bench scripts with independent oracles and a
  classical baseline each): knapsack (`bench/knapsack.py`, log-space DP; n = 20-80), bin packing (`bench/binpacking.py`, DP over
  bin-load vectors; n = 12-36 x 3 bins), Ising-MRF denoising (`bench/denoise.py`, transfer matrix + ICM; 8 x 12), tree inference
  (`bench/tree_infer.py`, sum-product; n = 200 / 1,000), budgeted selection with correlations (`bench/portfolio.py`, chain x budget
  DP; n = 30 / 60), 3-SAT counting (`bench/sat.py`, brute force + WalkSAT; n = 14 / 18: exact; the sampler refuses 16/16),
  agentic tool-call planner (`bench/agent_planner.py`, brute force; 6-7 steps), assignment with side constraints
  (`bench/assign_side.py`, brute force; n = 8 / 12). Totals over the sampled answers: 0 released items outside 0.05 of the
  oracle, 0 false `diagnostics_passed`. Examples `examples/knapsack-20.json`, `denoise-8x12.json`, `agent-plan-6.json` (tests
  `run_knapsack_example_matches_dp`, `run_ising_denoise_matches_brute_force`, `run_agent_plan_example_is_exact`). Not covered:
  MaxSAT (no soft ternary terms in v1), ILP baselines (no scipy on the measuring machine).
- Known failures found during development (BENCHMARKS / docs "Limits in v1"; neither gives a wrong answer): (1) dense n x
  k domains:
  k = 65,535 with 3 allowed values per variable, n = 200, `--op sample` peaks at 3,584 MB and takes 1.85 s at a 200 ms budget
  (the polish clones the model per temperature; gate 0.9-1.0 s; polish FIXED: `Chain::beta` carries the
  temperature, no clones, same plans bit for bit: n = 200 polish 549.7 -> 50.1 ms at its 50 ms budget, peak RSS 3,444-3,584 ->
  2,322 MB; sparse domains stay a v1 limit); (2) gate false refusal: on quota programs whose caps sum to n,
  members forced BY COUNTING trip the `frozen` rule (saturated-k3: 8/120 sampler answers over 6 seeds, chains fine);
  `#[ignore]`d red test `stress_saturated_forced_by_counting_is_not_frozen`. (2) FIXED (`pbit_ir::forced_values`:
  on partition programs a variable every chain held at one value is proved forced by the capacitated matching and treated as
  a constant): 8/120 -> 0/120 refused on the same seeds, 0 false; `--cycles off` control still refuses 20/20; untouched
  holdout 20261011 0 false / 0 wrong / 0 refused; the red test is un-ignored.
- `bench/calibrate.py` family `v2-constructs`: 7 variables x 4 values with an at-least cap, an all_different, an
  implication, a forbid table and a precedence; the brute-force oracle checks each construct by its direct meaning (not by
  pbit's cap lowering). Untouched seed 20261007 (5 programs x seeds 1..4): defaults 20/20 `exact`, sampler forced 20/20
  `diagnostics_passed`, 140 released, 0 false, 0 wrong, 0 refused.
- Never-moved rule sharpened: partition programs (the router; cap-free programs) now escalate per connected component
  of the free variables, like every other program, instead of every variable (`partition_programs_escalate_only_the_stuck_
  component`: two independent quota pools, one stuck: only that pool is escalated). Measured: `router_bench` family A 6/16
  passed, 1,390 -> 1,380 released (wall-clock noise; router programs are one component through their caps), 0 wrong; seed
  20261006 with every move off, old vs new binary: identical counts in all 11 families (0 false, 0 wrong); corpus 0 / 0 / 0.
  `bench/calibrate.py`: `PBIT_BIN` env var to score another binary.
- `bench/calibrate.py`: five harder holdout families from their own random stream (three ferro clusters with
  mixed-sign bridges, rare modes behind a cap, 3-value clusters, a 150-variable 4-value chain with a transfer-matrix oracle, an
  8-value alphabet with caps) and `--families`. Untouched seed 20261004, all 10 families + the frozen corpus, defaults and
  sampler forced: 0 false whole answers, 0 wrong released items, 0 refused (gate/3 unchanged). Moves-off control: the three
  cluster families refuse (20/20, 20/20, 7/20 not whole), still 0 false. Per-family table in BENCHMARKS "Known failure modes".
- `pbit run` (op decide): when the raw space exceeds `--exact-limit` the components tier now runs BEFORE the
  whole-program frontier DP (as `pbit decide` already did). 100,000 independent two-value variables: frontier 4,441.9 ms ->
  forest 32.5 ms (medians N = 5, Apple M4, load ~2.4; same log Z 73283.230992, identical odds; MAP differs only on exact-tie
  variables, same `plan_logw`); a 100,000-variable path is unchanged (forest ~34 ms). Tier names on such inputs move
  `frontier` -> `forest` / `components`. Test `run_components_tier_goes_before_the_frontier_over_the_limit`.

### Added (stress corpus)
- A frozen stress corpus (`pbit-cli/tests/stress/`, 7 inputs from the 2026-09-30 external review) with independent oracles
  (occupancy count DP,
  brute force, transfer matrix) and a "Known failure modes" section in BENCHMARKS: on 5 of 7 inputs 2-3 of 20 seeds pass the gate
  with every released item wrong. Those gate tests were `#[ignore]`d (red) until the collective moves and gate/3 (below).

### Changed (gate cost at many chains)
- The gate's chain-disagreement check uses sign projections from 128 chains (k <= 12) instead of every pair of chains,
  and its per-chain passes (both batch-means passes, the frozen check, the projections) run on a bounded pool of `--threads`
  workers instead of one OS thread per chain (`pbit_ir::pool_map`, `pbit_ir::GATE_THREADS`, which the CLI sets from
  `--threads`). Statistics are bit-identical (test `gate_pool_is_bit_identical`; old vs new binary same bytes at fixed
  `--sweeps`, 4 to 3,000 chains). `pbit demo --tasks 24 --seed 1 | pbit decide --mode sample --chains N`, two binaries
  interleaved, N = 5 medians, Apple M4, load 6.2-7.2: gate 1,000 chains 54.0 -> 15.9 ms, 10,000 chains 2,255.7 -> 15.8 ms
  (peak RSS 840 -> 368 MB), 100,000 chains: no answer in 90 s -> whole call 583 ms (gate 88.5 ms; the sampling phase now
  overruns a 200 ms budget there, BENCHMARKS §6).
- `pbit decide` lowers the router program for its components tier only under `--mode auto` with `--frontier-states` > 0,
  and the
  lowering stops when `--exact-ms` passes (`Problem::lower_until`, clock read per group member). One 3,000-task
  group: `--mode sample` 884.6 -> 618.3 ms, `--exact-ms 0` 885.6 -> 614.9 ms, `--exact-ms 50` 898.1 -> 741.0 ms (N = 5 medians).
  Outputs unchanged (byte-identical at fixed `--sweeps`).

### Changed (calibration on a frozen-threshold holdout)
- `bench/calibrate.py` (stdlib only): the frozen stress corpus and seeded holdout families (rare modes in weakly bridged
  ferro clusters, heterogeneous groups under a binding cap, 3-value constrained cycles, a 7-value alphabet, varying group
  sizes), each scored against a brute-force / count-DP / transfer-matrix oracle, at defaults and with the sampler forced;
  the header records commit, gate version, the gate thresholds, machine, load and every command. CONTRIBUTING: thresholds are
  frozen before a holdout and never retuned on it.
- Holdout seed 20261001 (gate/3 thresholds frozen at 54c9a16, `--op sample`, 5 programs x seeds 1..4 per family): rare-modes
  8 whole answers, ALL 8 false (96 wrong released items, 12 refused); group-sizes 12/20 refused; the other three families
  20/20, 0 false. The global flip maps (A,A) <-> (B,B) of two clusters but never reaches (A,B). Diagnostic on the same seed
  with `--cluster on`: rare-modes 20/20, 0 false; group-sizes 0 refused. **`--cluster` is now on by default** (`pbit run`
  and `pbit decide`); validated on the UNTOUCHED holdout seed 20261002 at the new defaults: every family 20/20, 0 false, 0
  wrong, 0 refused (control, same seed with `--cluster off`: rare-modes 9/9 false, 108 wrong; group-sizes 8 refused). The
  frozen corpus at the new defaults: 0 false, 0 wrong, 0 refused in 14 family-modes (ferro12-rc-w1.0 sampler had 1/20
  refused before). Cost (earlier fixed-work numbers, unchanged move): +48.4% per sweep on the 32-task one-group router,
  +24.6% on ferro12, -0.4% on the 300-task demo, +1.3% on chain100; at defaults the budget is wall-clock, so it shows as fewer
  sweeps: median sweeps in 200 ms (sampler forced, N = 5 alternating on/off, Apple M4, load ~3): asymmetric-router32 h0.06
  -20.4%, ferro12 -2.4%, 300-task demo -2.1%, chain100 within noise (+14.2%; the off runs spread 118k-190k). Sampled answers
  change for the same seed (the move consumes draws); `--cluster off` reproduces the sampling before this move.

### Changed (gate/3)
- `gate.version` is "gate/3": every released item also needs its own indicator split-R-hat (`Gate::rhat_task`, computed since
  0.1.0 but unused) below `ITEM_RHAT_MAX` = 1.05, and a whole answer needs every variable's; new `release_reason` `item_rhat`,
  new gate fields `item_rhat_max` (finite max) and `item_rhat_infinite` (count). It catches chains that hold different values
  of a variable while their log-weight traces agree. Measured: 0 change on `maxcut_oracle` (144 runs, deterministic) and on
  1,000 iid bits (150 runs); it does not change the moves-off stress counts (every chain in the same wrong mode is invisible to
  any within-run statistic: BENCHMARKS "Known failure modes"). New example `iid_gate` (review case E6) and a `COLLECTIVE=1` knob on
  the pbit-ir oracle examples (the libraries still default the moves off; the CLI defaults them on).
- The frozen rule's "a free variable that never changed value in any chain" test now also runs on partition programs (the
  router, and every program without capacity constraints, which counts as a partition: max-cut, ferro12; it returned before
  reaching it): such a run is refused whole, as on other programs. Moves-off ferro12 at fixed work: 3/20 false -> 0/20 (20 refused). Conservative by design (the gate
  cannot tell "certain" from "stuck"). Measured on `router_bench` (wall clock, collective off): family A 7 -> 6 of 16 runs
  passed, 1,565 -> 1,370 released (one correct 200-task run, maxTV 0.0178, refused), 0 wrong before and after; families B and C
  unchanged; the 300 / 1,000 / 3,000-task demos at CLI defaults are unaffected (frozen 0; 5/5, 3/3 passed, 3/3 partial with
  every escalation `item_bound`). Two golden gate digests re-pinned (their `frozen` counts now include the never-moved tasks).
- Occupancy-count R-hat (`Gate::rhat_occ`, `pbit_ir::rhat_occupancy`): per capacity constraint, split-R-hat of its load trace
  across chains; a variable is released only if every constraint it belongs to reads < 1.05 (reason `item_rhat`); gate fields
  `occupancy_rhat_max` / `occupancy_rhat_infinite`. It sees chains that hold different counts on a worker while each member's
  indicator still mixes (16 variables, 7 vs 9 ones per row on two chains: indicator R-hat < 1, occupancy R-hat infinite; test
  `occupancy_rhat_sees_count_modes_the_indicators_miss`). Measured on the night suites (same harness as above): no change beyond
  the never-moved rule (scheduling, which has caps, 428 -> 428; router_bench 1,375 family-A released incl. the c2 refusal; 0 false).
- `gate.mode_transitions` (global_flips, label_swaps, chains_without): accepted collective moves per chain, counted in both
  samplers without consuming draws (`Samples::moves`, `Chain::moves`; the router and IR samplers report identical counts). It
  is a reading, not a release rule: on ferro12 the flip is accepted every sweep (h = 0); on the 300-task demo (no two-value
  groups) 0 moves are accepted and the answer still passes; `--collective off` on the h0.06 stress input reports 0 transitions
  next to its known false whole answer (`--sweeps 4000 --seed 5 --polish-ms 0`: every chain at P(A) ~0.997 vs the true 0.1302,
  item_rhat_max 1.0003, chains_without 4), which is the case the reading exists for.

### Fixed (router enumeration on near-saturated groups)
- Default `pbit decide` on a near-saturated 3,000-task group spent 199.9 s in the router enumeration before declining: every
  search node summed the affinity over all group mates. The enumeration now keeps per-(group, worker) counts of assigned mates and
  a memo of the same sequential affinity fold per (task, worker): O(workers) per node, bit-identical sums (golden digests
  unchanged, new test `enumeration_affinity_memo_is_bit_identical`, old vs new binary same answers). Default decide, N = 5
  medians, Apple M4: 3,000 tasks at 91% fill 199.9 s (N = 1) -> 2.70 s; 1,000 at 91% 19.63 -> 0.73 s; 1,000 at 77% 2.56 ->
  0.39 s; 500 at 63% 1.14 -> 0.30 s (BENCHMARKS §6). The node budget itself is unchanged (64 x `--exact-limit`). Cost, found
  by an old-vs-new A/B: small-group exact inputs are 5-22% slower (default decide, N = 9, load 4.8: 12-task demo
  7.17 -> 8.01 ms, 24-task 26.5 -> 28.0 ms; full enumeration `--mode exact`, N = 5, load 3.5: 14-task demo 1,455 -> 1,773 ms;
  same answers); the sampled path is unchanged.
- `pbit decide` sampling at many chains: the wall-clock budget is now a deadline per worker (the r-th chain of a worker
  stops at
  call start + (r + 1) x budget / ceil(chains / threads)) instead of a slice each chain timed for itself, so a chain's overrun
  is charged to the next ones. 100,000 chains on a 200 ms budget: sampling 354-432 -> 269-318 ms, call 498-572 -> 405-460 ms
  (medians of N = 5 in two interleaved runs, loads 3.5-4.2); at that setting no chain sweeps (`sweeps` 0: the starts are the answer, refused);
  4 chains unchanged. Same chains and output shape; late chains sweep less. The rest of the overrun is the per-chain build.
  The IR sampler (`pbit run`) is unchanged.

### Fixed (numeric validity)
- Scores / weights must be finite with |x| <= 1e9 (`limit` error); the printer no longer emits a literal `inf` for |x| >
  ~1.8e302
  (|x| >= 1e15 prints in exponent form); a non-finite computed number never ships with an answer (`numeric` error, exit 3;
  infinite gate diagnostics only on a refusal, listed in `gate.non_finite`); telemetry rates no longer divide by zero.
- The frontier DP (`tier: "frontier"`) now runs in log space: its per-layer-scaled linear weights underflowed past ~745 nats
  and returned NaN marginals labelled `exact` (weights of +-1000 suffice), which aborted the CLI (exit 134). Printed odds and
  log Z unchanged on the demos; +9-12% wall on the largest DP layers.
- Seeded 10,000-document fuzz test of both front-ends (CLI test `fuzz_inputs_never_crash_and_outputs_stay_finite_json`).

## 0.1.0 - 2026-09-30

Initial public release.

- `pbit-core`: a Philox4x32-10 counter RNG (reproducible, stream-splittable) and two p-bit
  kernels, a bit-sliced multispin kernel (64 replicas per `u64`) and an SoA `f32` heat-bath.
- `pbit-ir`: the instruction set of the virtual processor. Categorical variables with
  allowed / forbidden / forced values, unary weights, pairwise couplings and at-most-k caps;
  three tiers: exact (enumeration, a frontier dynamic program, minimum-remaining-values search),
  a constraint-preserving Gibbs sampler, and the certification gate.
- `pbit-decide`: the assignment-router front-end (tasks to workers under quotas, allowed sets,
  clamps and group affinity), lowered bit-identically onto `pbit-ir`; anytime mode; annealed
  plan polish; exact-oracle instance families for calibration.
- `pbit` CLI: `demo`, `decide`, `run`, `ir`, `stats` and `version`. JSON in, JSON out. Exit codes
  0 plan, 1 infeasible, 2 bad input, 3 refused. Processor controls from flags, `PBIT_*`
  environment variables or `pbit.json`: threads, chains, CPU limit, memory limit, priority,
  progress, fixed-work sweeps, exact-tier time cap, frontier cap, wall-clock budgets.
- Zero external crates: a fresh checkout builds offline with stable Rust.
