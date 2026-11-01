# probbit cheat sheet (0.14.0)

The CLI's own help and docs, shortened; `probbit <command> --help` has every flag.

## Help, completions, the manual page
| command | does |
|---|---|
| `probbit --help` | every command by group (persona, live, monitor, decide/run, mcp, other), the exit codes, six examples |
| `probbit help COMMAND [SUBCOMMAND]` | the same text as `probbit COMMAND [SUBCOMMAND] --help`: usage, flags with defaults, exit codes |
| `probbit completions bash\|zsh\|fish` | a completion script for the shell (commands, subcommands, flags); install lines in its `--help` |
| `probbit man` | the manual page (roff); `install.sh` puts it in `~/.local/share/man/man1/` (`<prefix>/share/man/man1/` for `PROBBIT_INSTALL_DIR=<prefix>/bin`), so `man probbit` works |

Help, completions and the man page all come from one command table, so they list exactly the flags the parser accepts. A
mistyped command or subcommand exits 2 with the closest name: `did you mean "persona"?`.

## Verdicts (`decide`, `run`, `evaluate`; a persona turn's `engine.verdict`)
| verdict | means | exit |
|---|---|---|
| `exact` | odds are exact; `tier` says how | 0 |
| `diagnostics_passed` | sampled; every released item passed the diagnostics gate (not a proof) | 0 |
| `partial` | act on `released`, escalate the rest | 0 |
| `refused` | the sampler cannot vouch for the answer; escalate | 3 |
| `infeasible` | no plan satisfies the rules (a proof) | 1 |

`declined` (exit 3): `decide --mode exact` or `run --op exact` stopped before an answer.

## Tiers (`tier`, the method that answered)
The exact tiers run before the sampler:
- `enumerate`: every feasible plan listed (up to `--exact-limit`, default 2,000,000);
- `frontier`: a dynamic program over loads, when groups share few resources (`--frontier-states`, default 4096);
- `occupancy` / `forest` / `components`: independent parts solved one by one (two-value groups by a count dynamic
  program, cap-free trees by sum-/max-product, a mix);
- `sample`: constraint-preserving Gibbs sampling + the diagnostics gate (`decide` omits `tier` when it samples).

## A persona stance
- `line`: the stance line. The host puts it into the model's prompt for this turn; an agent can read it to know its own
  stance; it is not addressed to the end user.
- `habits.active`: habits in force this turn. `habits.bound`: the active habits the habit-free twin breaks, i.e. the ones
  that changed the stance. Empty = no habit had to change anything this turn.
- `status`: `ok` (exact or diagnostics_passed), `partial`, `refused` (the line: just the habits), `fallback`.
- `pursue` (a persona with a `drives:` block): the goal that gets the next unit of effort, with exact odds under the habits;
  `drives`: per goal the wanting, afterglow, expectation, last prediction error, deadline and turns since it was pursued.
- `senses` (a persona with a `senses:` block, docs/persona.md §2.11): per sense read this turn its `raw` reading, `src`, `z`
  and `zd`, and the `mean` and `n` of the normal it was judged against; `noticed`: the sense `why` names.
