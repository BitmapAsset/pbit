# Calling probbit from anything

`probbit` is one binary with a JSON contract: a document on stdin, one JSON document on stdout, and an exit code that says
what kind of answer it is. Anything that can start a process can use it: a shell, Python, Node, PowerShell, a CI job, or
an agent harness with a shell tool. No server, no bindings, no network. The document formats are in the README
("Use it from anything": the router document of `probbit decide`) and in [probbit-ir-json.md](probbit-ir-json.md) (programs for
`probbit run`, and the Decision API request of `probbit evaluate`).

| exit | meaning | what to do |
|---|---|---|
| 0 | an answer: verdict `exact`, `diagnostics_passed` or `partial` | act on `released`; escalate `escalated` (empty unless `partial`) |
| 1 | `infeasible`: no plan satisfies the rules (a proof) | relax a rule or a cap; it is an answer, not a crash |
| 2 | bad input: one `{"error": {"code", "path", "message"}}` object on stdout (piped, as an agent runs it; `--json` at a terminal); a bad flag prints one line on stderr instead | fix the document or the flag |
| 3 | `refused` / `declined` (the gate or a cap said no), or `{"error": {"code": "numeric"}}` | escalate the whole decision; any retained candidate plan is diagnostic only |

Decision documents that carry a `plan` also carry `plan_status` (`released`, `partial`, or `diagnostic`)
and `released_plan`, which projects the candidate onto the released IDs. On refusal this projection is empty;
some early failures have no plan at all. The legacy `plan` remains for inspection, not unconditional execution.
A partial projection is **not** necessarily a complete, independently feasible plan: the host must handle
coupled actions and escalate the missing decisions. Exit 0 alone is not permission to execute every candidate action.

Persona/live commands have command-specific verdicts. In particular, live exit 4 means a held writer lock,
a paused individual or a retired individual; do not retry it as though it were a transient engine error.

Every recipe below was run in the cross-platform matrix (`.github/workflows/bench.yml`; transcripts in
`scripts/bench/results/`), on the OSes named under each.

## Shell (bash, zsh, sh)

Run on Linux and macOS (and in Git Bash on Windows).

```sh
probbit demo --tasks 12 | probbit decide --pretty                 # a synthetic 12-task routing document, decided
probbit decide --budget-ms 200 < router.json > decision.json   # your document
case $? in 0) echo answer ;; 1) echo infeasible ;; 2) echo "bad input" ;; 3) echo "refused: escalate" ;; esac
```

Read fields with any JSON tool, e.g. `python3 -c 'import json,sys; print(json.load(sys.stdin)["verdict"])' < decision.json`.

## Python

Run on Linux, macOS and Windows: `python/probbit.py` is stdlib only (Python >= 3.9); `python3 python/test_probbit.py` is its test.

```python
import probbit                                        # python/probbit.py (copy it next to your code, or add python/ to sys.path)
decision = probbit.decide(probbit.demo(tasks=12), budget_ms=200)   # exit 1 and 3 come back as answers
answer = probbit.run(program, deadline_ms=1000)                  # a probbit-ir program; bad input raises probbit.ProbbitInputError
```

The binary is found via `binary=`, `$PROBBIT_BIN`, `probbit` on `PATH`, or `../target/release/probbit` next to `probbit.py`
(on Windows set `PROBBIT_BIN` to the full path of `probbit.exe`).

## Node

Run on Linux, macOS and Windows: [examples/node/decide.mjs](../examples/node/decide.mjs), no dependencies (Node >= 18).
It exports `probbit(args, input)`, which spawns the binary, writes the JSON, and resolves to
`{kind: 'answer' | 'infeasible' | 'bad_input' | 'refused', exitCode, output, stderr}`.

```sh
node examples/node/decide.mjs               # a 12-task demo, then one call per exit code (0, 1, 2, 3)
node examples/node/decide.mjs router.json   # decide your document
```

```js
import { probbit } from './decide.mjs';
const r = await probbit(['decide', '--budget-ms', '200'], problem);   // problem: an object or JSON text
if (r.kind === 'answer' && r.output.plan_status === 'released') act(r.output.released_plan);
else escalate(r); // partial decisions need application-specific handling of coupled actions
```

