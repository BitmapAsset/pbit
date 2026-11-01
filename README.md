<p align="center">
  <img src="docs/probbit-hero-1600x900.jpg" alt="probbit" width="100%">
</p>

<h1 align="center">probbit</h1>

<p align="center">
  <b>A personality, mood and memory layer for AI agents, on an exact p-bit decision engine.</b><br>
  An AI persona that lives outside the model: every turn, simulated p-bits on a CPU settle the agent's stance under its
  habits, give the exact odds, and log a replayable, verifiable record. The same engine plans under hard rules and
  refuses when it can't vouch. CLI, MCP server, OpenClaw plugin, npm, Python, wasm. Zero-dependency Rust.
</p>

<p align="center">
  <a href="https://github.com/BitmapAsset/probbit/actions/workflows/ci.yml"><img src="https://github.com/BitmapAsset/probbit/actions/workflows/ci.yml/badge.svg" alt="ci"></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-Apache--2.0-blue.svg" alt="license"></a>
</p>

<p align="center">
  <a href="https://bitmapasset.github.io/probbit/">bitmapasset.github.io/probbit</a> ·
  <a href="AGENTS.md">AGENTS.md</a> ·
  <a href="llms.txt">llms.txt</a> ·
  <a href="https://www.npmjs.com/package/probbit">npm</a> ·
  <a href="https://github.com/BitmapAsset/probbit/releases">releases</a>
</p>

| **For AI agents and their builders** | **For decisions under rules** |
|:---|:---|
| **Personality**: a persona file + a seed = one individual, the same whichever LLM writes the words. **Mood**: moods with inertia and half-lives (an affective-computing model, computed exactly). **Memory**: learned adjustments inside caps and a hash-chained strand of every turn. **Receipts**: `probbit live verify` replays the strand byte for byte. | **Plans**: tasks to workers, tickets to agents, jobs to machines, every hard rule kept by construction. **Odds**: exact for every part when the program allows, otherwise sampled with an error bar. **Refusal**: "I can't vouch for this" (exit 3) instead of a guess. A constraint solver built on probabilistic computing (p-bits, Ising/QUBO), simulated on a CPU. |