- `credit` (a persona with a `learning.credit` block, docs/persona.md §2.8): `paid` (`turn`, `e`, `sign`, `absorbed`,
  `stance`) or null, `closed` (`turn`, `as`: control, expired, cancelled, paid, evicted), `open` (turns), `stance` (this
  stance's digest).
- `needs` (a persona with a `needs:` block, docs/persona.md §2.12): per need its `deficit` after this event, the `forecast`
  and the `term` the `pursue` field read; `drives.window`: per goal with `expect_by_hours` the hours left, or null.
- `pace` (a persona with a `pace:` block, docs/persona.md §2.14): `level` (slow / steady / brisk) and `rbar`, the average
  reward after this event.
- `surprise` (a persona with a `surprise:` block, docs/persona.md §2.13): per learned trait its gain g while above 1.
- `glyph`, `glyph_why` (since 0.13.0; a persona with `display.glyphs`, docs/persona.md §2.18): one emoji from the stance by rule order
  (a habit in force, else the mood table cell, then the surprise modifier); `live` strand lines carry `glyph`, `verify`
  recomputes it.

## Senses (0.9.0, docs/persona.md §2.11)
- Event: `{"senses": {"tool_fail": true, "tool_latency": {"v": 2.4, "src": "env:runner"}}, "src": "env:host"}`: a bare
  reading takes the event's `src`; each sense lists its sources (`env`, `human`; never `self` or `clock`).
- Per reading: z against the learned normal (mean, variance, count), capped at `zcap`; beyond the `dead_zone` it moves the
  sense's `effects`. The mean stays within `prior.mean ± drift_cap`, the variance within `[0, var_cap]`.
- Rules read `sense.<id>.raw` (holds whatever the history) or `sense.<id>.z`, never the normal.
- Refused: undeclared senses, out-of-range numbers, flags that are not true/false, a source the sense does not list, readings
  through the MCP turn tools. A refused event changes nothing.
- `live verify` summary: `normals` per sense (`mean`, `n`, `max_move_50`); `monitor`: a row per sense and an alert when a
  normal moved over `drift_cap / 2` within 50 events.

## Action-addressed feedback (0.9.0, docs/persona.md §2.8)
- Block: `learning: {..., credit: {max_open: 8, half_life_events: 32}}`.
- Events: `{"await": true}` (the host will report this decision's outcome); `{"praise": true, "for": 3, "src": "env:ci"}` or
  `"for": {"turn": 3, "stance": "sha256:..."}` (pays decision 3; the digest binds it); `{"cancel": [4], "src": "env:executor"}`.
- Eligibility e = 0.5^((t − n − 1) / half_life_events), 6 decimals: 1, 0.937084, 0.805245, 0.5 at 0, 3, 10, 32 events late;
  a decision expires 128 events after it was made; beyond `max_open` the oldest is evicted. e scales the capped 0.8 step.
- Refused whole: ambiguous unaddressed feedback (B-R1), a decision not open (B-R2: paid, cancelled, expired, evicted, never
  opened, to come), `for` without feedback (B-R3), a bad cancel (B-R4), malformed keys or a cancel from `self` (B-R5), a
  digest that is not the turn's stance (B-R6). A control line closes every open decision; MCP refuses `cancel`.
- `live verify` summary: `open`; `live verify STRAND --actions FILE`: lines `{"turn": T, "action": {trait: level}}`, the
  earliest one that differs from the released level (exit 1).

## Host contract (docs/persona.md §7)
- One outcome per stance: a stance is paid once; a second outcome addressed to it is refused ("turn N is neither open nor the
  previous stance"). One event with `await: true` per tool call, each outcome `for` its own stance.
- No event id: a duplicate observation event (a retried hook, a redelivered callback) is appended and counted twice. Keep an
  idempotency key per event (your event id -> the receipt: the stance's `turn`, the new state's `digest`); never resend an
  accepted event.
- Restart time: `--clock real` starts with the process and refuses an event's own `elapsed_hours`, so downtime is not counted;
  to count it, run `--clock fixed` and stamp `elapsed_hours` from your own clock (MCP and Python's `live_event` always do).

## Needs, omission windows, valence (0.10.0, docs/persona.md §2.12, §2.9)
- Block: `needs: {drive: {"n": 4, "m": 3}, r_cap: 1, items: [{id, unit, cap, weight, per_hour, satisfy_from, amount, bind}]}`
  (quote `"n"`: YAML 1.1 reads a bare n as false); the bound goal declares `expect_by_hours`.
- Event: `{"needs": {"context": {"amount": 0.6, "src": "env:compactor"}}, "src": "env:host"}`: a bare amount takes the event's
  `src`, which must be in the need's `satisfy_from` (`env`, `human`; never `self` or `clock`).
- Per event: every need grows (per stamped hour, per raw reading of a number sense), then each satisfaction pays
  r = D(x) − D(x with x_j ← |x_j − a|) as a win (or a setback) of its bound goal; D = (Σ w x^4)^(1/3) at n 4, m 3: amount 0.2
  pays 2.0229× more at deficit 0.8 than at 0.2, and −0.117 at deficit 0.
- `pursue` gains the term want × s × (expected reduction), capped at `pursue.cap`; s, the goal's success rate, rises on a win
  and falls × (1 − rate) when the goal's window expires unanswered. Rules read `need.<id>.deficit`, never the term.
- Windows: open after a released stance pursues the goal, count down by stamped hours, close on a win, fire once at 0:
  d = −min(E, pe_cap), E ← E × (1 − rate); `why`: "an expected win did not come"; not a learning signal.
- `drives.valence: {mood, expect, surprise}` (0 < expect < surprise): mood evidence = expect × E of the goal pursued before
  + surprise × the turn's prediction error; `afterglow.liking` adds a gain on the win itself.
- Refused: N-R1–N-R6 when the persona is read; amounts that are undeclared, ≤ 0, above cap, twice, from a source not listed,
  with the bound goal's own win (N-E3), or engine-owned ids sent as inputs (N-E4); amounts through the MCP turn tools.
- `live verify` summary: `needs` per need (`deficit`, `at_cap_hours`); `monitor`: a NEEDS row per need, its window, an
  at-cap alert; `lint`: horizons whose forecast passes K/2 before the deficit does, habits that exclude a bound goal.

## Surprise, pace, dispositions (0.10.0, docs/persona.md §2.13 to §2.15)
- `surprise: {volatility_rate: 0.3, v0: 0.45, r_ref: 4, r_max: 20, g_max: 3}` (needs a learning block; the defaults shown):
  per learned trait a gain g in [1, g_max] multiplies the step and its cap; a run of contrary feedback raises it, consistent
  feedback brings it back to 1; a new learner starts closed at g 1 (0.11.0). Stance `surprise: {trait: g}` while g > 1; `why`: "learning
  fast: feedback changed". `precision: true` (with senses) scales each sense's effect by 1 − var / var_cap.
- `pace: {half_life_hours: 6, cap: 4, r0: 1, levels: [0.5, 1.5], effects: {initiative: 0.5, verbosity: -0.3}}` (needs
  `reward_from`): R̄ ← clip(R̄ × 0.5^(h/6) + 0.109101 × r_t, 0, 4) after the drive step, r_t = the accepted wins + 1 per
  reward flag on; stance `pace: {level, rbar}` (slow / steady / brisk); inputs `pace_up`, `pace_down` (the engine's);
  `lint` warns when Σ|effects| × max(cap − r0, r0) > 1.
- `dispositions: {optimism_spread, patience_spread, volatility_spread}` (0 to 3): genes o (rate × e^±o on the learned step
  and on E), p (× the credit half-life and the deadline τ) and an offset on `surprise.v0`; a spread of 0 stores nothing.
- Refused: V-R1, V-R2, P-R1–P-R3, D-R1, D-R2 when the persona is read; `pace_up` or `pace_down` sent as inputs. `live verify`
  summary: `pace` (`rbar`, `max`), `surprise` (`g_max_seen` per trait); `monitor`: a PACE row (R̄ with its r0 tick) and a
  GAIN row per learned trait.

## Exit codes
- `decide`, `run`, `evaluate`: 0 answer (exact | diagnostics_passed | partial), 1 infeasible (a proof), 2 bad input (one
  `{"error"}` object on stdout, see Errors) or bad flag (stderr), 3 refused / declined / non-finite result.
- `persona`: 0 done; 1 lint found an unresolved contradiction or (with rules) a counterexample, fuzz found a counterexample or
  prove left a rule unknown; 2 bad persona / state / inputs / script / rule, or bad flag.
- `live`: 0 done; 1 verify: a line differs; 2 a bad persona, state, event or flag; 4 refused (the writer lock, a pause, a
  retirement); 5 verify: unsupported, a line has a field or line kind this engine does not know (a newer engine's strand: not
  verified, and not a divergence).
- `check`: 0 every rule held and every strand replays; 1 a rule broken, an unresolved contradiction, a strand that fails or one
  this engine cannot verify (unsupported) (1 wins); 2 a bad persona, rule file or flag; 3 the only problem is an unproven rule.
- `monitor`: 0 every line replays; 1 a line differs; 2 a bad flag, a port that cannot be had, or a file that is not a strand.

## Errors
One channel, the same for every command (0.8.2):
- **stderr**: one human line, always: `probbit: bad JSON: expected string at byte 1`.
- **stdout**, bad input (a document, persona, state, event; numeric failures; live refusals): one
  `{"error": {"code", "path", "message"}}` object when stdout is **not a terminal** (a pipe, a file, every wrapper: Python, MCP,
  npm, the examples), or with **`--json`**. At a terminal without `--json`: the line only, never both.
- **stdout**, bad command or flag (unknown command, subcommand or flag; a bad flag value): nothing, so a piped caller tells it
  from bad input by an empty stdout. With `--json`: `{"error": {"code": "flag", ...}}`, or for an unknown command or
  subcommand `{"error": {"code": "command", ..., "suggestion": "persona"}}` (`null` when nothing is close).
- `--json` is accepted by every command; it changes no answer (for `persona fuzz` / `prove` it also selects the JSON report).
- An unknown command or subcommand names the closest one: `probbit: unknown command "persna"; did you mean "persona"?`

## `probbit persona <sub> PERSONA`
| sub | does |
|---|---|
| `init [--seed N] [--out STATE]` | a new individual (genes from the seed, resting stance) |
| `turn --state STATE [--inputs JSON\|FILE]` | the stance on stdout; the new state replaces STATE |
| `replay [--seed N] --script JSON\|FILE` | init, then every turn of the script: one stance per line |
| `explain [--seed N] --script ... --turn K` | turn K in words: every contribution, the odds, the twin |
| `diff [--seed A] [--other P2] [--seed2 B] --script ...` | distance between two individuals |
| `lint [--never RULE \| --props FILE]` | contradicting habits; with rules: prove, then fuzz the unknown ones |
| `fuzz (--never RULE \| --props FILE) [--seeds 0-99]` | the shortest event script whose stance breaks a rule |
| `prove (--never RULE \| --props FILE) [--seeds 0-99]` | held by construction, proved for every event sequence, or unknown |
| `check` | valid? digest and sizes |
| `compile --state STATE [--inputs ...]` | the turn's probbit-ir program |
| `describe` | traits, moods, inputs, habits, agenda |

## `probbit live`, `probbit monitor`
| command | does |
|---|---|
| `live PERSONA [--seed N \| --state FILE] [--strand FILE] [--events FILE]` | JSONL events in, one stance per event out |
| `live PERSONA --demo week [--seed N] [--plain]` | a scripted week: learning to a cap, a night, rules that hold |
| `live verify STRAND [--actions FILE]` | replay a strand; the earliest line that differs (and host action, with `--actions`), or the earliest line this engine cannot read: `unsupported by this engine: FIELD`, exit 5 |
| `monitor STRAND [--follow] [--once] [--plain] [--serve] [--open] [--port N]` | the strand replayed as live bars in the terminal, or the Activity Monitor page on 127.0.0.1 (channel table, event-history traces, inspector) |
| `monitor --demo [week\|drives] [--open]` | synthetic data: the tutor's week, paced, in under a minute; `drives`: a short goal-signal scenario |

## `probbit check PERSONA` (0.9.0): one gate for CI
`probbit check PERSONA [--props FILE] [--strand FILE]... [--seeds 0-99] [--depth 8] [--hours 1,12,48] [--json]` runs `persona
lint`'s pipeline on the rules (prove, then fuzz the unknown ones) and `live verify` on every strand. One line per rule
(`held`, `held_by_construction`, `broken seed N`, `unproven`), one per strand (`ok N events` / `FAIL line K: ...` /
`line K: unsupported by this engine: FIELD (written by ENGINE)`), then
`check: held H · broken B · unproven U · strands S/T`. `--json`: `{persona, props, seeds, depth, hours, rules, strands,
verdict}`, where `rules` is the `persona lint` document. In GitHub Actions: `uses: BitmapAsset/probbit@<ref>` (README,
"Check in CI").

## Testing a character
- `lint`: contradicting habits; with `--props`, every rule proved or fuzzed;
- `fuzz`: "none found" is evidence over the scripts searched, not a proof;
- `prove`: `held by construction` (a habit implies it), `proved for every event sequence` (a sound bound) or `unknown`;
- `live verify`: a strand replays byte for byte.
