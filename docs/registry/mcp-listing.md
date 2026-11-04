# probbit MCP server: registry listing (prepared, not submitted)

The fields the MCP registries ask for, filled in once so a listing is a copy-paste. Nothing here has been submitted.
Every claim below matches the README and `probbit mcp`'s own `tools/list` (0.13.0).

## Common fields

| field | value |
|---|---|
| name | probbit |
| display name | probbit: personality, mood and memory for AI agents, and an exact p-bit decision engine |
| short description (≤ 100 chars) | A personality, mood and memory layer for AI agents, plus exact plans under hard rules. Offline. |
| description | probbit gives an agent an individual that lives outside the model: a persona file plus a seed, a stance every turn (exact odds per trait, habits as hard rules) and a replayable, verifiable strand. The same engine, simulated p-bits on a CPU, settles plans under hard rules, gives the odds for every part and refuses when it cannot vouch. One binary; default: `libm` for amplitude-register math; `nostd` adds `hashbrown` and `portable-atomic`. No runtime network or API key. |
| categories / tags | ai-agents, agent-memory, persona, agent-personality, decision-making, constraint-solver, scheduling, developer-tools |
| repository | https://github.com/BitmapAsset/probbit |
| homepage | https://github.com/BitmapAsset/probbit#readme |
| license | Apache-2.0 |
| transport | stdio (JSON-RPC 2.0, one message per line) |
| command | `probbit mcp` |
| install | `curl -fsSL https://raw.githubusercontent.com/BitmapAsset/probbit/main/install.sh \| sh` (Linux, macOS), `npm i -g probbit` (Node 18+), or `irm https://raw.githubusercontent.com/BitmapAsset/probbit/main/install.ps1 \| iex` (Windows) |
| protocol revisions | serves 2026-07-28; `initialize` also accepts 2025-11-25, 2025-06-18, 2025-03-26, 2024-11-05 |
| environment variables | none required; no API key |
| network | none (`openWorldHint: false` on every tool) |
| platforms | Linux x86_64 and arm64, macOS (Apple silicon and Intel), Windows x86_64 |
| icon | docs/brand/icon-512.png |
| author / maintainer | the repository owner (BitmapAsset) |

## Client configuration

```json
{"mcpServers": {"probbit": {"command": "probbit", "args": ["mcp"]}}}
```
Claude Code: `claude mcp add probbit -- probbit mcp`. Codex CLI: `codex mcp add probbit -- probbit mcp`.
Through npm without a global install: `{"command": "npx", "args": ["-y", "probbit", "mcp"]}` (not yet tested in a client).

## Tools (10)

| tool | what it does | read-only |
|---|---|---|
| `probbit_decide` | assign every task to one worker under hard rules (allowed sets, quotas, clamps) with workflow affinity: a plan that keeps every rule, odds per task, a verdict | yes |
| `probbit_run` | a general constrained categorical program (variables, values, scores, pairs, caps, all_different, implies, tables, precedes, linear): the same answer shape with marginals | yes |
| `probbit_stats` | machine, build features, effective controls and a measured self-test | yes |
| `probbit_demo` | a seeded synthetic agent-routing document to pass to `probbit_decide` | yes |
| `probbit_evaluate` | a decision model's answers plus your rules over them: the rule-abiding answer set with odds | yes |
| `probbit_persona_init` | a persona and a seed: the individual's initial state | yes |
| `probbit_persona_turn` | a persona, the individual's state and this turn's inputs: the stance (a level per trait with exact odds, the habits in force, the stance line for the prompt) and the new state | yes |
| `probbit_persona_fuzz` | a persona and a character property: a search for a counterexample across individuals | yes |
| `probbit_live_event` | one event of a resident individual, optionally appended to a strand | no (writes the strand when one is given) |
| `probbit_live_verify` | a strand: replays every line and reports `ok` or the first line that differs | yes |

## Registry-specific notes (for whoever submits)

- **Official MCP registry** (registry.modelcontextprotocol.io, `mcp-publisher`): needs a `server.json` and proof of the
  package. For the npm package the registry checks an `mcpName` field in the published `package.json`
  (`"mcpName": "io.github.BitmapAsset/probbit"`); that field is not in 0.13.0, so it needs a package release first.
  Draft `server.json` (check the current schema URL with `mcp-publisher init` before submitting):

  ```json
  {
    "name": "io.github.BitmapAsset/probbit",
    "description": "A personality, mood and memory layer for AI agents, plus exact plans under hard rules. Offline.",
    "repository": {"url": "https://github.com/BitmapAsset/probbit", "source": "github"},
    "version": "0.13.0",
    "packages": [
      {
        "registryType": "npm",
        "identifier": "probbit",
        "version": "0.13.0",
        "transport": {"type": "stdio"},
        "packageArguments": [{"type": "positional", "value": "mcp"}]
      }
    ]
  }
  ```
- **Smithery**: a local stdio server; the listing takes the description, the client configuration above and the tool list.
- **mcp.so** and similar directories: name, description, repository URL, categories, the client configuration.
- Check before submitting: the step 8 command in the README ("For AI agents reading this") lists the same 10 tools.