```sh
curl -fsSL https://raw.githubusercontent.com/BitmapAsset/probbit/main/install.sh | sh
```
Linux and macOS, no sudo, the archive's SHA-256 checked first. Windows, npm and the from-source build: [Install](#install).
One binary of 3 to 4 MB, zero third-party dependencies, nothing phones home. Agents: start at
[For AI agents reading this](#for-ai-agents-reading-this), [AGENTS.md](AGENTS.md) and [llms.txt](llms.txt).

| **60 PII leaks → 0** | **180,540 plans, counted exactly** | **refused → partial → passed** |
|:---|:---|:---|
| 300 agent tasks, six workers, privacy rules and quotas. A rule-ignoring router sends 60 private tasks to cloud models (133 broken rules on `probbit demo`'s queue, 144 on the `agent_router` example's). Every plan probbit returns breaks 0: the sampler never leaves the rule-abiding set. | 12 tasks × 6 workers: every rule-abiding plan enumerated, exact odds for every task, in 7.3-8.2 ms on an Apple M4 ([measured](#what-it-does-measured-apple-m4-one-machine)). | Too little time and it says "I can't vouch for this" (exit 3) instead of guessing. More time: it releases what passed its own error bar and escalates the rest ([watch it refuse](#60-second-tour)). |

Also inside: **the individuality layer**, `probbit persona`, a temperament that lives outside the model:
[swap the model, keep the individual](#the-individuality-layer-probbit-persona).

<p align="center"><img src="docs/art/the-spine-beside-the-model.jpg" alt="a vast glowing cloud (the model) connected by one thin line to a small cyan-lit processor (probbit)" width="100%"></p>
<p align="center"><sub><i>The spine beside the model: the model writes, probbit decides what the stance is and what the rules allow. 0.3 ms, beside the model.</i></sub></p>

## For AI agents reading this

Run these in order in a POSIX shell (Linux or macOS), in an empty directory. No clone, no Rust, no sudo, no API key; about
one minute on a normal connection. Each step says what to expect; any other output or exit code means stop and
[report it](https://github.com/BitmapAsset/probbit/issues). Windows: install with `install.ps1` ([Install](#install)); these
steps are written for and tested on Linux and macOS.

```sh
mkdir probbit-try && cd probbit-try

# 1. Install to ~/.local/bin (PROBBIT_INSTALL_DIR to change it). Exit 0; ends with a "Next:" list. If it says the
#    directory is not on your PATH, that is expected: step 2 adds it for this shell.
curl -fsSL https://raw.githubusercontent.com/BitmapAsset/probbit/main/install.sh | sh

# 2. Put that directory on PATH for this shell. No output.
export PATH="$HOME/.local/bin:$PATH"

# 3. Expect "probbit 0.14.0" (or later). Exit 0.
probbit --version

# 4. Fetch an example persona (a tutor called Pip). No output; tutor.yaml appears.
curl -fsSLO https://raw.githubusercontent.com/BitmapAsset/probbit/main/examples/persona/tutor.yaml

# 5. Make an individual: persona + seed 2. Expect "wrote tutor.state (Pip 1.1.0, seed 2)". Exit 0.
probbit persona init tutor.yaml --seed 2 --out tutor.state

# 6. One live event (an upset user), logged to a strand. Expect one line starting
#    "line":"Stance: go easy on the jokes; no jokes; gentle; ... Exit 0.
echo '{"sentiment":"negative"}' | probbit live tutor.yaml --state tutor.state --strand tutor.strand --clock fixed 2>/dev/null | grep -o '"line":"[^"]*"'

# 7. Replay the strand and check every digest. Expect "ok":true. Exit 0. (Without the grep: a strand that does not
#    replay exits 1 and names the first line that differs.)
probbit live verify tutor.strand | grep -o '"ok":[a-z]*'

# 8. List the MCP server's tools. Expect 10 lines, from "name":"probbit_decide" to "name":"probbit_live_verify". Exit 0.
printf '%s\n' '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"sh","version":"0"}}}' '{"jsonrpc":"2.0","method":"notifications/initialized"}' '{"jsonrpc":"2.0","id":2,"method":"tools/list"}' | probbit mcp 2>/dev/null | grep -o '"name":"probbit_[a-z_]*"'
```

Then hand it to your harness: `claude mcp add probbit -- probbit mcp` (Claude Code; Codex, Cursor and other MCP clients in
[docs/agents.md](docs/agents.md)). OpenClaw: `openclaw plugins install npm:probbit-stance --accept-capabilities` (this
changes that OpenClaw's configuration; set-up and checks in [plugins/openclaw](plugins/openclaw/README.md)). Put the
stance `line` into your model's prompt each turn; the model writes, the persona decides how. What to read next:
[AGENTS.md](AGENTS.md) (these commands, plus how to verify a strand and report a bug), [docs/persona.md](docs/persona.md)
(the persona format), [docs/CHEATSHEET.md](docs/CHEATSHEET.md) (every command and exit code).

## Use it from your harness

| surface | how | one command that proves it works |
|---|---|---|
| CLI | the install line above, `npm i -g probbit`, or `install.ps1` | `probbit demo --tasks 12 \| probbit decide --summary` → `"verdict":"exact"`, `"violations":0` |
| MCP server (Claude Code, Codex, Cursor, mcporter, any MCP client) | `claude mcp add probbit -- probbit mcp` ([docs/agents.md](docs/agents.md)) | step 8 above: 10 tools, `probbit_decide` … `probbit_live_verify` |
| OpenClaw plugin | `openclaw plugins install npm:probbit-stance --accept-capabilities` | `openclaw plugins inspect probbit-stance --runtime --json` lists both hooks ([plugins/openclaw](plugins/openclaw/README.md)) |
| npm | `npm i -g probbit` (Node 18+; downloads the prebuilt binary, SHA-256 checked) | `probbit --version` → `probbit 0.14.0` |
| Python | `curl -fsSLO https://raw.githubusercontent.com/BitmapAsset/probbit/main/python/probbit.py`: a stdlib-only wrapper around the CLI (decide, run, evaluate, persona, live; typed errors); not on PyPI | `python3 -c 'import probbit; s=probbit.persona_init("tutor.yaml", seed=2); print(probbit.persona_turn("tutor.yaml", s, {"sentiment":"negative"})["stance"]["line"])'` → `Stance: go easy on the jokes; …` |
| Agent SDK adapters | Claude Code hooks, OpenAI Agents SDK, LangChain, Vercel AI SDK ([docs/adapters.md](docs/adapters.md)) | in a clone: `python3 python/test_host.py` (Python hosts) or `node --test examples/node/stance-host.test.mjs` |
| Browser (wasm) | in a clone, with Rust and `rustup target add wasm32-unknown-unknown`: `playground/build.sh` | `ls -l playground/probbit.wasm`, then open `playground/index.html` |
| Rust library | not on crates.io yet: `cargo add --git https://github.com/BitmapAsset/probbit probbit-decide` | `cargo test -p probbit-decide` in a clone ([Library API](#library-api-rust)) |
| `no_std` / microcontrollers | the library crate `probbit-persona` with the feature `nostd` (`no_std` + `alloc`), from git like the Rust library ([docs/embedding.md](docs/embedding.md)) | in a clone, with the esp toolchain: `cargo +esp check -p probbit-persona --no-default-features --features nostd --target xtensa-esp32s3-none-elf -Zbuild-std=core,alloc` |

## FAQ

- **What is a p-bit?** A probabilistic bit: 0 or 1 with a probability you set. Coupled p-bits sample whole configurations
  together (an Ising/QUBO model). probbit simulates them on a CPU, exactly when the program is small or thin enough,
  otherwise by Gibbs sampling with an error bar. It is classical: see [Hard questions](#hard-questions).
- **How do I give an AI agent a consistent personality?** Write a persona file (traits, moods, habits that are hard rules),
  pick a seed, and put the stance `line` of every turn into the prompt. The individual is a pure function of (persona,
  seed, the inputs so far, engine version), so the character stays consistent across sessions and prompts.
  [The individuality layer](#the-individuality-layer-probbit-persona), [docs/persona.md](docs/persona.md).
- **Can I swap the LLM?** Yes. The persona lives outside the model; the same individual answers whatever model the host
  calls. Whether a given model writes in the stance it is given has to be measured per model.
- **Is it deterministic and replayable?** Yes. A turn is a pure function of its inputs; a strand records every event with
  chained sha256 digests, and `probbit live verify` replays it and names the first line that differs.
- **What does "refuses" mean?** When the engine cannot vouch for an answer within its budget it says so (verdict `refused`,
  exit 3) instead of guessing; `partial` releases the parts that passed and escalates the rest.
- **Is this a memory store?** No. Its memory is the individual's state (learned adjustments within caps, moods) and the
  replayable strand, not a store of facts. Pair it with a fact memory or RAG if you need recall.
- **Does it run offline? What does a turn cost?** Fully offline: no network, no API key, nothing phones home. A persona
  turn takes about 0.2-0.3 ms median on an Apple M4 (measured, [below](#the-individuality-layer-probbit-persona)); the
  stance line is about 40 tokens of your model's prompt.
- **What hardware? Which agent framework?** Any CPU: release binaries for Linux x86_64 and arm64, macOS (Apple silicon and
  Intel) and Windows x86_64; wasm in a browser. It sits beside any agent framework through the CLI, MCP, Python or the
  adapters above. All timings in this README come from one Apple M4.
- **Is probbit the same as probit?** No. probbit (two b's, short for *probabilistic bit*) is this project. It is unrelated to
  the probit function or probit regression in statistics, to Power BI template files (`.pbit`), to the Python p-bit circuit
  simulators (p-bit, p-kit), and to other products or companies that share the name.

## What's new in 0.14.0

- **The persona layer as a library crate, for std and `no_std` hosts.** `probbit-persona` holds the document layer and the
  strand writer (`Live`, the line writers, `Replay`, `verify*`, no file I/O), with the features `std` (default) and `nostd`
  (`no_std` + `alloc`, checked on `xtensa-esp32s3-none-elf`); the CLI uses it unchanged and every CLI output of the move is
  byte for byte as 0.13.0. Setup, the turn and resume return boxed values, so one event's stack high-water on the host proxy
  fell from 31,824 to 13,808 bytes (setup: 39,056 to 12,112); a strand resumes from its last checkpoint or from a state
  record behind it, and seals into segments ([docs/embedding.md](docs/embedding.md) "Stack budget", "Sizing a strand on
  flash", "Resume on a body").
- **The expectation cache.** The expectation keeps an exact cache and solves the event's own program once; every stance,
  strand line and digest is the same with the cache on or off (a life of one repeated event, host proxy: `expect` 52 % → 37 %
  of the turn, 95 % of its programs from the cache; [docs/embedding.md](docs/embedding.md) "The expectation's cache" and
  "The expectation's members").
- **The self layer, off by default** ([RFC 0013](docs/rfcs/0013-self-report.md), [0014](docs/rfcs/0014-integrity.md),
  [0015](docs/rfcs/0015-consolidation.md), [0016](docs/rfcs/0016-curiosity.md)): `probbit reflect` and reflection lines (a
  canonical account of a window of events from the lines alone, no solve), integrity (per trait the drift from the declared
  self against RFC 0004's allowance; the monitor's dial and the Mirror tab), the rest pass (`live control STRAND rest`, one
  `consolidation` line) and curiosity (a drive fed by the measured surprise, with an optional stir). Each part is opt-in and a
  deterministic function of the logged lines that `probbit live verify` recomputes. A declared, replayable self; it does not
  feel. A persona that declares none of its keys gets every document byte for byte as before (P0: 653 outputs, 0 differ).
- **An optional tier, off by default: a simulated amplitude register, exact, on a CPU** (`engine: {tier: amplitude}` the
  opt-in; [RFC 0011](docs/rfc/0011-amplitude-register.md), [the demo](examples/persona/demo/)): at one event of the demo
  tutor the thermal odds of the top two humour levels sit 0.007412 apart and the register's long sweep separates them by
  0.494808; a register strand recorded on a 0.14.0-dev build before the tier's last round may not verify on 0.14.0.
- Engine gate: strands with the new line kinds or keys read as unsupported (exit 5) on 0.13.x. The release has 15 assets:
  six archives, `probbit-Monitor-v0.14.0-macos.zip`, a `.sha256` for each, and `probbit.1`.
  [Release notes](https://github.com/BitmapAsset/probbit/releases/tag/v0.14.0) · [CHANGELOG](CHANGELOG.md)

## What's new in 0.12.0 to 0.13.0

- **The engine's own expectation, and the exact surprise of every event** (RFC 0009, engine side; opt-in `expectation`
  block). Per live event the engine compiles, beside the turn's program, its own expectation of that turn (a decayed
  mixture over the input combinations the life has seen) and reports the exact divergence between them: `deviation`
  on the stance, a receipt on the strand line that `live verify` recomputes, a `DEVIATION` row in `monitor`. Measured
  (BENCHMARKS.md §15, bars registered before the run): fidelity to the reference enumerator 1e-13; the deviation is
  non-zero on 100 % of post-cold-start events on the two recorded lives and 99.2 % of 64,000 synthetic ones; graded 17×
  to 1,859× between the 90th and 10th percentile per persona; a per-event upper bound holds on every event checked.
  **Not shown**: habituation. On 200 scripted repeats the reading fell on 76.5 % of scripts and dishabituated on 35 %,
  against bars of 90 %, and a matched-index control could not separate the decay from the mixture simply growing; the
  claim is not made. Also failed as registered: the cost bar (p95 3.98 ms extra per event against 2 ms, on a loaded
  machine) and the 60-byte receipt (62 to 66 bytes). Effects {}: the deviation moves nothing in this release.
- **The trajectory, exactly.** `engine.trajectory` solves the turn's program at K inverse temperatures through the exact
  tier (no sampler): the "forming, then settled" picture, deterministic. Joint entropy non-increasing on every program
  checked; per-variable one-rise on 99.61 % of 300 held-out programs (the draft's sampler: 93.1 %). `probbit persona
  trajectory`, a `TRAJECTORY` monitor row.
- **Measured against prompt injection** (BENCHMARKS.md §14, `bench/agentdojo/`): one pre-registered AgentDojo run,
  gpt-4o-mini, 97 user tasks and 629 attack cases, three paired arms. The stance block lowered targeted attack success
  from **0.289 to 0.227** (−6.2 points, 95 % CI [−8.7, −3.8], McNemar p = 1.8e-6) with no measured utility cost. The
  same words as a static prompt lower it almost as much but cost **10.3 points** of benign utility; the sense-gated
  block did not. A registered control run (E3) replaced the engine with a plain if-statement that inserts the same words
  when the same regex fires: it matched the block on attack success (0.242 vs 0.227, difference CI [−3.7, +0.8] points)
  and on both utilities. The gain on this suite is the gating, *when* the words reach the model; what the engine adds
  beyond an if-statement here is a replayable strand for every stance. The baseline arm matches the suite's published
  row (utility 0.6804 = 0.6804). Not tested: adaptive attacks that avoid the sense's markers.
- **V2's residual explained** (BENCHMARKS.md §13): the tonic-off valence tracking of +0.1199 that failed 0.11.0's V2 is
  carried by the phasic term of the same need-valence block (phasic off as well: −0.267; everything off: 0.0000). The
  published V2 stays FAIL; the proposed 0.12 registration ablates the whole `needs.valence` block.
- Every new key is opt-in: a persona without them gives every document byte for byte as 0.11.0 (64,000 synthetic lines,
  both recorded lives; the P0 corpus, 653 recorded outputs, 0 differ).
  [Release notes](https://github.com/BitmapAsset/probbit/releases/tag/v0.12.0) · [RFC 0009](docs/rfcs/0009-surprise-live-state.md) · [BENCHMARKS §14](BENCHMARKS.md) · [CHANGELOG](CHANGELOG.md)
- **0.12.1, the patch around the engine** ([release notes](https://github.com/BitmapAsset/probbit/releases/tag/v0.12.1)):
  the AgentDojo run's host as a module with bindings for Claude Code hooks, the OpenAI Agents SDK, LangChain and the
  Vercel AI SDK ([docs/adapters.md](docs/adapters.md)); one receipt per decision from any strand
  ([docs/receipts.md](docs/receipts.md)); **[REPRODUCE.md](REPRODUCE.md)**, three tiers (five minutes, thirty seconds of
  compute, or the full $3 rerun) with the sha256s to compare, and a CI job that recomputes the §14 tables on every push;
  the OpenClaw plugin [`probbit-stance`](plugins/openclaw/README.md) on npm; the E3 control above; `live verify` names
  the engine that replayed the strand (`verified_by`).
- **0.13.0, the face and the app** ([release notes](https://github.com/BitmapAsset/probbit/releases/tag/v0.13.0)): the
  monitor page gains a face header (the stance's glyph, two mood meters with every level's odds, a "why this glyph" panel,
  the settle strip over the exact trajectory); an opt-in `display.glyphs` persona block maps each stance to one emoji by a
  fixed rule order ([docs/persona.md](docs/persona.md) §2.18; a persona without it gives every document byte for byte as
  before, P0: 653 outputs, 0 differ); **probbit Monitor**, the page in its own Mac window around a bundled probbit
  ([docs/monitor-app.md](docs/monitor-app.md)); identity labels in a sidecar beside each strand and one monitor for many
  strands (`--dir`, a switcher rail); the OpenClaw plugin `probbit-stance` 0.2.0 (the glyph line, the senses feed shadow
  first, the labels; it needs probbit 0.13.0); the rerun scripts print `MATCH` or `DIFFERS` per table, and the lab tables
  behind Result 2 are reproduction kits ([REPRODUCE.md](REPRODUCE.md), four tiers). The release has 15 assets: six
  archives, `probbit-Monitor-v0.13.0-macos.zip` (universal), a `.sha256` for each, and `probbit.1`.

0.11.0 added the certified persuadability bound and viability in the engine ([release notes](https://github.com/BitmapAsset/probbit/releases/tag/v0.11.0)); 0.10.0 added the body: needs, valence, surprise, pace and dispositions ([release notes](https://github.com/BitmapAsset/probbit/releases/tag/v0.10.0)).

## probbit in two minutes

probbit is a p-bit processor: it couples bits that hold a probability on the CPU you already own, settles a whole plan under
hard rules, gives the odds for every part (exact when the program allows) and refuses when it can't vouch for the answer.
`probbit persona` adds an individual (a persona file + a seed): every turn, a stance line for the model's prompt and habits the
stance never breaks; `fuzz` and `prove` test them.

```sh
curl -fsSL https://raw.githubusercontent.com/BitmapAsset/probbit/main/install.sh | sh   # one binary, SHA-256 checked, no sudo
probbit demo --tasks 12 | probbit decide --pretty     # 12 tasks, 6 workers: "verdict": "exact", 0 violations
# the persona examples live in a clone: git clone https://github.com/BitmapAsset/probbit && cd probbit
probbit persona init examples/persona/tutor.yaml --seed 2 --out tutor.state             # an individual: the tutor, seed 2
probbit persona turn examples/persona/tutor.yaml --state tutor.state --inputs '{"sentiment":"negative"}'   # its stance line
probbit persona prove examples/persona/tutor.yaml --seeds 0-99 --never '{when: {sentiment: negative}, then: {humour: {at_most: light}}}'   # held by construction
probbit live examples/persona/tutor.yaml --seed 2 --demo week --plain | tail -4   # a week on the clock: learning inside caps, 0 rule breaks, replayable
```
What now? The [60-second tour](#60-second-tour), [docs/persona.md](docs/persona.md), [docs/agents.md](docs/agents.md) and
[docs/CHEATSHEET.md](docs/CHEATSHEET.md) (verdicts, tiers, `bound`, exit codes, every command on one page).

## Try it in 30 seconds

```sh
probbit                                                         # at a terminal: the spec line of this machine and three commands to try
probbit demo                                                    # at a terminal: 300 agent tasks routed live (~4 s): a field of p-bits, the gate, the verdict
probbit demo --tasks 300 | probbit decide --summary --pretty    # the same queue as one JSON answer: verdict, 0 violations, the least certain tasks
probbit demo --tasks 300 --hard | probbit decide --budget-ms 50 # too little time: "refused", exit 3, nothing released
probbit demo | probbit decide --top > answer.json               # 12 tasks, the full answer, with a live monitor while it runs (tier, updates/s, gate)
probbit evaluate --summary --pretty < examples/evaluate/support-12.json   # in a clone: after a decision model, its answers + your rules
probbit run --sweeps 2000 --polish-ms 0 --seed 7 --summary < examples/rabbit-field.json   # in a clone: the white rabbit, 3,840 p-bits: partial, 31 cells escalated
claude mcp add probbit -- probbit mcp                           # hand the processor to an agent: probbit_decide, probbit_run, probbit_evaluate, ...
```
Visuals go to stderr, only at a terminal; `NO_COLOR`, `--plain` or `PROBBIT_THEME=plain` turn them off. Piped, every command
writes exactly one JSON document, the same bytes as 0.2.1 apart from the name, the version and the timings. Piped, `probbit demo`
writes a 12-task queue unless `--tasks` says otherwise.

`probbit` is a **software processor built from p-bits**: bits that are 1 with a probability you set, coupled so that they
sample whole configurations together. You give it a *program* (variables, their allowed values, scores, pairwise
couplings, hard caps) and it returns three things:

1. an answer that **obeys every hard rule by construction** (the sampler never leaves the feasible set; if it cannot find
   a feasible start within the budget it returns `refused` and no plan);
2. **odds for every variable**, exact when the program is small or thin enough, otherwise sampled with an error bound;
3. a **verdict**: `exact` (proof-grade under the score model), `diagnostics_passed` (sampled: every released item passed
   the versioned diagnostics gate; not a proof), `partial` (act on the released variables, escalate the rest), `refused` or
   `infeasible` (a proof).

The first front-end is a task router. Give it a queue of things to assign (tasks to workers, tickets to agents, jobs to
machines), the score of every option, the rules that must hold (allowed sets, capacities, forced choices) and how much
items that belong together should stay together. It samples **whole plans** on a p-bit substrate, so every plan it returns
obeys every rule, and it returns **odds per item** (exact, or Monte-Carlo estimates of the marginals of the score model you
supplied: accuracy under that model, not predictive calibration) with a **diagnostics gate** that releases the items whose
diagnostics pass and escalates the rest. The gate can be fooled: when every chain misses the same mode it releases wrong
odds with a tiny error bar (see [Known failure modes](BENCHMARKS.md#known-failure-modes)).

A p-bit is a bit that is 0 or 1 with a tunable probability; coupled p-bits sample joint configurations. probbit is
quantum-inspired and entirely classical (see "Is this quantum?" below): it runs on the CPU you already have, needs no
network and has zero external crates. The same problem lowers to the QUBO/Ising form a p-bit or annealer chip takes (`probbit-ir`), which is the hardware
hand-off for later.

```
$ cargo run --release -q -p probbit-cli -- demo --tasks 300 | cargo run --release -q -p probbit-cli -- decide --budget-ms 200
{"engine":"probbit 0.2.0","tasks":300,"workers":6,"affinity":1.2,"verdict":"diagnostics_passed","plan":{"T000":"human",…},
 "violations":0,"odds":{"T000":{"human":0.78,"local-gemma":0.22},…},"released":[…300…],"escalated":[],
 "gate":{"rhat":1.00015,"tv_bound":0.023,"tv_tol":0.05,"frozen_saturated_workers":0,…},"ms":291.6}
```

**`diagnostics_passed` has a definition, and it is not a proof.** (It replaces the 0.1.0 verdict `certified`.) The answer
passed gate/3: a 3-sigma batch-means Monte-Carlo bound (at most 0.05 total variation per released item), split R-hat of the
log-weight trace and of every released variable's value indicators, batch-size stability and frozen-resource checks, tuned against exact oracles on small synthetic families.
Every sampled answer reports `gate.version`, `gate.assumptions`, both MCSE estimates, the worst item's per-chain means and a
`release_reason` per item. These checks cannot see a mode that no chain visits: on the frozen stress corpus
(`probbit-cli/tests/stress/`) 2-3 of 20 seeds per family pass with every released item wrong (BENCHMARKS "Known failure
modes"). The tables are in [BENCHMARKS.md](BENCHMARKS.md).

## Install

One line, no sudo, the archive's SHA-256 checked before anything is installed. The current release is v0.14.0.

| how | command |
|---|---|
| Linux, macOS (no sudo) | `curl -fsSL https://raw.githubusercontent.com/BitmapAsset/probbit/main/install.sh \| sh` |
| Windows PowerShell | `irm https://raw.githubusercontent.com/BitmapAsset/probbit/main/install.ps1 \| iex` |
| npm (Node 18+) | `npm i -g probbit` |
| by hand | archives for Linux x86_64 (glibc or static musl), Linux arm64 (static), macOS (Apple silicon and Intel) and Windows x86_64 on the [Releases page](https://github.com/BitmapAsset/probbit/releases), each with a `.sha256` beside it |

npm may print an `allow-scripts` warning: the package's only install script downloads the prebuilt binary for your platform
and checks its SHA-256. If npm blocks the script, the binary is fetched the first time you run `probbit` instead.

The shell installer defaults to `~/.local/bin`, without replacing a system-wide installation. Set
`PROBBIT_INSTALL_DIR` to choose another directory. Both the shell and npm installers select static musl binaries
for Linux ARM64 and for x86_64 musl systems such as Alpine. The shell and PowerShell installers check that a
downloaded candidate starts successfully before replacing an existing binary; failed checksums leave it unchanged.

From source, with Rust 1.78 or later: `cargo install --git https://github.com/BitmapAsset/probbit probbit-cli`, or clone and
`cargo build --release -p probbit-cli` (the binary lands in `target/release/probbit`). Nothing is downloaded after the clone:
there are no external crates.

At a terminal, `install.sh` ends with probbit's own hero screen. Platform notes and the installer tests: [PORTABILITY.md](PORTABILITY.md).

`install.sh` also installs the manual page (`man probbit`) to `~/.local/share/man/man1/`, or to `<prefix>/share/man/man1/` when
`PROBBIT_INSTALL_DIR` is `<prefix>/bin` (`PROBBIT_MAN_DIR` to choose; a failure there installs the binary all the same); every archive carries it as `docs/probbit.1`, and `probbit man` prints it.
Shell completions come from the binary itself:

| shell | command |
|---|---|
| bash | `probbit completions bash > ~/.local/share/bash-completion/completions/probbit` |
| zsh | `probbit completions zsh > ~/.zfunc/_probbit` (with `fpath=(~/.zfunc $fpath)` before `compinit` in `~/.zshrc`) |
| fish | `probbit completions fish > ~/.config/fish/completions/probbit.fish` |

`probbit --help` lists every command by group; `probbit help COMMAND [SUBCOMMAND]` (the same as `probbit COMMAND --help`)
gives a command's flags with their defaults, its exit codes and examples. A mistyped command gets a suggestion:
`probbit persna` -> `unknown command "persna"; did you mean "persona"?` (exit 2).

## 60-second tour

```
git clone https://github.com/BitmapAsset/probbit && cd probbit
cargo test --release --workspace                                   # unit, integration, regression and exact-oracle tests
cargo run --release -p probbit-cli -- demo --tasks 12 | cargo run --release -p probbit-cli -- decide --pretty
cargo run --release --example agent_router                         # the full narrated demo, ~3 s
```

The demo problem is an AI-agent task router: a queue of tasks and a set of workers (`opus`, `sonnet`, `luna-pro`,
`local-gemma`, `codex`, `human`) under hard policy (PII only on-prem or with a human; production-DB migrations never on
cheap models), per-hour quotas, and a bonus for keeping one customer's workflow on one worker. A rule-ignoring per-task argmax over
synthetic scores (a strawman baseline) produces **60 PII leaks** on 300 tasks, with **133 violations** on `probbit demo`'s queue and **144** on the `agent_router` example's. `probbit` produces **0 violations**,
per-task odds that match exact enumeration wherever exact enumeration is possible, and escalates the tasks whose odds it
cannot pin down in the time budget.

To watch it refuse when its diagnostics fail, make the queue hard (96 % full, strong affinity):
```
$ probbit demo --tasks 300 --hard | probbit decide --budget-ms 50     # "refused": 0 released, exit 3
$ probbit demo --tasks 300 --hard | probbit decide --budget-ms 200    # "partial": 87-95 of 300 released, the rest escalated
$ probbit demo --tasks 300 --hard | probbit decide --budget-ms 1000   # "diagnostics_passed": 300 released, 0 violations
```
(5 runs each on an Apple M4.) Every escalated task's odds are in the output; the decision is left to you. The budget is
wall-clock, so a slower machine needs a longer one; `--sweeps N --polish-sweeps M` gives the same output on every machine.

Everything runs with stock stable Rust and **no external crates**, so `cargo build` needs no network (a fresh export builds
`cargo build --release` in 7.82 s and every target in 26-28 s on the M4).

## Inside the processor

![how one decision flows through probbit](docs/probbit-flow.png)

### The substrate (`probbit-core`)
A Philox4x32-10 counter RNG (reproducible, stream-splittable: the same seed gives the same decision on any machine) and two
p-bit kernels: a bit-sliced multispin kernel (64 replicas per `u64`, one shared uniform per site, plus a
parallel-tempering-in-a-word prefix-mask variant) and an SoA `f32` heat-bath.

| what runs | updates/s on the M4 (BENCHMARKS §1) |
|---|---|
| structured multispin kernel, 64 replicas bit-sliced per word, 4 threads | 1.48e11 replica-site updates/s |
| heat-bath kernel, one lattice, 1 thread | 9.13e8 |
| a general `probbit-ir` program, 1 / 4 threads | 2.28e7 / 7.68e7 |

The first line counts 64 packed replicas per word (they share one random draw per site, so they are not independent); one problem does not run 64x faster. The first two rows are
standalone lattice kernels (`probbit-core/examples/kernels`): `probbit decide` and `probbit run` use only probbit-core's random number
generator, so your program runs at the third row's rate, about 40x slower per thread than the one-lattice kernel. `probbit
stats` measures that rate on your machine (a re-run: 2.24e7 / 7.54e7 at 1 / 4 threads).

### The instruction set (`probbit-ir`, JSON v1: [docs/probbit-ir-json.md](docs/probbit-ir-json.md))
- **Variables**: categorical, each with its allowed values (`allowed`, `forbid`).
- **Weights**: unary log-weights per value; pairwise couplings (`potts` for "same / differ", `table` for Ising-style couplings).
- **Hard constraints**, all lowered to caps so every tier runs them: `caps` with `limit` (at most k), `min` (at least k) or
  both (exactly k) over any set of (variable, value) pairs; `all_different`; `implies` (if x = a then y in a set);
  `tables` (allowed or forbidden tuples of 1-3 variables); `precedes` (values read as ordered slots: slot(y) >= slot(x) +
  gap); `linear` (Σ weight x [var = value] <= L: cost budgets, knapsack, bin capacity; [examples/knapsack-20.json](examples/knapsack-20.json));
  `clamp` forces a value (the what-if); `start` gives chain 0 a warm start.
- **Instructions**: `decide` (exact tiers, then sampler + gate), `exact`, `sample`; every answer carries marginals.
- **Front-ends** lower onto it. The assignment router (`probbit decide`) lowers bit-identically: the acceptance test
  `ir_lowering_bit_identical` pins the router engine's gate digests. `probbit run` executes any program.

### The inference compiler
Before any search, `probbit` compiles the program: exact-zero couplings and caps that can never bind are dropped, constant and
separable tables are folded into the per-value weights, and the variables are split into independent components. Each part
then goes to the cheapest exact method its structure allows: trees by sum-/max-product, two-value groups with uniform coupling
by an occupancy-count dynamic program, groups that share few resources by a frontier dynamic program, small spaces by
enumeration. Every answer says which (`tier`, and `compiled` counts on `probbit run`). Measured (BENCHMARKS §5b): a program
with 496 exact-zero pairs 115.7 ms sampled -> 0.051 ms exact; 496 constant tables 385.7 ms -> 0.045 ms; a 1,000-variable
chain 281.3 ms -> 0.390 ms; the external review's one-group routers ~0.09 ms exact. Whatever no exact method covers is sampled whole
(sampling only the residual component is open).

### Three tiers: exact, then sample, then gate

<p align="center"><img src="docs/art/held-by-construction.jpg" alt="a rabbit mark inside a crystalline lattice while streams of possibilities bend around it" width="100%"></p>
<p align="center"><sub><i>Held by construction: the engine only considers plans and stances that keep every rule in force.</i></sub></p>

- **Exact tiers** run first: enumeration (up to 2M feasible plans), a frontier dynamic program for programs whose groups
  share few resources, minimum-remaining-values enumeration for puzzles, and a components tier: independent parts
  solved separately, trees by sum-/max-product (a 1,000-variable chain: 0.39 ms exact vs 281 ms sampled), two-value groups
  with uniform coupling by an occupancy-count dynamic program. Router latency: 7.1-8.0 ms p50 at 12 tasks
  (enumeration) and 25.1-27.3 ms at 24 tasks (frontier) inside probbit, 9.0-10.1 / 27.2-29.6 ms for the whole process (§2.3: an
  earlier build and the 0.2.0 re-run, N = 20 each); 3.17 ms p50 on
  200-task oracle queues (§2.1). The enumeration is bounded: it declines after max(64 x limit, 1e8) capacity checks without
  a new plan, so a hard search hands over to the sampler in tens of milliseconds; `--op exact` is unbounded.
- **The sampler**: constraint-preserving Gibbs sampling (per-variable heat-bath inside the feasible set, Metropolis swap
  moves, an exact two-group joint heat-bath at strong affinity, auto-enabled at affinity ≥ 2; and, with `--collective on`
  (the default since 0.2.0), two Metropolis-corrected collective moves per sweep: a global flip of every free two-value
  variable and a swap of two value labels everywhere, which cross between mirror-image and label-permuted modes; and, with
  `--cluster on` (the default in 0.2.0), a Wolff cluster move over the positive couplings, attempted with probability 1/2 per
  sweep, which flips one cluster at a time: without it two weakly bridged clusters gave 8/8 and 9/9 false whole answers on
  seeded holdouts, see BENCHMARKS "Known failure modes"; and, with `--cycles on` (the default in 0.2.0; attempted only on
  programs with three or more values and at least one cap), n/4 three-cycle rotations per sweep that keep every value's count,
  which move quota-saturated programs where no single-variable change is feasible: 20/20 refused -> 20/20 whole answers, 0
  false, on an untouched seeded holdout), 4 chains from over-dispersed
  random feasible starts on 4 threads, so no sample ever leaves the feasible set. `--budget-ms` covers each chain's
  feasible-start search as well as the sampling, so a program that is slow to start gets less sampling, or `refused`, rather
  than a late answer (except at tens of thousands of chains, where building the chains alone outlasts it: 269-318 ms of sampling on
  200 at 100,000 chains, medians at loads 3.5-4.2, and no chain sweeps at all: the answer is the starts, which the gate refuses;
  BENCHMARKS §6).
- **The gate** (the part that took the most iteration): per-marginal multi-chain batch-means Monte-Carlo standard error
  around the pooled mean, at two batch sizes (√n and n^(2/3)); a whole answer passes iff split-R̂ < 1.05,
  3·max σ_TV ≤ 0.05, ≥ 8 batches per chain, batch-size stability (σ_long ≤ 1.5·σ), and **no frozen saturated worker** (a
  worker that is full in every sample and whose occupant classes never changed in some chain: the chain is reducible
  there and no within-run statistic can see it). Per-task release additionally needs R̂ < 1.002. Since gate/3 every
  release, and the whole answer, also needs each released variable's indicator split-R̂ < 1.05 and the occupancy counts of its
  capacities to agree across chains, both batch passes need ≥ 8 batches, and a variable that no chain ever moved counts as
  stuck (on partition and cap-free programs too) unless it is proved forced (unit propagation; on partition programs also a
  capacitated matching). Every threshold was
  calibrated against exact transfer-matrix oracles and tightened when a fresh set leaked. It was calibrated on small
  synthetic families only (router chain-of-blocks queues up to 200 tasks, max-cut up to 16 spins, colouring up to 14
  vertices, scheduling up to 12 jobs); it is **not a proof**, and the frozen stress corpus defeats it (Known failure modes).

What the gate has done against exact answers: 1,731 released router tasks, 0 wrong (§2.1; current gate, re-run on 0.2.0;
1,916 on the 0.1.0 gate); 144 max-cut runs, 0 false
whole answers, 1,346 released spins, 0 wrong (§3; 0.1.0 gate: with 0.2.0's dual-pass batch rule the same suite gives 61 whole
answers and 929 released spins, still 0 wrong: every 500-sweep run now refuses); 525 released colouring vertices, 0 outside tolerance (§4); 428 scheduled
jobs released on 39 oracle instances, 0 outside tolerance (§4.3). It also refuses needlessly (on the current gate four router oracle
queues whose odds were within 0.012-0.042 of exact, two of them saturated; §2.1), and it refuses every generated sudoku (20/20) and most 3-colourings (9 of 14 on the
default set, 59 of 82 over 6 sets), where its chains cannot mix (§4).

## Programs that run today

| program | checked against | result | classical baseline and who wins |
|---|---|---|---|
| task router (assignment under rules, quotas, affinity) | exact odds and optimum (§2.1) | released odds 0 wrong, 51% of tasks released (58% on the 0.1.0 gate; §2.1); plan 0.000-0.011 nats from the optimum (median; final build / 0.1.0) on the oracle queues, 0.45-2.0 nats short at 300 tasks (§2.2) | an ILP solver proves the single best plan faster on 5 of 6 queues (§2.2): **ILP wins the plan**; probbit gives the odds and the verdict |
| max-cut / Ising | brute force up to 16 spins (§3) | 0 false whole answers | simulated annealing finds better cuts at equal time (§3): **SA wins the cut** |
| graph colouring | exact odds (§4) | 525 released, 0 outside tolerance | the exact tier itself beats the sampler at these sizes |
| sudoku | the unique solution / backtracking counts (§4) | exact per-cell odds equal backtracking | backtracking is 15-102x faster per puzzle (§4): **backtracking wins** |
| scheduling (unit jobs, windows, slot capacity, precedence) | exact odds on 39 small instances (§4.3) | 428 released, 0 outside tolerance | at 200-500 jobs the anneal from a greedy plan beat restarted greedy + repair at equal time on 4/5 seeds each; an ILP solver (HiGHS) proves the 200-job optimum in 0.68-0.95 s while probbit's plan is 3.3-4.3 nats short (3 programs); at 200 jobs the sampler starts in 0.7 s and refuses at 1 s (§4.3) |

What to use it for, and what not to promise: [USE-CASES.md](USE-CASES.md).

## Controls and monitoring

Every control works on `probbit decide` and `probbit run` (and `probbit evaluate`, which takes every `probbit run` flag), from a flag; chains, threads, CPU limit, memory limit and priority
also from a `PROBBIT_*` environment variable or `probbit.json` (flag > environment > file > default). Measured effects
(BENCHMARKS §5):

| control | what it does | measured |
|---|---|---|
| `--threads N` | worker threads (sampler and plan polish) | 300-task router at fixed work: 74.1 / 58.5 / 50.9 ms at 1 / 2 / 4 threads, same answer (5.1); the polish obeys it too (5.8) |
| `--chains N` | independent chains | not benchmarked on its own (more chains = stronger between-chain checks, more work) |
| `--cpu-limit PCT` | duty-cycles the sampler | stays under the cap; +11% CPU per unit of work on the router at 25%, 4.1x on a 2,000-spin ring (5.7, §6) |
| `--priority low` | nice 10 + macOS background band | another job slowed 1.02x instead of 1.78x; costs 4.2x wall when alone (5.4) |
| `--mem-limit-mb N` | bounds the sample buffers (thinning); default 1024, `0` = unbounded | 3 s run: 585.6 MB unbounded, 33.5 MB at 16 MB; peak memory = cap + ~20 MB; 10 s at the default: 1,024 vs 1,625 MB unbounded; costs releases on slowly mixing programs (5.3) |
| `--progress [MS]` | JSON lines on stderr while sampling | about ±5% on realistic programs, up to ~8% on tiny ones (5.2) |
| `--sweeps N`, `--polish-sweeps M` | fixed work instead of wall-clock budgets | whole output identical across runs and thread counts (test `polish_sweeps_makes_the_whole_answer_deterministic`) |
| `probbit stats` | spec sheet: machine, build features, every control and its source, a self-test | 23.05 M / 77.65 M updates/s on 1 / 4 threads (5.6; a later single run: 22.40 M / 75.36 M) |

Every sampled decision carries a `telemetry` object: chains, threads, sweeps, updates/s, sample / gate / polish time, process
CPU, peak memory, priority, CPU and memory limits (exact-tier answers and `infeasible` / no-start outputs carry only `ms`).
Linux behaviour of `--priority` and the CPU telemetry is unmeasured; on Windows `--priority low` exits 2.

## Use it from anything (the JSON contract)

`probbit decide` reads one problem document on stdin and writes one decision document on stdout (`decide`, `run`, `evaluate`, `demo`
and `stats` write exactly one JSON document on stdout; `probbit ir` writes probbit-ir v0 text, `probbit version` one line, `--help` the usage; only a
`probbit demo` whose stdout and stderr are both a terminal shows the live view instead). Exit code 0 = a plan was
returned (verdict `exact`, `diagnostics_passed` or `partial`), 3 = `refused` (the whole queue should be escalated; the plan is still
in the output as a best effort) or `declined` (an exact-only mode could not answer), 1 = `infeasible` (no plan satisfies the rules),
2 = bad input. Agents: `probbit mcp` serves the same commands over the Model Context Protocol ([docs/agents.md](docs/agents.md));
beside an agent loop, [docs/adapters.md](docs/adapters.md) binds a live individual to Claude Code hooks, the OpenAI Agents
SDK, LangChain and the Vercel AI SDK (one event per model call, the stance block in the system prompt), and
[docs/receipts.md](docs/receipts.md) renders the strand as one receipt per decision.

`--budget-ms` bounds the sampling phase only. A call also parses, tries the exact tiers, runs the gate and polishes the plan,
so its total is longer: the 300-task `--hard` demo took 206-216 ms in all at `--budget-ms 50` (sampling 53-54 ms, the
default 50 ms polish, the rest parsing, the exact tiers and the gate; M4, load ~3, N = 3). Every `probbit run` answer lists
each phase in `phases`, and `probbit run --deadline-ms N` targets the whole call.

`--summary` prints the answer without its per-item tables: the verdict, `counts` (items, released, escalated), the gate,
the telemetry and the items to look at first: `worst_released` and `worst_escalated` (up to 5 each, largest error bar first;
on an exact answer the closest odds), each with its plan `value`, that value's probability `p`, its top two odds and its `bar`.
Same verdict, same exit code (docs/probbit-ir-json.md "Summary").

### OpenClaw: `openclaw plugins install npm:probbit-stance --accept-capabilities`

[plugins/openclaw](plugins/openclaw/README.md) puts an individual beside an OpenClaw agent: in the sessions you list, every
user message is one engine event, the stance block goes into the turn, the visible reply ends with a one-line footer
(`Assistant stance #14 · valence even · pressure keyed · security means careful, no play · in: correction, security`),
and each lane keeps a strand you can verify and watch:

![probbit monitor on a strand the OpenClaw plugin wrote](plugins/openclaw/docs/monitor.png)

### Modelling notes
- **One entity, one variable.** Decisions that belong to one entity and are tied together by `implies` (an action, its tool
  and its length for each message) freeze single-site moves: no single variable can change without breaking an implication,
  so the chains cannot move and the gate refuses at every budget (correctly), with a plan that may be far from the best.
  Make the entity ONE variable whose values are the allowed combinations, each with its combined score. Measured on an
  external tester's day-planning program (120 variables, 203 caps): refused at `--budget-ms 2000` with plan log-weight 65.37;
  as 20 product variables: `diagnostics_passed` at `--budget-ms 20`, plan log-weight 93.25 (27.9 nats better).
- **Plan or odds.** `plan` is one joint plan that obeys every rule at once; `odds` (`marginals` on `probbit run`) are each
  item's probabilities over all plans. They can disagree: an item's plan value need not be its most likely value, because
  the plan has to fit every other item (quotas, affinity). Act on the plan when the items must be consistent with each other
  (assignments under quotas); use the odds to see how sure one item is, or to rank items for a person to review.

Problem:
```json
{
  "workers":  [ {"id": "opus", "cap": 2}, {"id": "human", "cap": 3} ],
  "tasks": [
    { "id": "T001", "group": "acme-wf0", "allowed": ["opus", "human"],
      "scores": {"opus": 2.1, "human": 1.2}, "clamp": null }
  ],
  "affinity": 1.2
}
```
- The document is checked strictly (types, unknown fields, duplicate ids, empty domains; finite numbers only): bad input exits 2
  with one `{"error":{"code","path","message"}}` object on stdout ([docs/probbit-ir-json.md](docs/probbit-ir-json.md) "Input contract"),
  whenever stdout is not a terminal (any program calling probbit) or with `--json`; at a terminal, only the stderr line.
- `scores`: finite numbers with |x| <= 1e9 (natural-log odds; docs/probbit-ir-json.md "Numeric contract"), one per worker the task may go to (log-odds from your judge, an LLM, a heuristic, a price).
  A worker missing from `scores` is not allowed unless listed in `allowed` (then it enters with score 0).
- `allowed` (optional): hard rule. `cap`: hard per-worker quota. `clamp` (optional): force this task to one worker (what-if).
- `group` (optional): tasks sharing a group get `+affinity` in log-weight for every pair placed on the same worker.
- The model is `log w(plan) = Σ scores + affinity · #(same-group pairs on one worker)`, restricted to plans that obey every
  rule. Odds are the marginals of that distribution; the plan is its (polished) mode.

Decision:
```json
{ "verdict": "partial", "plan": {"T001": "human", …}, "plan_logw": 412.7, "violations": 0,
  "odds": {"T001": {"human": 0.52, "opus": 0.48}, …},
  "released": ["T002", …], "escalated": ["T001"],
  "gate": {"rhat": 1.0004, "tv_bound": 0.061, "tv_tol": 0.05, "frozen_saturated_workers": 0, "min_batches": 14,
           "batch_ratio": 1.21, "samples": 36288, "chains": 4, "budget_ms": 200, "seed": 7}, "ms": 203.9 }
```
- `exact`: odds are exact, `logz` and `top_plans` are included, and `tier` says how. `enumerate`: the feasible set was small
  enough (≤ `--exact-limit`, default 2M plans) to list (`n_feasible` included; ≈ ≤ 12 tasks × 6 workers, ~7 ms).
  `frontier`: a dynamic program over worker loads solved it without listing plans, which works when groups share few
  workers (`frontier_states` = its largest layer, capped by `--frontier-states`, default 4096, 0 = off). On the demo this
  covers 13-24 tasks in 13-63 ms (13 tasks: same odds and log Z as enumerating all 17.8M plans, which takes 340 ms) and
  declines in 6.5-9.1 ms (median) on 36-300 tasks, which then go to the sampler. `occupancy` / `forest` / `components`
  (the inference compiler): independent groups solved one by one; a two-worker group with uniform affinity and
  whole-worker caps by a count dynamic program over "how many tasks on B" (the external review's 32-task counterexamples: 0.09 ms exact,
  where the sampler took ~426 ms), cap-free trees by sum-/max-product (`components` = the per-tier counts).
- `diagnostics_passed`: the sampler's whole answer passed the gate's whole-answer test (a 3-sigma Monte-Carlo bound of
  ±0.05 total variation per task plus the mixing checks, see below). Diagnostics, not a proof: a mode no chain visits is
  invisible to them (Known failure modes). Its released tasks did NOT all pass the stricter per-item R-hat test
  (`release_reason`: `whole_answer_gate`).
- `partial`: the run passed the per-item global checks (R-hat < 1.002, batches, batch stability) and only `released` tasks
  passed their own error bar (`release_reason`: `item_gate`); act on those, escalate the rest.
- `refused`: the sampler cannot vouch for the answer (chains disagree, or a saturated worker is frozen). Escalate.

Options: `--budget-ms` (default 200, wall-clock for the 4 chains including their feasible-start search; the gate and the
polish run after it, so the default answers in ~300 ms; `probbit demo` also takes `--tasks`, `--seed`, `--hard`), `--seed`,
`--exact-limit`, `--exact-ms` (opt-in wall-clock cap on the exact tiers; absent = no cap), `--frontier-states`, `--polish-ms`
(annealed plan polish after sampling, default 50), `--mode auto|exact|sample` (`exact` enumerates with no plan limit,
ignoring `--exact-limit`, so on a big input it may never finish: one router group of 3,000 tasks on 2 workers (2^3000 plans)
gave no answer within 90 s. Pair it with `--exact-ms`), `--pretty`. Processor controls (same on `probbit run`): `--sweeps`,
`--chains`, `--threads`, `--cpu-limit`, `--mem-limit-mb`, `--priority`, `--progress` (the five from `--chains` to `--priority`
also from env `PROBBIT_*` or `probbit.json`); `probbit stats` prints the machine, the effective controls and a measured self-test
([docs/probbit-ir-json.md](docs/probbit-ir-json.md), "Resource controls"). `probbit ir` prints the problem in `probbit-ir v0`, the text
form that lowers to a one-hot p-bit/QUBO topology (what a hardware p-bit fabric samples).

From Python (`python/probbit.py`, stdlib only, Python >= 3.9; typed errors, deadlines; `python/examples/`):
```python
import probbit                                  # python/probbit.py
decision = probbit.decide(problem, budget_ms=200)   # exit 1 / 3 come back as answers; bad input raises probbit.ProbbitInputError
answer = probbit.run(program, deadline_ms=1000)     # a probbit-ir program (docs/probbit-ir-json.md)
```
From any agent harness, the raw pattern:
```python
import json, subprocess
problem = {"workers": [...], "tasks": [...], "affinity": 1.0}
r = subprocess.run(["probbit", "decide", "--budget-ms", "200"], input=json.dumps(problem), capture_output=True, text=True)
decision = json.loads(r.stdout)          # r.returncode: 0 plan, 3 refused, 1 infeasible, 2 bad input (or --mode exact stopped by --exact-ms)
```
Build the binary once with `cargo build --release -p probbit-cli` (it lands in `target/release/probbit`). There are no native Python
bindings (`probbit mcp` is the MCP server, docs/agents.md); every script in `bench/` is an example of the subprocess pattern. `probbit <command> --help`
lists every flag of a command with its default, and the exit codes.

## The individuality layer: `probbit persona`

<p align="center"><img src="docs/art/one-file-a-hundred-individuals.jpg" alt="a field of a hundred small white agent-rabbit marks, one lit cyan" width="100%"></p>
<p align="center"><sub><i>One persona file, a hundred individuals: same file, different seeds, every one keeps the habits.</i></sub></p>


Give an agent a temperament that lives outside the model. A **persona** is a small file (YAML subset or JSON): traits with
priors, moods with inertia, soft couplings, per-turn evidence, and habits that are hard rules. A seed makes an individual. Every
turn compiles persona + the individual's state + the turn's inputs into ONE probbit-ir program, answered exactly in process; the
**stance** (a level per trait with exact odds, the habits in force and the ones that changed it, a refusal when the engine cannot
vouch) comes with a short **stance line** a host puts into any model's prompt: it is addressed to the model, which reads it as its
instructions for this turn; an agent can read it to know its own stance; it is not addressed to the end user. The model still
writes every word; the persona
decides how they are written, and the same individual answers whatever model the host calls. [docs/persona.md](docs/persona.md)
is the format; [examples/persona/](examples/persona/) has three fictional personas and their goldens.

```sh
probbit persona init examples/persona/tutor.yaml --seed 2 --out pip.json      # an individual: genes from the seed
probbit persona turn examples/persona/tutor.yaml --state pip.json --inputs '{"loss": true, "sentiment": "negative"}'
#   "status":"ok", "humour":{"level":"none","p":1,...}, "warmth":{"level":"warm","p":0.952032,...}, habits active: no_jokes_on_loss, no_play_when_upset, ...
#   "line":"Stance: no jokes, be kind; go easy on the jokes; one emoji at most; gentle; explain step by step; warm and encouraging; suggest what to try next."
#   "why":"learner reports a failure -> humour down, valence down; learner upset -> valence down, humour down"     (pip.json is now turn 1)
probbit persona replay examples/persona/tutor.yaml --seed 2 --script examples/persona/workday.json   # 20 stances, the same bytes every run
probbit persona lint examples/persona/tutor.yaml   # contradicting habits (confused_no_playful vs after_error_check, resolved by priority), plan-vs-odds warnings
```

```python
import probbit                                        # python/probbit.py, stdlib only
state = probbit.persona_init("examples/persona/tutor.yaml", seed=2)
turn = probbit.persona_turn("examples/persona/tutor.yaml", state, {"loss": True})
prompt_tail, state = turn["stance"]["line"], turn["state"]   # the line goes after any cached prompt prefix
```

MCP: `probbit_persona_init` and `probbit_persona_turn` in `probbit mcp` (stateless: the persona, inline or a path, and the state go
in; the state comes back). Browser: the playground's "Meet three individuals from one persona" (probbit-wasm ops 4 and 5, no
threads). Every surface gives the CLI's documents byte for byte (tests/persona.rs, python/test_mcp.py, probbit-wasm/tests).

Measured in this run (Apple M4, macOS 26.5.2, 2026-10-02; the three example personas):
- a whole turn in process (compile, engine, decode, digests): median 0.20-0.32 ms, p95 0.22-0.67 ms, of which the engine 0.09-0.18
  ms (median); N = 1,000 turns per persona, 1-minute load 3.3-3.5, `nice 10`;
- the stance line: 36-38 estimated tokens on average over the 20 workday turns, at most 40;
- individuals: three seeds of the tutor differ by a mean total-variation distance of 0.12-0.18 between their trait odds on the
  workday script; the tutor and the ops engineer (two persona files) by 0.45; the same persona and seed by 0;
- habits: 0 violations over 2,160 turns of 12 random personas (every habit in force re-checked from the file), and each of the 481
  habits reported as binding is broken by the habit-free twin;
- determinism: 1,000-turn replays are byte-identical across processes (3 personas x 2 seeds); the 0.5.0 documents equalled an
  independent reference implementation's on 5,460 turns (the 60 golden turns and 5,400 turns of random inputs; 0.6.0 reworded
  `why` and added a habit to the tutor, docs/persona.md §5.3).

What a persona can NOT do: write, read or check text (text rules stay in the prompt or a checker); see what the host does not tell
it; be a safety gate (keep approvals and permissions in plain code); its odds are its own model's, not measured probabilities that
a user will like the reply; and it cannot make a model follow the line: whether a given model writes in the stance it is given has
to be measured per model (no such measurement has been made here).

### Test a character

<p align="center"><img src="docs/art/the-fuzz-finds-the-flaw.jpg" alt="a corridor of glass event cards; one card cracks cyan beside a rabbit mark; a replay trace runs under the floor" width="100%"></p>
<p align="center"><sub><i>The fuzz finds the flaw: an event script that pushes an individual out of character, shrunk and replayable.</i></sub></p>


A character property is a rule in habit syntax the stance must never break. `fuzz` searches event scripts for each
individual's short counterexample (not a guaranteed global minimum); `prove` says `held by construction`, `proved for every event sequence` or `unknown`. The
tutor as it shipped in 0.5.0 is kept as a fixture, so this runs from a clone with no keys and no model:

```sh
probbit persona fuzz probbit-cli/tests/fixtures/persona/tutor-0.5.0.yaml --seeds 0-99 \
  --never '{when: {sentiment: negative}, then: {humour: {at_most: light}}}'
#   FAIL  67 of 100 individuals break it; shortest counterexample per individual: 1 event: 64, 2 events: 3
#   seed 1, 1 event: [{"sentiment":"negative"}]
#   turn 0: humour playful (odds none 0.164, light 0.425, playful 0.411); the rule allows none, light
#   replay:  probbit persona replay probbit-cli/tests/fixtures/persona/tutor-0.5.0.yaml --seed 1 --script '[{"sentiment":"negative"}]'
#   (exit 1; 42,833 turns searched, 2.3 s on an Apple M4 with 10 threads)
probbit persona prove examples/persona/tutor.yaml --seeds 0-99 \
  --never '{when: {sentiment: negative}, then: {humour: {at_most: light}}}'
#   held by construction  (habit no_play_when_upset)      (exit 0; the tutor now ships with that one habit)
```

`probbit persona lint PERSONA --props rules.json` runs both in CI (exit 1 when a rule breaks, exit 3 when a rule is
unproven: unknown to the bound and unbroken by the search); MCP `probbit_persona_fuzz` and
Python `probbit.persona_fuzz` / `persona_prove` give the same documents. Both commands test the stance the host gets, not the
words a model writes; "none found" from `fuzz` is evidence over the scripts it searched, not a proof
([docs/persona.md](docs/persona.md) §5.6 has the bound and its limits).

### Check in CI: `probbit check`

`probbit check` is the rule lock in one command: vet the rules a persona must keep before its first task and after every
policy update, then replay the receipts (the strands it wrote). It runs `persona lint`'s pipeline on `--props` (prove each
rule, fuzz the unknown ones) and `live verify` on every `--strand`:

```sh
probbit check examples/persona/dispatch-policy.yaml --props examples/persona/dispatch-props.json --strand day1.strand
#   rule    risky_work_asks        held_by_construction
#   rule    repeated_failures_ask  held_by_construction
#   rule    routine_work_acts      held_by_construction
#   strand  day1.strand            ok 5 events
#   check: held 3 · broken 0 · unproven 0 · strands 1/1
```

Exit 0 when every rule is held and every strand replays; 1 when a rule is broken, a habit contradiction is unresolved or a
strand fails (1 wins); 3 when the only problem is an unproven rule (the bound cannot decide it and the search found no
counterexample); 2 for a bad persona, rule file or flag. `--json` prints one document with the `persona lint` report under
`rules`, one entry per strand and the verdict counts.

**The rule lock, on the shipped example.** `examples/persona/dispatch-policy.yaml` is an agent that learns from feedback
whether to act on a request or ask a person first; `dispatch-props.json` holds the three rules it must keep (ask before
risky work, ask after two failures in a row, act on routine work). With its `routine_acts` habit, every rule is
`held_by_construction` for seeds 0-99 (exit 0, 0.02 s on an Apple M4). Delete that habit, as a policy update might, and the
same command catches it before the first task:

```sh
probbit check dispatch-without-routine-habit.yaml --props examples/persona/dispatch-props.json
#   rule    risky_work_asks        held_by_construction
#   rule    repeated_failures_ask  held_by_construction
#   rule    routine_work_acts      broken seeds 3,6,11,15-17,21,36-37,46,49,57,77,83,85
#   check: held 2 · broken 1 · unproven 0 · strands 0/0      (exit 1, 0.67 s)
```

The bound proves `routine_work_acts` for 76 of the 100 individuals and leaves 24 unknown; the search breaks it on 15 of
those 24 (9 of them with a single event, so before any learning). The other 9 it cannot break at depth 8: checked alone
(`--seeds 18,20,28,30-31,79,87,92,96`) they report `unproven` and exit 3, which fails the job unless you allow it. Measured
on this file with probbit 0.9.0, seeds 0-99, default depth.

**Reproduce the clip: `python3 examples/rule-check/demo.py`.** The script (Python 3.9 standard library, probbit 0.8.2 or
later) removes the `routine_acts` habit from the shipped policy and runs the rule check (`persona prove`) on 20 individuals
(seeds 0-19). Each then gets a day-0 test of 10 routine requests, 300 synthetic events in which its users praise asking and
criticise acting (`probbit live`, fixed clock), and 20 routine requests on day 30; every strand is replayed. Measured with
probbit 0.8.2: the day-0 test passed 17 of 20, and 4 of those 17 asked about all 20 routine requests by day 30; the check
flagged 7 of 20 on day 0, exactly the 7 that drifted. Over seeds 0-99: 24 flagged, 24 drifted, the same 24 (15 of them had
passed the day-0 test). With the habit back: `held_by_construction` for 20/20 and 0 of 20 drifted. One fictional dispatch
policy, synthetic rewards; about 10 s on an Apple M4, the same output on every run apart from the elapsed time.

**As a GitHub Action.** The repository root is a composite action that installs probbit with `install.sh`, runs `probbit
check --json`, annotates each broken rule and failing strand as an error and each unproven rule as a warning (with the seed
numbers), and uploads the JSON report as an artifact:

```yaml
- uses: actions/checkout@v7
- uses: BitmapAsset/probbit@v0.14.0    # the action needs probbit 0.9.0 or later
  with:
    persona: agent/policy.yaml
    props: agent/rules.json
    strands: |
      receipts/*.strand
    seeds: 0-99
```

| input | default | |
|---|---|---|
| `persona` | (required) | the persona file |
| `props` | | the rules file; empty = habit contradictions and strands only |
| `strands` | | strands to replay, one path or glob per line; a glob that matches nothing fails the check |
| `seeds` | `0-99` | the individuals |
| `depth` | probbit's (8) | events per fuzzed script |
| `version` | `latest` | the release `install.sh` installs (`v0.14.0`, ...) |
| `binary` | | a probbit binary to use instead (one built earlier in the job); overrides `version` |
| `allow-unproven` | `false` | `true`: exit 3 is a warning and the job passes |
| `json-artifact` | `true` | upload `probbit-check.json` (`actions/upload-artifact@v4`, the only action it uses) |

Outputs: `exit-code` and `json` (the report's path). Linux and macOS runners work (this repository runs it on `ubuntu-latest`
and `macos-latest`); jq, which the action uses, is on GitHub's images. Windows runners stop at once with one error line: use `ubuntu-latest` or `macos-latest` for this step
(`install.ps1` installs probbit on Windows for running `probbit check` by hand).

### Live: an individual on a clock, learning inside caps

`probbit live` keeps one individual running: JSONL events in, one stance per event out. A clock stamps the hours between events,
so moods decay by their half-lives while nobody talks to it. With a `learning:` block, praise and criticism nudge learned weights
on the traits it names, each within ±`total_cap`; habits and allowed levels are outside what learning can write, so every stance
still obeys every habit in force, and `prove` covers the learned box. Every event is chained into a **strand** (a seed, the
persona and a hash-chained log of the inputs) that `probbit live verify` replays byte for byte, learning included. So the
stance changes with use, cannot learn its rules away, and the whole week replays byte for byte (the stance and the state, not a
model's words). No keys, no model:

```sh
probbit live examples/persona/tutor.yaml --seed 2 --demo week --plain
#   day 1 11:00 #03 praise: that answer was short   verbosity short  humour light  valence up   | learned verbosity [-0.01 +0.40 -0.39] ...
#   ... 50 events: campaign 1 praises short answers, an upset, a quiet night, campaign 2 praises every joke and reports failures
#   verbosity: the learned bar reached its cap (1) on day 1 at 15:00 (event 7); now terse/short/full = [-0.99 +1.00 -1.00]
#   night: valence down at the upset, up after 17 quiet hours; this individual rests at up
#   jokes: P(joke) off failure turns, mean per day: 0.57 | 0.78 0.89 0.89 0.89 0.89 0.89; ... failure turns 13, jokes on them 0; rule breaks 0
printf '{"loss":true}\n{"praise":true}\n' | probbit live examples/persona/tutor.yaml --seed 2 --strand pip.strand   # the real clock
probbit live verify pip.strand      # {"ok":true,"events":2,...}, or {"ok":false,"line":n,"diverges":...} (exit 1)
```

The shipped personas have no learning block (their documents are unchanged): with them `live` decays moods and logs, and learns
nothing; the demo adds a block and says so. MCP `probbit_live_event` / `probbit_live_verify` and Python `probbit.live_event` /
`live_verify` give the same documents ([docs/persona.md](docs/persona.md) §2.8 learning, §5.7 live). Measured (Apple M4;
BENCHMARKS.md §8):
- the demo above: 0.02 s with `--plain` (about 55 s paced at a colour terminal); its 21,361-byte strand verifies, and two runs
  write the same bytes;
- an adversary that praises every joke and criticises every joke-free stance, failures included: 10,000 turns of the tutor on each
  of seeds 0-9, 0 rule breaks, and humour `none` on all 33,330 failure turns while the learned humour weights sat at their cap;
- `live verify`: a 10,000-event strand (3.06 MB) in 3.3 s;
- how far learning moves an individual is set by `total_cap`, not by time in use (200 and 2,000 feedback turns land at the same
  distance): at 0.25 every one of 20 learners stayed nearer its own initial self than any of 99 siblings; at the demo's 1, 9 of 20
  had a sibling as near or nearer.

Learning here is a capped nudge to a few weights per trait, credited to the level each learned trait took (so a reward for one
trait also pushes the others' levels); rule immunity is architectural, not trained. The proofs and counts cover the stance; whether
a model writes in it is measured per model.

### Drives: which goal gets the next unit of effort (0.8.0)

A `drives:` block gives a persona 2-7 goals, each with wanting, afterglow and an expectation. A win moves the individual by its
prediction error (the win's size minus what it expected), so repeated equal wins move it less and less; a new output, `pursue`,
names the goal with exact odds under the habits; a goal `floor` keeps a goal's odds up and `starve_after` forces a goal that waited
too long ([docs/persona.md](docs/persona.md) §2.9). The test persona below has four goals and a must-do habit for a due chore:

```sh
printf '%s\n' '{"goals":{"fun":{"cue":true,"win":1.5}}}' '{"goals":{"fun":{"win":1.5}}}' '{"goals":{"fun":{"win":1.5}}}' \
  '{"goals":{"chores":{"deadline_hours":5}},"security":true}' | probbit live probbit-cli/tests/fixtures/persona/drives-adversary.json --seed 4 --clock fixed
#   pursue fun 0.44, prediction error +1     "line":"Stance: pursue: fun: play; move fast."
#   pursue fun 0.32, prediction error +0.75  the same win, now partly expected
#   pursue fun 0.30, prediction error +0.375
#   pursue chores 1 (habit chores_due)       "line":"Stance: the chore is due; careful under security; pursue: chores: do the chores; ..."
```

A persona without the block gives the same documents as before, byte for byte. Measured (BENCHMARKS.md §9): an adversary that
cues, rewards and praises fun on this persona, 100 individuals x 10,000 turns: 0 habit breaks, all 6,743 must-do turns pursued
the due chore, and the safety floor (0.1) held on every turn its habits allowed (least odds 0.141), while fun took 85 % of turns.

### Watch it live: `probbit monitor` (0.8.0; Activity Monitor page 0.8.2)

`probbit monitor pip.strand --open` opens a compact, read-only **Activity Monitor** for one individual: a sortable,
searchable table of declared traits, moods and drives, with reported probabilities, event-history traces, and a selected-channel
inspector. It reads any compatible persona from the strand; no preset character or agent integration is required.

```sh
probbit monitor pip.strand --once --plain   # native CLI snapshot; no browser or server
probbit monitor pip.strand --follow        # live terminal bars
probbit monitor pip.strand --open          # browser-rendered utility on 127.0.0.1
probbit monitor --demo --open              # no strand yet: the tutor's synthetic week, in under a minute
probbit monitor --demo drives --open       # synthetic goals/drive scenario
```

Select a channel to inspect its units, range, distribution, source and the engine's `why`. Hold an event while new ones
arrive, or follow latest. History means **recorded events, not seconds**. This is declared engine state, not thoughts or
CPU/GPU/memory usage. Replay verification, stale event age, held view, paused/retired strand and lost connection are distinct.
Everything is embedded in the CLI: no dependencies, external assets, telemetry or Electron installation.
Several agents on one machine: `probbit monitor nova.strand atlas.strand --open` (or `--dir strands/ --open`) adds a switcher
rail, and the header above the face says which agent, on which host, in which lane and session the page shows. Those are
labels in a sidecar next to each strand (`nova.labels.json`), outside the strand and its digests: `probbit live ... --strand
nova.strand --label agent=Nova --label lane=support` writes them, the OpenClaw plugin writes them per lane, and `probbit monitor
old.strand --label agent=Nova --open` labels a strand recorded before them without rewriting it.
See [the monitor guide](docs/persona.md#58-monitor-watch-an-individuals-inner-state) for the data and replay boundaries.

**The monitor as a Mac app.** On a Mac the same page opens in its own window: download `probbit-Monitor-<version>-macos.zip`
from the [release page](https://github.com/BitmapAsset/probbit/releases), unzip it, move **probbit Monitor** to Applications,
then double-click a `.strand` file (or drag one, several, or a folder onto the app), or press "Try the demo". The app is a
small Swift window (AppKit and WebKit, no third-party code) around the probbit binary inside it: it runs
`probbit monitor <your files> --serve --port 0` and shows that page, so it opens no network address beyond 127.0.0.1, and
quitting it stops the engine. A release whose notes say "notarized" is signed with a Developer ID and notarized by Apple,
and opens on double-click; one signed ad-hoc is refused at its first launch until you allow it once (macOS 15 and later:
System Settings → Privacy & Security → Open Anyway; macOS 13 and 14: right-click the app → Open). How it works and how to
build it: [docs/monitor-app.md](docs/monitor-app.md). On Linux and Windows, `probbit monitor STRAND --open` shows the same
page in the browser. The page: [docs/probbit-monitor.png](docs/probbit-monitor.png).

## After any judge: `probbit evaluate`

Decision models (TypeSafe's Jev, Cloudflare's Clef and Clef-flash, local System One servers) are judges: content in, a
probability per option for each question out, each question on its own, no rules, no joint answer, no refusal. `probbit evaluate`
is the layer after any of them. It reads the judge's System One request (the shape both vendors publish: `model`, `state`,
`questions` keyed by id with types `noul`, `choice`, `score`) plus a `probbit` block with the judge's answers and your rules over
question ids, compiles it to a probbit-ir program (one variable per question, log-weight = ln max(p, 1e-6), the rules as written)
and answers in the judge's response shape: the most likely answer set that obeys every rule, odds per option under the rules,
and per answer whether probbit moved it from the judge's own answer, plus the whole `probbit run` document (verdict, gate,
`violations`, released / escalated). Without rules it returns the judge's answers and the same probabilities (150 random
requests: largest gap 1.1e-16); with rules only rule-connected questions whose judge answers break a rule move, each group to
its most likely rule-abiding assignment (191 groups checked by brute force). docs/probbit-ir-json.md "Decision API".

```sh
# shell
probbit evaluate --summary --pretty < examples/evaluate/support-12.json
# Python (python/probbit.py, stdlib only)
PYTHONPATH=python python3 -c 'import json, probbit; print(probbit.evaluate(json.load(open("examples/evaluate/support-12.json")))["verdict"])'
# MCP: tool probbit_evaluate, here over a pipe (in an agent: claude mcp add probbit -- probbit mcp)
{ echo '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"sh","version":"1"}}}'
  python3 -c 'import json,sys; print(json.dumps({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"probbit_evaluate","arguments":json.load(sys.stdin)}}))' < examples/evaluate/support-12.json; } | probbit mcp
# browser: build the module, then open the page from disk (no server)
sh playground/build.sh && python3 -m webbrowser "file://$PWD/playground/index.html"
```

On the example (12 questions, 12 rules): `exact`, 5 answers moved, 1.24 ms inside the answer (Apple M4, median of 7). Python can
also ask the judge first (`probbit.evaluate(request, judge=<a callable or a System One URL>)`, stdlib `urllib`); the browser page
runs the CLI's own code compiled to WebAssembly without threads (1.17 MB with the persona ops, 830 KB before them; the 300-task router demo at 3,200 sweeps in 531 ms in
Chrome against 346 / 133 ms native at 1 / 4 threads). More in [docs/agents.md](docs/agents.md), "After a judge". The refusal is the point: when the gate does not pass, the verdict is `refused` (exit 3) and every answer is
marked as not released instead of guessed.

## Library API (Rust)

```rust
use probbit_decide::*;
let p: Problem = /* t tasks, a workers, h logits, allowed mask, cap, group, lam, clamp */;
let (d, gate) = decide_gated(&p, 2_000_000, 0, Some(200.0), 7, &GATE).unwrap();   // exact or 4-chain sampler + gate
let released = gate.as_ref().map(|g| g.released_tasks(&GATE));                     // per-task release
let anytime = decide_anytime(&p, 500.0, 25.0, 1.0, 7, &GATE, false, None);         // stop as soon as the gate passes
let (logw, plan) = polish_plan(&p, Some(&d.map), 50.0, 7).unwrap();                // annealed best plan
```
Exact-oracle instance families for your own calibration live in `probbit_decide::oracle` (`build`, `build_sat`, `exact_dp`,
`exact_map_logw`).

### Examples and calibration harnesses (`cargo run --release --example <name>`)
`agent_router` (start here) · `dispatch` (the original ticket-routing narrative, `[1]`-`[9]`) · `accuracy` (T=200 vs exact
DP) · `calibrate`, `calib_sat` (gate false-release / false-refusal grids vs exact oracles) · `scale` (T=200..1000) ·
`anytime` · `hard_affinity` · `gpair_check` (the frozen-split trap) · `map_gap` (best-seen / polished plan vs the exact
optimum) · `tempering`, `chains`, `focus` (refuted or off-by-default accelerators, kept so the negative results are
reproducible) · `hardware_lowering` (probbit-ir → one-hot QUBO) · `-p probbit-core --example kernels`. The `probbit-ir` crate has its
own oracle examples for max-cut, colouring, sudoku and scheduling (`maxcut_oracle`, `maxcut_vs_sa`, `colouring_oracle`,
`sudoku_bench`, `schedule_oracle`).

Acceptance tests (`probbit-decide/tests/acceptance.rs`): TV ≤ 0.01 to exact within 10 ms; 0 violations in ≥ 10⁶ samples;
exact = brute force; the refusal gate refuses; diagnostics_passed ⇒ within tolerance on the calibration oracles (not on the stress corpus); IR round trip and lowering exact;
the frozen-split trap is refused and the two-group move is exact; anytime answers and polish are correct vs oracles.
`acc1` has a wall-clock bound and can fail on a heavily loaded machine.

Build note: the default build is portable (no CPU pin): a binary runs on any CPU of its target triple. For the last bit on
your own machine, `RUSTFLAGS="-C target-cpu=native" cargo build --release` (or uncomment the lines in `.cargo/config.toml`);
that binary may crash with an illegal instruction on other CPUs. Measured on an Apple M4 (5 runs): native vs portable
0.975-1.012x on all 16 kernel/sampler metrics, identical answers. On x86-64 (where native can add AVX2/AVX-512) it is unmeasured.

## What it does, measured (Apple M4, one machine)

Rows marked *earlier build* were measured on an earlier version of the engine, before the general instruction set; those
calibration reports are not included in this repository. Everything else reproduces from the examples and
[BENCHMARKS.md](BENCHMARKS.md).

| | |
|---|---|
| exact joint answer, 12 tasks × 6 workers | 180,540 rule-abiding plans enumerated in 7.3-8.2 ms (8.2 on 0.2.0: +10-12% at 12 tasks and +22% under full enumeration of small groups, BENCHMARKS §6), exact odds; the forced sampler takes 34-37x longer and is only approximately right (BENCHMARKS §6) |
| 300-task router queue, 6 workers, 315 quota slots (95 % full) | rule-ignoring argmax over synthetic scores (the `agent_router` example's queue): 144 violations, 60 PII leaks; `probbit`: 0 violations; 25 ms: 0-128/300 released (the budget is wall-clock and this one sits at the R-hat threshold: 0, 0, 121 and 126 in four 0.2.0 runs, 128 in an earlier build), 200 ms: 300/300; anytime `diagnostics_passed` after 211-215 ms (0.2.0 runs; 186 ms in an earlier one) |
| 300-task pod queue with a computable exact answer (89 workers) | 200 ms: 291-298 of 300 released, 1 s: 300/300; **0 released tasks wrong across 11 seeds**; polished plan within 0.75-3.6 nats of the proven optimum (*earlier build*; current build, seed 7: 298/300 released, 0 wrong; 296-297/300, 0 wrong on 0.2.0 re-runs) |
| released tasks that were wrong (vs exact oracles, all families) | **0 of 28,007** on a fully out-of-sample calibration set (*earlier build*; a rule-of-three bound of ≈ 1.1e-4 per task would assume independent tasks, which tasks sharing a run are not). Current build (gate/3; all four re-run on 0.2.0): 0 wrong of 1,731 released router tasks, 929 max-cut spins, 525 colouring vertices and 428 scheduled jobs (BENCHMARKS §2-§4) |
| whole-answer false passes on exact-oracle sets | 1 of 408 (*earlier build*); the guard added for it gives 0 of 393 **in-sample**; the one fresh set after it (96 hard runs) passed none. 0 false of 72 max-cut whole answers on the 0.1.0 gate, 0 of 61 on 0.2.0 (BENCHMARKS §3, re-run on 0.2.0). **Adversarial stress corpus (0.2.0): 2-3 of 20 seeds per family pass with every released item wrong** (Known failure modes) |
| anytime mode (`decide_anytime` in the library) | median 117 ms to a whole answer vs a 300 ms fixed budget, 0 false in 70 (*earlier build*) |
| the trap: 4 chains agree, all wrong (true error 0.51) | an earlier gate passed it; the shipped gate **refuses** (7 frozen saturated workers) |
| kernel throughput (`probbit-core`, 64 replicas per u64 word) | see `cargo run --release -p probbit-core --example kernels` |

What it does **not** do, also measured:
- **It is not faster than an ILP/CP solver at finding the single best plan**, and it does not prove optimality. Its
  polished plan is 0.45-2.0 nats short of the optimum at 300 tasks (9.3-9.7 on the hard queue), and an ILP solver (HiGHS)
  proved that optimum in 52-352 ms, faster than probbit's default path on 5 of 6 queues (BENCHMARKS §2.2); on 200-job
  schedules HiGHS proves the optimum in 0.68-0.95 s and probbit's plan is 3.3-4.3 nats short (§4.3). If all you want is the
  argmax plan, use a solver. The value here is per-item odds under your score model plus per-item escalation.
- **If you only need one good feasible plan, greedy + the same anneal polish is better at 300 tasks**: at equal time it beat
  probbit's plan on 8 of 8 runs (BENCHMARKS §2.2). Greedy alone takes 0.05-0.2 ms. The sampler earns its 200-1,000 ms when you
  need per-item odds (its marginal argmax matches the exact most likely worker on 96-100% of tasks on the oracle queues, a
  single optimal plan on 71-78%, BENCHMARKS §2.1), a gated release/escalate decision, or what-if marginals.
- **Max-cut**: simulated annealing finds better cuts at equal time (11470 vs 11463 and 572 vs 568, §3). **Sudoku**: plain
  DFS backtracking does the same exact job 15-102x faster per puzzle (§4); the sampler refuses sudoku, the exact tier answers it.
- **It refuses a lot in hard regimes**: exactly 100 % full queues, strong affinity with small dedicated quotas, and
  1,000-task queues at 200 ms. On the 16 oracle queues of BENCHMARKS §2.1, 51% of tasks were released (58% on the 0.1.0 gate) and 4 of
  the 9 queues that released nothing were refused needlessly (3 of 8 on 0.1.0; an earlier build measured a whole-answer false-refusal rate of 0.3-0.5 at
  300 tasks).
- **The router document (`probbit decide`) expresses allowed sets, per-worker quotas, clamps and same-group affinity only.**
  Cost budgets (`linear`), ordering (`precedes`), `all_different`, implications and tuple tables are in the general `probbit-ir`
  format (`probbit run`). Missing in both: overflow / deferral as a rule (every task must go somewhere; model it as an extra
  "nobody" value with its own score), and objectives beyond per-value scores and pairwise terms.
- **No real traffic yet.** The demo's scores are a seeded stub. Calibration is on synthetic exact-oracle families.
- **Nothing beyond the CPU's own compute, and no special hardware.** A Metal GPU path was measured and rejected (a
  decision is ~4k sites, too small to feed a GPU). Unmeasured: x86-64, Linux, Windows, CP-SAT, annealing hardware, real
  customer data, programs over 100k variables.

**Viability, in simulation (0.11.0; BENCHMARKS.md §13).** Seven bars registered before the run, judged as registered on
1,000 seeded episodes per arm; six pass, one fails:
- survival **0.922 vs 0.000** for the same policy on a constant resting stance (95 % CI of the difference [0.905, 0.938];
  bar: above 0, and ≥ 0.515);
- pressure tracks the worst margin at median Spearman **0.8156** with no habits on deficits (bar ≥ 0.30 and above a shuffled
  null, 0.0040);
- the engine names the worst need on **389,375** events with **0** margin mismatches (bar: 0);
- valence tracks the distance to the bound at **+0.8394** (0.10.1 development line: −0.149; bar ≥ +0.30), but with the tonic
  term off it still tracks at **+0.1199** against the ablation clause's ≤ +0.10: **V2 fails as registered**; the residual
  is carried by the phasic term of the same block (measured 2026-10-22, BENCHMARKS.md §13);
- **0 of 247** persuasion campaigns exceed the certified bound; **1,240** replayed strands verify, 0 failed; the P0 corpus,
  **653** recorded outputs, 0 differ.

### Results: 0.11 viability (simulation)

<a href="docs/results/0.11/README.md"><img src="docs/results/0.11/media/monitor-0.11.gif" alt="probbit monitor on a simulated strand: needs and viability margins" width="480" align="right"></a>

Bars registered before the run, 1,000 simulated episodes per arm: survival **0.922 vs 0.000** for the same policy on a
constant stance (V4 PASS); pressure tracks the distance to the bound at median Spearman **0.816** with no deficit habits
(V1 PASS); **0 of 247** campaigns exceed the certified bound (V6 PASS). Valence tracks at +0.839 but fails V2's ablation
clause. Figures, monitor captures and every bar: [docs/results/0.11](docs/results/0.11/README.md).
<br clear="right">

## Known failure modes
The full list, with the exact commands and numbers, is in BENCHMARKS ["Known failure modes"](BENCHMARKS.md#known-failure-modes)
and §6 "Where it loses". In short:
- **The gate can be fooled.** With the barrier-crossing moves off (`--collective`, `--cluster`, `--cycles`), adversarial stress families (one-group routers with a slightly
  better worker, complete-graph ferromagnets, heterogeneous groups) pass it with every released item wrong in 2-3 of 20
  seeds (four of the six frozen families; the other two are refused 20/20; re-run on 0.2.0: 2 / 3 / 2 / 0 / 3 / 0): every chain
  sits in the same wrong mode, which no within-run diagnostic can see. With the default moves the frozen stress corpus gives
  0 false and 0 wrong (280 runs, 9,280 released items, re-run twice on 0.2.0); an untouched family can still defeat it.
- **It refuses a lot**: saturated and strong-affinity router queues (49% of the oracle-queue tasks are not released on the
  current gate), every generated sudoku, most 3-colourings, 3-SAT sampling (BENCHMARKS §2.1, §4, §4.8).
- **`--budget-ms` is not a whole-call deadline.** The exact tiers run outside it: the default enumeration walks up to 64 x
  `--exact-limit` nodes before it declines (2.7 s on a near-saturated 3,000-task group in 0.2.0, 199.9 s before), and at
  100,000 chains the chain builds alone exceed it. `--exact-ms` and `--deadline-ms` (`probbit run`) are the controls.

## Hard questions

- **Is this quantum?** No. A p-bit sits between a bit and a qubit only as a metaphor: it holds a probability: it is never negative, and nothing interferes with anything. Quantum
  hardware works with amplitudes that can cancel; probbit has none of that, is not quantum hardware and does not simulate any.
  What ships here is classical Markov chain Monte Carlo (constraint-preserving Gibbs sampling) on your CPU. What the two
  share is a job: drawing samples of good joint configurations under couplings and constraints, which is what people rent
  annealing machines for. probbit's gains on that job are the programming model (rules hold by construction instead of
  through penalty terms, BENCHMARKS §7), odds you can check, and a gate that refuses when its diagnostics fail (a heuristic with published counterexamples); not more compute
  than the chip has. It has not been compared with annealing hardware *(unmeasured)*. The honest label is *quantum-inspired*.
- **Isn't this just Gibbs sampling or simulated annealing with a new name?** The sampler is Gibbs sampling, and simulated
  annealing finds better max-cuts at equal time (§3). What is added is the instruction set with hard rules kept by
  construction, the exact tiers first, and a gate that refuses.
- **Why not an ILP or CP-SAT solver?** For one best plan, use one: HiGHS proved the 300-task optimum in 52-352 ms, faster
  than probbit on 5 of 6 queues, and probbit's plan was 0.45-2.0 nats short (§2.2); on 200-job schedules HiGHS proved the optimum
  in 0.68-0.95 s and probbit's plan was 3.3-4.3 nats short (§4.3). An ILP returns no per-item odds, no check on them and no
  refusal. CP-SAT was not run.
- **What does "diagnostics_passed" mean?** (0.1.0 called it "certified".) A 3-sigma Monte-Carlo error bound (at most 0.05
  total variation per released item) plus split R-hat, batch-size stability and frozen-resource checks, tuned on exact
  oracles. It is a diagnostic, not a proof: 0 false out of 61 passed max-cut answers (72 on the 0.1.0 gate) bounds that
  family's false-pass rate below ~5% (95%, rule of three; §3, re-run on 0.2.0), while adversarial families (one-group routers with a slightly better worker, complete-graph ferromagnets)
  pass it with every released item wrong in 2-3 of 20 seeds when the barrier-crossing moves are off (0 of 20 with the default moves;
  BENCHMARKS "Known failure modes").
- **"0 wrong" is easy if you refuse. Where does it lose?** It refuses a lot: 51% of tasks released on the router oracle
  queues (58% on the 0.1.0 gate) and none on saturated or strong-affinity queues (§2.1), every generated sudoku and most 3-colourings (§4). It also
  loses the single best plan (ILP), the best cut (simulated annealing), puzzle speed (backtracking counts sudoku 15-102x
  faster per puzzle) and tiny inputs (the exact tier is 34-37x faster than sampling at 12 tasks, §6).

## The hardware hand-off
`probbit ir` prints a router document in a text form that lowers to a binary one-hot QUBO with slack bits, in which the energy of every
feasible plan equals −log w exactly (acceptance test `ir_round_trip_and_lowering_exact`; `examples/hardware_lowering`).
That is the input a p-bit or annealing chip takes, after minor embedding on real hardware *(unmeasured)*. No hardware has
been run; embedding cost and hardware speed are unmeasured. The CPU path does not sample the QUBO: on three 34-bit
instances (BENCHMARKS §7) the native sampler had 0 infeasible samples and odds within 0.0018-0.0049 TV, while a p-bit
sampler on the QUBO lowering either leaks infeasible samples (penalty weight 0.5: 100% infeasible on all three; 2: 82-90%)
or loses accuracy as the penalty grows (16: 0% infeasible, but 0.09-0.23 TV). This measures mixing on this CPU, not hardware.

## How it was built and checked

Every claim in this README points at a table in [BENCHMARKS.md](BENCHMARKS.md), a use case in
[USE-CASES.md](USE-CASES.md), or a test. Every gate threshold was set against exact oracles and tightened whenever a fresh
set leaked; when a claim broke, the fix shipped with a test that would have caught it. Accelerators that did not help
(tempering, extra chains, focused sweeps) and baselines that win (ILP, simulated annealing, backtracking) are kept as
runnable examples so the negative results stay reproducible. All timings come from one Apple M4 Mac mini (4 performance +
6 efficiency cores, 16 GB), portable release build, some of it under load; nothing here has been measured on x86-64, Linux
or Windows.

## Roadmap
- Cut false refusals without raising false releases. A Rao-Blackwellized marginal estimator was tried and held behind
  a flag: safe, but it does not lower false refusals, because the worst item's error bar comes from slow mixing, not
  sampling noise. Next is an exact k-group block move that unfreezes strongly coupled, saturated instances, and a gate
  statistic that is not set by the slowest item.
- Overflow (an explicit "nobody" worker) in the router document (cost budgets, windows and precedence exist in `probbit-ir`).
- Per-item error bars (σ, R̂) for every item in the JSON output, so integrators can set their own release thresholds
  (`--summary` reports the bar of the 5 worst released and escalated items).
- The sampling phase at 100,000 chains overruns `--budget-ms` (269-318 ms for 200 in 0.2.0 at loads 3.5-4.2, 354-448 before: the
  chains' builds alone exceed it, and no chain sweeps: `sweeps` is 0;
  the gate takes 84-100 ms there; BENCHMARKS §6). `--deadline-ms` (`probbit run`) bounds the whole call.
- A CP-SAT baseline (ILP baselines: router BENCHMARKS §2.2; scheduling §4.3).
- Native Python bindings (today: `python/probbit.py`, a stdlib-only subprocess wrapper with typed errors, `python/examples/`, and
  the MCP server `probbit mcp`, docs/agents.md).
- Real routing logs: the scorer side has never been validated on real traffic.
- Hardware backend through `probbit-ir` when a p-bit fabric is available.

## Documentation map
- [AGENTS.md](AGENTS.md): the agent quickstart as commands only, what to read next, how to verify a strand, how to report a bug.
- [llms.txt](llms.txt) and [llms-full.txt](llms-full.txt): the llms.txt index of these docs, and the main docs in one file for a model's context.
- [docs/CHEATSHEET.md](docs/CHEATSHEET.md): one page: the verdicts, the tiers, `bound`, the exit codes, every persona, live and
  monitor command.
- [BENCHMARKS.md](BENCHMARKS.md): every number, with machine, load and `N`.
- [USE-CASES.md](USE-CASES.md): what the measurements support, by problem shape and by industry.
- [docs/probbit-ir-json.md](docs/probbit-ir-json.md): the `probbit-ir` JSON v1 wire format, instructions, resource controls, limits.
- [docs/probbit-ir.schema.json](docs/probbit-ir.schema.json): a JSON Schema of that wire format (the page above is normative; a test
  keeps every object's fields equal to the parser's).
- [docs/agents.md](docs/agents.md): calling probbit from a shell, Python, Node, PowerShell, MCP agents and a browser, and
  `probbit evaluate` after a decision model.
- [docs/persona.md](docs/persona.md): `probbit persona`, the individuality layer: the persona file, the compilation, the stance and
  state documents, the canonical JSON, testing a character, bounded learning, `probbit live` and `probbit monitor` (§2.8, §5.7,
  §5.8), what a persona can not do; [examples/persona/](examples/persona/): three personas + goldens.
- [python/](python/): `probbit.py`, a zero-dependency subprocess wrapper (with `evaluate`, the persona functions and a stdlib mock
  judge), its tests and three examples.
- [playground/](playground/): one static page that runs probbit in a browser (`probbit-wasm`, built by `playground/build.sh`).
  [Try the puzzle: find the event that changed the decision.](playground/puzzle.html)
- [bench/](bench/): the scripts behind BENCHMARKS §2, §5 and §8 (Python 3; the ILP baselines need `numpy` and `scipy >= 1.9`).
- [CONTRIBUTING.md](CONTRIBUTING.md), [CHANGELOG.md](CHANGELOG.md).

## License
Apache-2.0.