The npm package (`npm/`) installs the binary and a `probbit` command with exit codes passed through. On Windows, spawn
`probbit.exe` itself (set `PROBBIT_BIN`): Node cannot spawn npm's `probbit.cmd` shim without a shell.

## A host-enforced agent regression

[examples/agent-harness](../examples/agent-harness/README.md) runs a local failing tool behind a real dispatch gate,
records its incident, reproduces a history-dependent retry change, then checks a repaired policy and replays its strand.
It requires no model or credentials. This is a bounded integration example, not a production permissions framework.

## PowerShell

Run on Windows (`windows-latest`) with PowerShell 7.6 as written.

```powershell
probbit demo --tasks 12 | probbit decide | ConvertFrom-Json | Select-Object verdict, violations, ms
Get-Content router.json -Raw | probbit decide --budget-ms 200 | ConvertFrom-Json; $LASTEXITCODE   # 0 / 1 / 2 / 3
```

Windows PowerShell 5.1 (the `powershell.exe` that ships with Windows) adds a UTF-8 byte-order mark to text it pipes into
a program, even with `$OutputEncoding` at its `us-ascii` default. From 0.3.0 probbit skips one leading byte-order mark, so
the same pipes work there directly (0.2.x exited 2: `bad JSON: unexpected character 'ï' at byte 0`). Background, for
0.2.x or for other programs: let `cmd` pipe (`cmd /c "probbit demo --tasks 12 | probbit decide"`), or write UTF-8 without a
mark with `[Console]::InputEncoding = [System.Text.UTF8Encoding]::new($false)` and
`$OutputEncoding = [System.Text.UTF8Encoding]::new($false)`.

## MCP: one line per agent

`probbit mcp` is a [Model Context Protocol](https://modelcontextprotocol.io) server on stdio (JSON-RPC 2.0, one message per
line; stdout carries only protocol messages, logs go to stderr; it exits when stdin closes). Its ten tools take the
commands' own documents and return the commands' own JSON, byte for byte:

| tool | arguments | returns |
|---|---|---|
| `probbit_decide` | a router document (`workers`, `tasks`, `affinity`, `comment`) plus optional `flags` | `probbit decide` |
| `probbit_run` | a probbit-ir program ([probbit-ir.schema.json](probbit-ir.schema.json)) plus optional `flags` | `probbit run` |
| `probbit_stats` | optional `sweeps`, `chains`, `threads` | `probbit stats` |
| `probbit_demo` | optional `tasks`, `seed`, `hard` | `probbit demo` (a router document) |
| `probbit_evaluate` | a System One request plus the optional `probbit` block ([probbit-ir-json.md](probbit-ir-json.md#decision-api-probbit-evaluate)) plus optional `flags` | `probbit evaluate` |
| `probbit_persona_init` | `persona` (the document) or `persona_path`, optional `seed` | `probbit persona init` (the state) |
| `probbit_persona_turn` | `persona` or `persona_path`, `state`, `inputs` (with a drives block: `goals`, the goal signals), optional `flags` (`timing`, `no_inertia`) | `{stance, state}`: `probbit persona turn`'s stance and the state it writes ([persona.md](persona.md); with a drives block the stance has `pursue` and `drives`, §2.9) |
| `probbit_persona_fuzz` | `persona` or `persona_path`, `never` (a rule in habit syntax) or `props` (a list of rules), optional `seeds` (`"0-99"` or a list), `fuzz_seed`, `scripts`, `depth`, `beam`, `grid`, `hours`, `threads` | `probbit persona fuzz --json`: per rule each individual's shortest counterexample ([persona.md](persona.md) §5.6); a counterexample is an answer |
| `probbit_live_event` | `persona` or `persona_path`, `state` (a stored individual) or `seed` (a new one), `event` (its inputs, with `elapsed_hours` = hours since the previous event by the host's clock, and `goals` for a drives block), optional `strand_path` | `{stance, state}`: one event of `probbit live` ([persona.md](persona.md) §5.7): moods (and drives) decay over the hours, feedback moves the learned weights of a persona with a learning block; with `strand_path` the event is appended to that strand on the server's disk (a new file gets the header; an existing one needs the state after its last line) and the answer gets `strand: {path, events, head}` |
| `probbit_live_verify` | `strand` (the strand's text) or `strand_path` | `probbit live verify`: `{"ok": true, events, persona, seed, engine, verified_by, final_state, last_line}`, or `{"ok": false, line, diverges}` for the earliest line that does not replay, or `{"ok": false, line, unsupported, engine, strand_engine}` for the earliest line with a field or line kind this engine does not know (a newer engine's strand: not verified, and not a divergence); answers, not errors |

The persona and live tools run in the server's process and keep nothing between calls: pass the returned state back on the next
turn and put `stance.line` into the model's prompt (after any cached prefix). A refused or fallback stance is an answer; a bad
persona, state, input or event is a tool error with the `{"error": {"code": "persona", ...}}` object. A person can watch the
individual behind a `strand_path` as it grows: `probbit monitor STRAND --serve --open` (a page on 127.0.0.1) or `--follow` in a
terminal ([persona.md](persona.md) §5.8).

`flags` are the command's flags without the dashes: `{"budget_ms": 200, "seed": 3, "summary": true}`; `summary: true`
returns the compact answer (README, "Use it from anything"). `infeasible` and `refused` / `declined` are answers; bad input
and flag errors come back as tool errors (`isError`) with the `{"error"}` object or the flag message.

Protocol: checked against the MCP specification revision 2026-07-28 (the current one on 2026-10-01). Requests that
carry `io.modelcontextprotocol/protocolVersion` in `_meta` are served statelessly (`server/discover`, `tools/list`,
`tools/call`, `ping`); a client that opens with `initialize` (revisions 2025-11-25, 2025-06-18, 2025-03-26, 2024-11-05)
is served by the revision negotiated there. Tested by `python3 python/test_mcp.py` (a dependency-free client over pipes).

| agent | one line | checked |
|---|---|---|
| Claude Code | `claude mcp add probbit -- probbit mcp` | real calls on 2026-10-02 (Claude Code 2.1.284, which opened with `initialize` 2025-11-25): `probbit_demo` then `probbit_decide` with `summary`, 12 and 29 ms; `probbit_evaluate` on the 12-question example with `summary` (found through its tool search): `exact`, the 5 moved answers reported back, 3.3 ms inside the answer |
| Codex CLI | `codex mcp add probbit -- probbit mcp` (writes `[mcp_servers.probbit]` with `command = "probbit"`, `args = ["mcp"]` to `~/.codex/config.toml`) | the entry it writes (codex-cli 0.141.0); no model call |
| Cursor | `.cursor/mcp.json`: `{"mcpServers": {"probbit": {"command": "probbit", "args": ["mcp"]}}}` | not run here |
| mcporter (and harnesses that read its config) | `config/mcporter.json`: `{"mcpServers": {"probbit": {"command": "probbit", "args": ["mcp"]}}}`, then `mcporter call probbit.probbit_demo tasks=3` | `mcporter list probbit` and that call (mcporter 0.7.3) |
| LangChain | a tool around [python/probbit.py](../python/probbit.py): `@tool def route(doc: dict) -> dict: return probbit.decide(doc, summary=True)` | the wrapper's own tests; LangChain itself not run here |

Use the full path of the binary (`probbit.exe` on Windows) where `probbit` is not on the agent's `PATH`.

Beside the agent loop itself (one individual per conversation, one event per model call, the stance block in the system
prompt, a strand as the receipt): [adapters.md](adapters.md) has the host module (`python/probbit_host.py`,
`examples/node/stance-host.mjs`) and the bindings for Claude Code hooks, the OpenAI Agents SDK, LangChain and the Vercel AI
SDK; [receipts.md](receipts.md) says what the strand proves.

## After a judge: `probbit evaluate`

A decision model (a judge) answers each question on its own. `probbit evaluate` takes the judge's request, the judge's
probabilities and your rules over question ids, and returns the most likely answer set that obeys every rule in the judge's
response shape, with odds and the gate's verdict; without rules it returns the judge's own answers. Format and invariants:
[probbit-ir-json.md, "Decision API"](probbit-ir-json.md#decision-api-probbit-evaluate). The example request,
[examples/evaluate/support-12.json](../examples/evaluate/support-12.json), has 12 questions, the judge's answers and 12 rules.
Measured on an Apple M4 (load 2.0-3.6).

Shell:

```sh
probbit evaluate --summary --pretty < examples/evaluate/support-12.json   # exact, 5 of 12 answers moved, 0 violations; 1.24 ms (median of 7)
probbit evaluate --program < examples/evaluate/support-12.json | probbit run # the compiled probbit-ir program: the same answer
```

Any judge that answers the documented request and answer JSON works, local or remote, chosen by you.
Python (stdlib only; the judge is a callable or a URL, called with `urllib`; the key comes from the environment
variable you name):

```python
import probbit
answer = probbit.evaluate(request)          # the request carries the judge's answers (probbit.judge or probbit.weights)
# or let probbit ask the judge first (the request then carries no answers, only its probbit.rules):
answer = probbit.evaluate(request, judge="http://127.0.0.1:8080", auth_env="JUDGE_KEY")   # POST <url>/v1/systemone
answer = probbit.evaluate(request, judge=lambda ask: {"urgent": 0.41, "team": {"billing": 0.48, "technical": 0.44}})
moved = [q for q, a in answer["answers"].items() if a["probbit"]["changed"]]
```

A URL without a path gets `/v1/systemone`; a URL with a path is used as is. When there are no top-level `answers`, a
`result` object is unwrapped unless `success` is false (a judge error). These behaviors are tested against a local mock
(`python/mock_judge.py`); they were not exercised against a remote service. Against the mock: the judge received the request untouched (no `probbit` block), the round trip took
7.0 ms (Python 3.9.6). Judge failures raise `probbit.ProbbitJudgeError`.

MCP: tool `probbit_evaluate` takes the same request with optional `flags` (`{"summary": true}`, `{"program": true}`); over pipes
8.0 ms per call (`python/test_mcp.py` checks it against the CLI).

Browser: `sh playground/build.sh` (needs `rustup target add wasm32-unknown-unknown`), then open `playground/index.html` from
disk: no server, no framework. The page runs the 300-task router demo and the evaluate example with an editable JSON box,
then shows the verdict, the odds table and the timing. The module (`probbit-wasm`, 830,233 bytes) runs the CLI's own code
without threads; at fixed work its documents equal the CLI's at `--threads 4` (test `probbit-wasm/tests/no_threads.rs`). The
300-task demo at `--sweeps 3200 --polish-ms 0`: 531.4 ms in headless Chrome against 346.2 / 132.7 ms native at
`--threads 1` / `4` (medians of 5); the evaluate example 4.6 ms. JavaScript, as the page does it:

```js
const { instance } = await WebAssembly.instantiate(bytes, { probbit: { now_ms: () => performance.now() } });
const call = (op, text) => { const x = instance.exports, input = new TextEncoder().encode(text), p = x.probbit_alloc(input.length);
  new Uint8Array(x.memory.buffer, p, input.length).set(input); const n = x.probbit_call(op, p, input.length);   // 0 decide, 1 run, 2 evaluate, 3 demo
  return JSON.parse(new TextDecoder().decode(new Uint8Array(x.memory.buffer, x.probbit_out_ptr(), n))); };
const answer = call(2, JSON.stringify(request));   // flags go in a "flags" object, as in probbit mcp
```

Agent runtimes with a decision-model slot (see the provider package).

## Check in CI

`probbit check PERSONA --props rules.json [--strand FILE]...` vets a persona's rules (prove, then fuzz the unknown ones) and
replays its strands in one call: exit 0 all held and replayed, 1 a rule broken or a strand failing, 3 only unproven rules,
2 bad input. `--json` gives one document for a script to read. In GitHub Actions the repository is itself an action
(README, "Check in CI").

## For agents with a shell tool

Coding agents with a shell tool call probbit the same way a script does: they run the command, read the JSON, and branch on
the exit code. Nothing needs to be registered. A paragraph like this one in the project's agent instructions
(`CLAUDE.md`, `AGENTS.md`) is enough:

> To route tasks to workers under hard rules (allowed workers, quotas) with odds, write a router document (README, "Use it
> from anything") and run `probbit decide --budget-ms 200 --summary < doc.json`. Exit 0: act on the plan for the released
> items and hand the escalated ones to a human. Exit 1: no plan satisfies the rules. Exit 2: fix the document (the `error`
> object says where). Exit 3: escalate everything. Never read stderr as data. `probbit <command> --help` lists every flag.

Practical notes for agent use:

- Bound the call: `--budget-ms` caps the sampler (with `--budget-ms 300` the 300-task demo answered in 420-495 ms on
  every machine in BENCHMARK-MATRIX.md: the budget, then the gate and the polish), and `probbit run --deadline-ms N` bounds
  a whole `run` call (a target, not a hard limit on a slow CPU: PORTABILITY.md). Small documents (about 12 tasks by 6
  workers) are answered exactly.
- Fixed work is reproducible: `--sweeps N --polish-ms 0` makes the answer a pure function of the document and `--seed`,
  which is what a test or a replayed agent step wants.
- `--summary` keeps an agent's context small: verdict, counts, the gate, the 5 worst released and escalated items and the
  telemetry instead of a plan and odds per item (the 300-task demo at `--sweeps 800`: 2.9 KB instead of 43 KB).
- `probbit stats` prints the machine, the effective controls and a measured self-test; `--threads`, `--cpu-limit` and
  (on Linux and macOS) `--priority low` keep it from crowding the agent's own process.
- `--top`, `demo --live` and the hero screen draw only on a terminal; an agent's pipes never see them.
- A persona's character rules in CI: `probbit persona lint PERSONA --props rules.json` proves each rule or, when the bound
  cannot decide it, fuzzes it (exit 1 when one breaks; exit 3 when one stays unproven: unknown and no counterexample found); `probbit persona fuzz` / `prove` with `--json` give the documents
  (docs/persona.md §5.6). They test the stance a host gets, not the words a model writes.

## How a host identifies itself

Since probbit 0.13.0 (not in 0.12.1). `probbit monitor` heads each strand with who wrote it. Those labels live in a
**sidecar** next to the strand, never in the strand: the strand path with its last extension replaced by `.labels.json`
(`strands/support.strand` -> `strands/support.labels.json`, `pip` -> `pip.labels.json`). The strand's header is the chain's
first link, so a label in it would change every digest; the sidecar can change without touching the receipt.

- Format: one flat JSON object of string -> string, keys sorted, compact, raw UTF-8, one trailing newline
  (`JSON.stringify(sorted) + "\n"`; in Python `json.dumps(d, sort_keys=True, separators=(",", ":"), ensure_ascii=False) + "\n"`),
  written atomically (a temp file in the same directory, then a rename).
- Rules: at most 8 keys; a key is 1-64 of `A-Z a-z 0-9 _ . -`; a value is 1-64 characters with no control character
  (no newline, no tab).
- Keys with a meaning: `agent` (who), `host` (the harness and its version), `lane`, `session`. Any other key is free.
  `session` is a short label for people reading the monitor: never a raw session key, a chat or user id, or anything secret.
- Writing: `probbit live PERSONA --strand F --label agent=support-bot --label host="my harness 2.1"` merges those keys into
  F's sidecar (keys not named stay); `--label` without `--strand` is an error. A host that does not run the CLI for its
  events writes the file itself with the same merge.
- Reading: `probbit monitor F --serve` (or `--dir DIR --serve`, each strand with its own sidecar) shows them;
  `probbit monitor F --label lane=night G --label lane=day --serve` overrides a strand's labels for the page only and
  writes nothing.

What the shipped hosts write:

| host | labels |
|---|---|
| OpenClaw plugin ([plugins/openclaw](../plugins/openclaw/README.md)) | `host` = `OpenClaw <version>` (`OpenClaw` when the runtime gives no version), `lane` = the lane name, `agent` = the `agentName` config key, else the agent id of the session key, `session` = the channel and kind of the session key plus 8 hex of its sha256 (`telegram group #3fa9c2d1`; `#3fa9c2d1` for other shapes); the `labels` config key adds free keys and overrides any of these |
| Claude Code hook ([examples/hosts/claude-code](../examples/hosts/claude-code/README.md)) | `host` = `Claude Code`, `session` = `#` + the first 8 characters of the session id, `agent` = `$PROBBIT_AGENT` when set |
| Python (`probbit.live_event(..., labels=)`, `StanceHost(..., labels=)`) and the LangChain, OpenAI Agents SDK and Vercel AI SDK examples | what the caller passes; the examples pass `host` = the framework's name |
