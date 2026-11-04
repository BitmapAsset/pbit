# probbit

**A personality, mood and memory layer for AI agents, on an exact p-bit decision engine.** Every stance is computed on a
CPU, keeps its rules by construction, and leaves a replayable, verifiable record.

![probbit: simulated p-bits on a CPU settle an AI agent's stance and plans under hard rules](https://raw.githubusercontent.com/BitmapAsset/probbit/main/docs/probbit-hero-1600x900.jpg)

[![ci](https://github.com/BitmapAsset/probbit/actions/workflows/ci.yml/badge.svg)](https://github.com/BitmapAsset/probbit/actions/workflows/ci.yml)
[![npm](https://img.shields.io/npm/v/probbit)](https://www.npmjs.com/package/probbit)
[![license](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](https://github.com/BitmapAsset/probbit/blob/main/LICENSE)

## What it is, in 20 seconds

- **An AI persona outside the model.** A persona file and a seed make one individual: personality, mood with inertia (an
  affective-computing model) and memory as learned adjustments within caps. The LLM writes the words; probbit gives it
  the stance line for each turn.
- **Exact odds under hard rules, held by construction.** Simulated p-bits (probabilistic computing on an Ising/QUBO
  model) settle the stance or plan; small programs are solved exact, larger ones sampled with an error bar. It is also a
  constraint solver for plans: tasks to workers, every rule kept, or a refusal (exit 3) instead of a guess.
- **A receipt.** Every turn is a line on a hash-chained strand; `probbit live verify` replays it byte for byte.

For **agent builders** (a CLI, an MCP server, Node and Python), **OpenClaw users** (the OpenClaw plugin
`probbit-stance`), and **researchers** (every number reproducible from the repository).

## Install and try it in 30 seconds

```sh
npm install -g probbit
```

Then, in an empty directory (POSIX shell, Linux or macOS; no clone, no Rust, no API key). Each line ends with what to
expect; every one exits 0.

```sh
probbit --version                                     # probbit 0.13.0 (or later)
probbit demo --tasks 12 | probbit decide --summary    # one JSON line with "verdict":"exact" and "violations":0
curl -fsSLO https://raw.githubusercontent.com/BitmapAsset/probbit/main/examples/persona/tutor.yaml   # tutor.yaml appears
probbit persona init tutor.yaml --seed 2 --out tutor.state                                          # wrote tutor.state (Pip 1.1.0, seed 2)
echo '{"sentiment":"negative"}' | probbit live tutor.yaml --state tutor.state --strand tutor.strand --clock fixed 2>/dev/null | grep -o '"line":"[^"]*"'   # "line":"Stance: go easy on the jokes; no jokes; gentle; ...
probbit live verify tutor.strand | grep -o '"ok":[a-z]*'                                            # "ok":true
printf '%s\n' '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"sh","version":"0"}}}' '{"jsonrpc":"2.0","method":"notifications/initialized"}' '{"jsonrpc":"2.0","id":2,"method":"tools/list"}' | probbit mcp 2>/dev/null | grep -o '"name":"probbit_[a-z_]*"'   # 10 lines, "name":"probbit_decide" ... "name":"probbit_live_verify"
```

If npm says the install script did not run (`--ignore-scripts`, or a newer npm's script approval), that is fine: the first
`probbit` call fetches and checks the binary. Any other output: [open an issue](https://github.com/BitmapAsset/probbit/issues).

## How one decision flows

![how one decision flows through probbit: JSON in, a program with hard rules, the exact unit or the p-bit sampling unit, the diagnostics gate, the answer back](https://raw.githubusercontent.com/BitmapAsset/probbit/main/docs/probbit-flow.png)

A request arrives as JSON (a router document, a probbit-ir program or a persona event) and is normalised into one
program: a weight for every choice plus the hard rules (allowed values, quotas, habits). When the rule-abiding plans are
few enough to count (up to about 2 million), the exact unit enumerates every one and returns exact odds; otherwise
simulated p-bits sample the program with constraint-preserving Gibbs moves, so every sample keeps every rule by
construction. A diagnostics gate (split R-hat, an error bar per part, a frozen-worker check) then releases the answer,
releases what passed and escalates the rest (`partial`), or refuses (exit 3) when it cannot vouch. The answer carries its
receipt: the gate statistics and seed for a decision, a sha256-chained strand line for a persona turn.

## Use it from your harness

| harness | one line to run | what proves it |
|---|---|---|
| [MCP server](https://github.com/BitmapAsset/probbit/blob/main/docs/agents.md) (Claude Code, Codex, Cursor, any MCP client) | `probbit mcp` (Claude Code: `claude mcp add probbit -- probbit mcp`) | the last line of the block above: 10 tools |
| [OpenClaw plugin](https://github.com/BitmapAsset/probbit/blob/main/plugins/openclaw/README.md) | `openclaw plugins install npm:probbit-stance --accept-capabilities` (changes that OpenClaw's configuration) | `openclaw plugins inspect probbit-stance --runtime --json` lists both hooks |
| [Node](https://github.com/BitmapAsset/probbit/blob/main/examples/node/decide.mjs) | `curl -fsSLO https://raw.githubusercontent.com/BitmapAsset/probbit/main/examples/node/decide.mjs && probbit demo --tasks 12 > router.json && node decide.mjs router.json` | prints `exit 0 answer: verdict exact, 12 of 12 released, ...` |
| [Python or any language](https://github.com/BitmapAsset/probbit/blob/main/docs/agents.md) | JSON on stdin, JSON on stdout, the exit code is the verdict; a stdlib-only wrapper: [python/probbit.py](https://github.com/BitmapAsset/probbit/blob/main/python/probbit.py) | `python3 -c 'import json,subprocess as s; d=s.run(["probbit","demo","--tasks","12"],capture_output=True).stdout; r=s.run(["probbit","decide"],input=d,capture_output=True); print(r.returncode, json.loads(r.stdout)["verdict"])'` prints `0 exact` |
| [Browser (wasm)](https://github.com/BitmapAsset/probbit/tree/main/playground) | in a clone, with Rust and the `wasm32-unknown-unknown` target: `playground/build.sh` | `playground/probbit.wasm` exists; open `playground/index.html` |
| [CI](https://github.com/BitmapAsset/probbit/blob/main/docs/CHEATSHEET.md) | `probbit check tutor.yaml --strand tutor.strand` | ends `check: held 0 · broken 0 · unproven 0 · strands 1/1`, exit 0 |

Agent SDK adapters (Claude Code hooks, OpenAI Agents SDK, LangChain, Vercel AI SDK):
[docs/adapters.md](https://github.com/BitmapAsset/probbit/blob/main/docs/adapters.md).

## For AI agents reading this

- `probbit` is one offline binary (3 to 4 MB; default: `libm` for amplitude-register math; `nostd` adds `hashbrown` and `portable-atomic`): JSON in, JSON out, the exit code is the verdict
  (0 answer, 1 infeasible or a strand that differs, 2 bad input, 3 refused).
- Proof that it works here: the seven lines under "Install and try it", each with its expected output.
- The full contract: [docs/agents.md](https://github.com/BitmapAsset/probbit/blob/main/docs/agents.md),
  [AGENTS.md](https://github.com/BitmapAsset/probbit/blob/main/AGENTS.md),
  [llms.txt](https://github.com/BitmapAsset/probbit/blob/main/llms.txt), every command and exit code in
  [docs/CHEATSHEET.md](https://github.com/BitmapAsset/probbit/blob/main/docs/CHEATSHEET.md).
- Put the stance `line` of each turn into the model's prompt; the persona format is in
  [docs/persona.md](https://github.com/BitmapAsset/probbit/blob/main/docs/persona.md).
- The front door for people and agents: <https://bitmapasset.github.io/probbit/>.

## FAQ

- **Is probbit the same as probit?** No. probbit (two b's) is a p-bit engine: a p-bit is a probabilistic bit, 0 or 1 with
  a probability you set, and coupled p-bits sample whole configurations together. It is unrelated to the probit model in
  statistics, to Power BI `.pbit` files and to any exchange or company of a similar name.
- **Does it need a GPU, a network or an API key?** No. It runs offline on any CPU as one binary; the default build uses `libm` for amplitude-register math
  (`nostd` adds `hashbrown` and `portable-atomic`). Nothing phones home; this npm install step downloads the binary.
- **Can a stance be verified later?** Yes. `probbit live verify STRAND` replays the strand, checks every sha256 digest and
  names the first line that differs (exit 1).
- **Does the engine experience its moods?** No. A mood is a declared state with exact numbers that replays byte for byte: a
  mechanism and a simulation, not an experience.
- **How does it relate to the language model?** It runs beside the model, not inside it: the model writes the words,
  probbit decides the stance and what the rules allow, so you can swap the LLM and keep the individual.

## The npm wrapper

`npm install` fetches the release archive for your platform from GitHub Releases (`v` + this package's version): macOS
arm64 and x86_64, Linux x86_64 (glibc 2.34 or later, otherwise the static musl build), Linux ARM64 (static musl),
Windows x86_64 (Windows on Arm runs it under x64 emulation). It checks the archive against the `.sha256` file published
beside it and keeps the binary inside this package; a checksum mismatch fails the install and installs nothing. If the
install step could not run (`--ignore-scripts`, offline), the first `probbit` call fetches the binary.

| variable | effect |
|---|---|
| `PROBBIT_VERSION` | release tag to fetch (default: `v` + this package's version) |
| `PROBBIT_DOWNLOAD_BASE` | fetch `$PROBBIT_DOWNLOAD_BASE/<tag>/probbit-<tag>-<target>.tar.gz` (`.zip` on Windows) instead of GitHub |
| `PROBBIT_TARGET` | Rust target triple to fetch instead of the detected one |
| `PROBBIT_BINARY` | at install: copy this binary instead of downloading; at run time: run this binary |

`probbit`'s exit code passes through unchanged: 0 answer (`exact`, `diagnostics_passed`, `partial`), 1 `infeasible`
(a proof), 2 bad input or flag, 3 `refused` / `declined`. Node 18 or later; the binary uses `libm` (`nostd` adds `hashbrown` and `portable-atomic`). License: Apache-2.0.

[Repository](https://github.com/BitmapAsset/probbit) ·
[front door](https://bitmapasset.github.io/probbit/) ·
[CHANGELOG](https://github.com/BitmapAsset/probbit/blob/main/CHANGELOG.md) ·
[issues](https://github.com/BitmapAsset/probbit/issues)
