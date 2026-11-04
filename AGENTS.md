# AGENTS.md: probbit for agents

probbit: a personality, mood and memory layer for AI agents, on an exact p-bit decision engine (simulated on a CPU).
This file is for agents (and the people running them). Commands first.

## Quickstart (Linux or macOS, POSIX shell, empty directory; no clone, no Rust, no sudo, no API key)

```sh
mkdir probbit-try && cd probbit-try
curl -fsSL https://raw.githubusercontent.com/BitmapAsset/probbit/main/install.sh | sh            # exit 0; ends with a "Next:" list (a "not on your PATH" note is expected)
export PATH="$HOME/.local/bin:$PATH"                                                             # no output
probbit --version                                                                                # probbit 0.13.0 (or later); exit 0
curl -fsSLO https://raw.githubusercontent.com/BitmapAsset/probbit/main/examples/persona/tutor.yaml   # no output; tutor.yaml
probbit persona init tutor.yaml --seed 2 --out tutor.state                                       # wrote tutor.state (Pip 1.1.0, seed 2); exit 0
echo '{"sentiment":"negative"}' | probbit live tutor.yaml --state tutor.state --strand tutor.strand --clock fixed 2>/dev/null | grep -o '"line":"[^"]*"'
#   "line":"Stance: go easy on the jokes; no jokes; gentle; ..."; exit 0
probbit live verify tutor.strand | grep -o '"ok":[a-z]*'                                         # "ok":true; exit 0
printf '%s\n' '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"sh","version":"0"}}}' '{"jsonrpc":"2.0","method":"notifications/initialized"}' '{"jsonrpc":"2.0","id":2,"method":"tools/list"}' | probbit mcp 2>/dev/null | grep -o '"name":"probbit_[a-z_]*"'
#   10 lines: probbit_decide probbit_run probbit_stats probbit_demo probbit_evaluate probbit_persona_init
#   probbit_persona_turn probbit_persona_fuzz probbit_live_event probbit_live_verify; exit 0
```

Other install paths: `npm i -g probbit` (Node 18+); Windows `irm https://raw.githubusercontent.com/BitmapAsset/probbit/main/install.ps1 | iex`;
a different directory: `PROBBIT_INSTALL_DIR=/some/bin` before `sh`.

## Wire it into a harness

```sh
claude mcp add probbit -- probbit mcp                                     # Claude Code
codex mcp add probbit -- probbit mcp                                      # Codex CLI
openclaw plugins install npm:probbit-stance --accept-capabilities         # OpenClaw (changes that OpenClaw's configuration)
```
Cursor and other MCP clients: `{"mcpServers": {"probbit": {"command": "probbit", "args": ["mcp"]}}}`.
Each turn: send the turn's inputs as one event, put the stance `line` into the model's prompt, keep the strand.

## Exit codes

- `0` an answer (`exact`, `diagnostics_passed`, `partial`; every persona turn status)
- `1` `infeasible` (a proof that no plan keeps the rules); `live verify`: a line differs
- `2` bad input or flag (one `{"error"}` object on stdout when piped)
- `3` `refused` / `declined`: the engine cannot vouch; an answer, not a crash
- `4` live: the strand is locked, paused or retired
- `5` live verify: a field this engine does not know (a newer engine's strand)

## Verify a strand

```sh
probbit live verify tutor.strand                       # {"ok":true, "events":N, "verified_by":"probbit X.Y.Z", ...}
probbit live verify tutor.strand --from-checkpoint     # start at the last checkpoint line
probbit monitor tutor.strand                           # watch the individual's state (docs/persona.md §5.8)
```
A strand is append-only JSONL: a header (persona, initial state, engine version), then per event the inputs as used, the
stance and state digests and the sha256 of the line before. Verify replays every event and names the first line that differs.

## Read next

- https://raw.githubusercontent.com/BitmapAsset/probbit/main/docs/CHEATSHEET.md: every command and exit code
- https://raw.githubusercontent.com/BitmapAsset/probbit/main/docs/persona.md: the persona format
- https://raw.githubusercontent.com/BitmapAsset/probbit/main/docs/agents.md: MCP tools, Python, Node, browser
- https://raw.githubusercontent.com/BitmapAsset/probbit/main/docs/adapters.md: Claude Code hooks, OpenAI Agents SDK, LangChain, Vercel AI SDK
- https://raw.githubusercontent.com/BitmapAsset/probbit/main/llms.txt: the index of every doc

## Report a bug

Open an issue at https://github.com/BitmapAsset/probbit/issues with: `probbit --version`, the OS and architecture
(`uname -sm`), the exact command, its exit code, and its stdout/stderr (a strand or persona file if one is involved; no
secrets). A command that does not print what this file says it prints is a bug in this file or in probbit: report it.

## For contributors

Build: `cargo build --release -p probbit-cli` (Rust 1.78+; default: `libm` for amplitude-register math; `nostd` adds `hashbrown` and `portable-atomic`). Test: `cargo test --release --workspace`.
Docs and metadata changes: keep every claim tied to a measurement in BENCHMARKS.md or a test. See CONTRIBUTING.md.
