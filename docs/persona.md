# probbit persona: the individuality layer (format 1)

A **persona** gives an agent a stable individual temperament that lives outside the language model. It is a small file of
traits, moods, habits and evidence rules. Every turn it compiles, with the agent's state and the turn's inputs, into **one
probbit IR program**. probbit answers with the **stance** for that turn: a level for every trait with exact odds, the habits
that were in force and the ones that changed the outcome, a refusal when the engine cannot vouch, a one-line explanation,
and a short **stance line** (at most 40 tokens by default) that the host puts into whatever model writes the reply.
The stance line is addressed to the model: the host puts it into the model's prompt for this turn (section 7), so the model
reads it as its instructions for how to write; an agent can read it to know its own stance; it is not addressed to the end user.

- **Same persona file + a different seed = a different, stable individual.** The seed fixes each individual's small,
  permanent offsets ("genes"); the same individual is recognisable on inputs it has never seen.
- **Swap the model, keep the individual.** The stance is computed without any model in the loop, so it is the same for every
  model and every host, and it can be replayed byte for byte.
- **A persona does not write text.** It decides *how* the text is written (warm or plain, one line or a breakdown, a joke or
  none, verify first, ask first). The model still writes every word.

This document is normative for the file format, the compilation, the stance document and the state. It is implemented by
`probbit persona` (probbit-persona/src/persona.rs and yaml.rs, no dependency) and served on every surface with the same documents: the
CLI, `probbit mcp` (tools `probbit_persona_init`, `probbit_persona_turn`), the Python wrapper (`probbit.persona_init`,
`persona_turn`, `persona_replay`) and the browser module (probbit-wasm ops 4 and 5). `probbit live` (section 5.7, live.rs) keeps an
individual running on a clock, with learning (section 2.8) and a replayable log; `probbit monitor` (section 5.8, monitor.rs)
draws its inner state live from that log. The example personas and their goldens are in
[examples/persona/](../examples/persona/). An independent reference implementation of this format (Python) gives the same
documents byte for byte on every example turn (section 5.3).

```sh
probbit persona init examples/persona/tutor.yaml --seed 2 --out pip.json                       # an individual: genes from the seed
probbit persona turn examples/persona/tutor.yaml --state pip.json --inputs '{"loss": true}'    # this turn's stance; pip.json moves on
```

Contents: 1 Files · 2 Schema · 3 Compilation · 4 The stance document · 5 State, canonical JSON, replay, testing, live · 6 What a
persona can NOT do · 7 Commands and surfaces · 8 A model-proposed stance (the `evaluate` bridge) · 9 Versioning.
On one page (verdicts, tiers, `bound`, exit codes, every command): [CHEATSHEET.md](CHEATSHEET.md).

## 1. Files

A persona is one document, written either in **JSON** or in the **YAML subset** below; both forms parse to the same object
and have the same digest (`sha256` of the canonical JSON: keys sorted, no whitespace, UTF-8).

**YAML subset.** Block mappings, block sequences (including `- key: value` items), one-line flow collections (`[a, b]`,
`{a: 1, b: [x, y]}`), plain / single-quoted / double-quoted scalars (double-quoted ones take JSON's escapes), numbers,
`true` / `false` / `null` / `~`, and `#` comments. Rejected with a line number, never guessed: tabs in indentation, anchors,
aliases, tags, block scalars (`|`, `>`), flow collections spanning lines, duplicate keys, a second document, numbers that are not
exact in a double (an integer beyond 2^53, `1e999`), and plain keys another YAML reader would not read as strings (`yes`, `no`,
`on`, `off`, `y`, `n`, `true`, `false`, `null`, numbers: quote them). A file that uses only this subset means the same thing to
every YAML 1.2 reader. A persona file whose name ends in `.json` is read as JSON (duplicate keys refused), any other as the subset.

The document is **strict**: every field below is type-checked; an unknown field is an error at its path (e.g.
`traits[2].sprad: unknown field`), as for every probbit document. Habit rules are checked when the file is read, as strictly as
`probbit run` reads them, so a malformed rule fails at once and not only on the turns it is in force. The same holds for a turn's inputs and for the state: a bad
value is one `{"error": {"code": "persona", "path", "message"}}` object and exit code 2, never a silent coercion.

## 2. Schema

```yaml
probbit_persona: 1                  # format version (required)
identity:                           # required
  name: Rook                        # 1-64 chars: letters, digits, space, _ . -   (genes are keyed by name + seed)
  version: 1.0.0                    # a string
  seed: 1                           # the default individual (integer >= 0); `init --seed N` makes another one
  summary: a cautious ops engineer  # carried, never read by the engine
traits: [...]                       # required, at least one (section 3)
moods: [...]                        # optional
couplings: [...]                    # optional soft links between traits / moods
inputs: [...]                       # optional per-turn evidence
history: [...]                      # optional features derived from past turns
habits: [...]                       # optional hard rules
agenda: {...}                       # optional ordered beats of a reply
engine: {...}                       # optional engine settings
line: {...}                         # optional stance-line settings
learning: {...}                     # optional bounded learning from feedback (section 2.8)
drives: {...}                       # optional goals with wanting, afterglow and expectation; the `pursue` output (section 2.9)
reward_from: [human, env]           # optional: the sources a reward may come from (section 2.10)
comment: any text                   # ignored
```

Identifiers (`id` of traits, moods, inputs, history features, habits, agenda steps, and level names) match
`[a-z][a-z0-9_]*` (at most 48 characters). Level names may not be YAML-reserved words (`yes`, `no`, `on`, `off`, `true`,
`false`, `null`, `y`, `n`).

### 2.1 Traits and moods

```yaml
- id: verbosity
  levels: [terse, short, full]      # 2-7 ordered levels, lowest first
  prior: [0.5, 0.4, 0.1]            # probabilities (normalised), or `logw: [...]` natural-log weights; default uniform
  spread: 0.5                       # individuality: s.d. (nats) of each individual's ordinal shift; default 0
  say: [1-2 sentences, a short paragraph, full detail]   # phrase per level for the stance line ("" = say nothing)
  fallback: short                   # level used when there is no stance (default: the prior's most likely level)
  vouch: 0.5                        # optional odds floor: below it the trait is "unsure" (default 0 = off)
  hold: short                       # optional: the level an unsure trait is held at (needs vouch > 0)
```

A **mood** has the same fields plus `inertia` (0 <= k < 1, default 0.5: how much of the previous turn's mood carries over)
and `half_life_hours` (optional: idle time halves the carried mood). Traits are the stance; moods are slower internal
variables that shape it through couplings. Trait and mood ids share one namespace.

**The core trait set.** Any trait may be declared; the eight below are recommended so that personas, hosts and tools
interoperate (the same ids and level names in every persona make individuals comparable across personas):

| trait | levels (low → high) | what it sets in a reply | standard dimension it follows |
|---|---|---|---|
| `warmth` | cool, neutral, warm | pleasantries, encouragement | interpersonal warmth (agreeableness) |
| `directness` | gentle, balanced, blunt | verdict first vs softened | politeness strategy / assertiveness |
| `verbosity` | terse, short, full | length | quantity |
| `humour` | none, light, playful | jokes | playfulness |
| `formality` | casual, neutral, formal | register | formality of register |
| `initiative` | follow, suggest, lead | answer only / propose / act | proactivity |
| `caution` | bold, measured, careful | verification depth, risk talk | conscientiousness / risk appetite |
| `curiosity` | focused, open, exploring | follow-up questions, alternatives | openness |

Recommended moods: `valence` (down, even, up) and `arousal` (calm, steady, keyed): the two axes of the circumplex model of
affect. Common custom traits: `emoji` (none, sparse, rich), `patience`.

### 2.2 Couplings (soft)

```yaml
- {vars: [valence, humour], align: 0.8}              # ordinal alignment: + w x c(a) x c(b)
- vars: [caution, initiative]
  table: {careful: {lead: -0.8}, bold: {follow: -0.3}}   # sparse log-weights, level of a -> level of b -> w
```

`c(level)` is the centred level index, from -1 (lowest) to +1 (highest). A positive `align` makes the two variables move
together, a negative one opposite. Couplings are soft: they shape the odds; they never forbid anything.

### 2.3 Inputs (per-turn evidence)

```yaml
- id: sentiment
  kind: level                       # flag | level | number
  levels: [negative, neutral, positive]
  default: neutral
  say: {negative: user upset, positive: user upbeat}    # used in the explanation
  effects:                          # level -> target -> effect
    negative: {valence: -1.2, warmth: 0.4, humour: -0.8}
- id: error
  kind: flag                        # effects apply when the flag is true
  reactivity_spread: 0.3            # individuality: each individual's gain on this input, exp(N(0, s^2)); default 0
  effects: {caution: 1.0, verbosity: -0.6, valence: -0.6}
- id: stakes
  kind: number                      # 0..max (default max 1; outside it: an error): effects scale linearly with the value
  effects: {caution: 1.6, humour: -1.0}
```

An **effect** on a trait or mood is a number (an ordinal shift: `w x c(level)`, so `+1.0` pushes toward the highest level),
a list (one log-weight per level) or a mapping (`{level: log-weight}`). An effect on an agenda step (`step.<name>`) is a
number (positive = earlier) or per-slot log-weights. All weights are natural-log weights, so `+0.7` roughly doubles the odds.

**Standard inputs** (recommended names, so one host can drive any persona; a persona ignores what it does not declare and
lists it in the stance's `ignored`):

| id | kind | meaning |
|---|---|---|
| `sentiment` | level: negative, neutral, positive | the user's tone this turn |
| `error` | flag | the agent's previous action failed |
| `loss` | flag | the user reports a loss or bad news |
| `praise` / `criticism` | flag | the user praised / corrected the agent |
| `stakes` | number 0..1 | how costly a mistake would be |
| `time_pressure` | number 0..1 | how hurried the user is |
| `claim_done` | flag | the draft reply is about to report a task as done |
| `task` | level: chat, question, code, ops, explain (a persona may declare its own list) | what kind of turn this is |
| `elapsed_hours` | number (reserved) | idle time since the previous turn; decays moods by their `half_life_hours` |

### 2.4 History features

```yaml
- id: errors_in_row
  of: error                         # a flag input
  kind: streak                      # streak: consecutive turns with the flag, this one included; recent: count in the last `window` turns
  cap: 3                            # the value saturates at cap (default 5)
  window: 5                         # recent only (default 5)
  effects: {caution: 0.5, verbosity: -0.4}   # per unit of the value
```

History features are inputs computed from the state; they can drive effects and habit conditions.

### 2.5 Habits (hard rules)

A habit is a rule the stance obeys **by construction**: the engine only considers stances that satisfy every habit in force.

```yaml
- id: no_jokes_on_loss
  when: {loss: true}                # optional; all conditions must hold (flag: true/false; level input: a level or a list;
                                    #   number or history feature: a number = at least, or {at_least, at_most})
  then: {humour: [none]}            # trait/mood -> allowed levels:
                                    #   [levels] | {not: [levels]} | {at_most: L} | {at_least: L},
                                    #   L = a level, prev, prev-1 or prev+1 (the previous turn's level, clamped to the range)
  say: no jokes                     # phrase for the stance line
  priority: 0                       # used only to resolve conflicts (section 5.5); default 0
- id: verify_before_reporting
  rules:                            # raw probbit IR rules over trait / mood ids, level names and agenda steps
    precedes: [{before: step.verify, after: step.report}]
- id: one_flourish_at_a_time
  rules:
    linear: [{terms: [[humour, playful, 2], [humour, light, 1], [emoji, rich, 2], [emoji, sparse, 1]], limit: 3}]
```

`rules` takes the probbit IR constructs exactly as in a program: `caps` (with `members`), `implies`, `tables` (1-3 variables,
`forbid` or `allow`), `precedes` (agenda steps), `linear` (whole-number weights) and `all_different`. Soft `pairs` belong in
`couplings`. A habit without `when` is always in force. The persona's fallback stance must obey every unconditional habit
(checked when the file is read).

Examples of the three habits every persona author asks for: never joke when the user reports a loss (`when: {loss: true}`,
`then: {humour: [none]}`); verify before claiming done (`when: {claim_done: true}`, `then: {caution: [careful]}`, or a
`precedes` on agenda steps); after an error, shorter and more checks (`when: {error: true}`,
`then: {verbosity: {at_most: prev-1}, caution: {at_least: prev+1}}`).

### 2.6 Agenda (optional)

```yaml
agenda:
  steps: [acknowledge, diagnose, fix, verify, report]     # 2-8 beats
  prefer: {acknowledge: 0.6, report: -0.8}                 # number = earlier (+) / later (-), or per-slot log-weights
  say_order: true                                          # put "order: a > b > c" in the stance line (default true)
```

Each step becomes a variable `step.<name>` over ordered slots `s0 .. s(n-1)`, all different (a permutation). Habits order
steps with `precedes`. Keep the agenda free of couplings to traits: it is then its own component and costs nothing.

### 2.7 Engine and line

```yaml
engine:
  op: decide            # decide (exact tiers first, then the sampler + gate) | sample (sampler + gate only)
  sweeps: 2000          # fixed work per chain if the sampler runs (deterministic)
  polish_sweeps: 200
  exact_limit: 2000000
  chains: 4
  twin: true            # also solve the habit-free twin in the same program, to report which habits changed the stance
  values: positional    # positional (level i -> l<i>: small pair tables, fast) | semantic (level names: readable programs)
  on_conflict: fallback # fallback | yield: when habits contradict, the lower-priority one yields (section 5.5)
  # tier: amplitude     # optional tier, off by default (RFC 0011): an exact simulated qubit register on a CPU; absent = the default tiers
  # q_max: 12           # the most encoded qubits (ceil(log2 d) per variable) in one component the amplitude tier takes (1-16)
  # amplify: false      # with the tier (RFC 0011, round 4): exact amplitude amplification in place of the projection
  # schedule: tf-linear-16x4 # with the tier (RFC 0011, round 5): the register's evolution: tf-linear-16x4 | tf-linear-64x16 | qaoa-p2
line:
  max_tokens: 40        # estimated tokens (max of chars / 4 and 4/3 per word)
  order: [verbosity, caution, directness, warmth, humour, initiative, formality, curiosity]
  prefix: "Stance: "
```

**`engine.tier: amplitude` (an optional tier, off by default; `engine: {tier: amplitude}` the opt-in;
docs/rfc/0011-amplitude-register.md).** The turn's program is solved by
an exact simulated qubit register on a CPU: a variable with d allowed values takes ⌈log2 d⌉ qubits (a three-level trait two,
its fourth code unused and kept at amplitude 0), one register per connected component of the program (2^q complex
amplitudes each), a fixed transverse-field sweep with a general diagonal phase layer, the habits as a projection onto the
plans that obey them, when the largest component has at most `q_max` encoded qubits; otherwise the default tiers answer and
`engine.amplitude` says why (`"declined: ..."`). With the tier, the stance document's `engine` carries `tier: "amplitude"`,
`qubits`, `components`, `encoding`, `mixer`, `circuit`, `schedule`, `schedule_name`, `state` (sha256 digests of the encoding,
the layer list, the schedule and the final amplitudes at 6 decimals) and
`feasible_mass`; `live verify` recomputes them from the strand. Without the key nothing changes, byte for byte. An engine
without the tier reports a strand that uses it as `unsupported by this engine: engine.tier` (exit 5). `engine.amplify: true`
(an optional tier, off by default; `engine: {tier: amplitude}` the opt-in, then this key; RFC 0011, round 4) replaces the projection by exact amplitude amplification, simulated on a CPU: the
rules are the oracle, the register is rotated onto the plans that obey them, the stance is the projection's (the same plan and odds, measured equal to
1.1e-12) and `engine.amplify` logs the iterations, the final phases and the mass before and after (`feasible_mass` 1). `engine.schedule`
(an optional tier, off by default; `engine: {tier: amplitude}` the opt-in, then this key; absent = `tf-linear-16x4`; RFC 0011,
round 5) names the evolution of the register, simulated exactly on a CPU: `tf-linear-64x16` (four times
the layers) separates the top two levels where the thermal tier is a near tie (median gap 0.152 vs 0.049 on the recorded corpus) and
agrees more elsewhere; `qaoa-p2` is two fixed QAOA layers (RFC 0011, "Schedules and the indifferent set"). Since round 7 the
register computes its math functions with one library (`libm`) on the std and the no_std build. Measured replay (x86_64,
std and no_std builds): 100/100 lives on each of the three schedules, recorded on the std build, re-verify on the no_std build
and re-write byte for byte; the arm64 replay of the tier is not measured. The math change is in the last bits: the tutor's
and the demo's default-schedule strands are byte-identical to before, a larger persona's can move. Measured cost: 1.50× per
event for the long sweep against the default sweep on a recorded day (a 52-qubit persona, x86_64); the no_std build's stack
high-water ≤ 2,944 B on every schedule. The register is a
simulation: no quantum hardware, no speedup claim; its stances agree with the default tier in 96 % of trait decisions on the
recorded corpus (RFC 0011, round 3). With the key, `persona bound` probes through the tier: compute the certificate again with
it, since one computed without the key does not cover the tier.

### 2.8 Learning (optional)

```yaml
learning:
  from: [praise, criticism]   # flag inputs: the reward, then (optional) the correction
  traits: [verbosity, humour] # the traits that learn (moods do not)
  rate: 0.5                   # step size (natural-log weight per unit of credit)
  step_cap: 0.2               # the most one feedback turn moves a level's weight, before recentring
  total_cap: 1.0              # no learned weight ever leaves [-total_cap, +total_cap]
```

All five fields are required; 0 < step_cap <= total_cap <= 50 and 0 < rate <= 50. With this block the state keeps, per learned
trait, one **learned weight** Δ per level (`learned`, all 0 at `init`) and the **credit** of the previous stance (`credit`):
onehot(level) − odds for each learned trait that stance released, 0 for one it did not (status refused or fallback, an
unreleased trait of a partial turn). A level that a habit or a hold forced has odds 1, so its credit is 0 as well.

Feedback is about the previous reply. On a turn whose reward flag is on (sign +1) or whose correction flag is on (sign −1; both
on: 0), each learned trait's weights move, before the turn is compiled:

    step(l) = clip(sign x rate x credit(l), -step_cap, +step_cap)
    step(l) = step(l) - mean over levels of step                    (recentred: the step moves weight between levels)
    Δ(l)    = round(clip(Δ(l) + step(l), -total_cap, +total_cap), 6)

With a `surprise` block (section 2.13) each learned trait's gain g joins the step and its cap, `step(l) = clip(sign x rate x g x
credit(l), -step_cap x g, +step_cap x g)` (g is 1 without the block, so the step is the one above); with an optimism gene o
(section 2.15) `rate` is `rate x e^(sign x o)`, and with a patience gene p a payment's half-life is `half_life_events x p`,
clipped to [1, 10000].

A turn with no feedback, or feedback on a stance with zero credit, moves nothing. Δ is added to the trait's field (section 3,
step 2), so the feedback turn's own stance already uses it. What learning can and cannot change:

- It changes the learned traits' unary weights and nothing else, within ±total_cap per level. Genes, priors, couplings, moods, inputs and
  history effects are untouched.
- It never changes a habit or a trait's allowed levels: habits are rules of the program (section 3, step 6), the learner's whole
  output is the Δ table, and every stance the engine returns obeys every habit in force whatever Δ is. Rule immunity is
  therefore by construction, not measured.
- A persona without the block has no `learned` or `credit` field, and every document it produces is the same, byte for byte,
  as before the block existed.

**Action-addressed feedback (`learning.credit`, optional).** Without it, feedback pays the previous stance. With it, the host
can name the decision an outcome is about, so a late or out-of-order outcome credits the stance the host acted on:

```yaml
learning:
  # ... the fields above, unchanged
  credit:
    max_open: 8                # decisions awaiting an outcome at once (1-64; default 8)
    half_life_events: 32       # eligibility halves per 32 events of delay (1-10000; default 32)
```

Three event keys are read (not compiled, so not in `ignored`) and come back in the stance's `inputs` as given:

- `await: true`: the host will act on this turn's stance and report its outcome. The decision stays open with its credit and
  its stance's digest (`credit.stance` in the stance document).
- `for: T` or `for: {turn: T, stance: "sha256:..."}`: this feedback pays the decision of turn T. The second form binds the paid
  stance's digest, so a restored state can never pay another decision under the same turn number (B-R6); with `for: T` the
  paid digest comes back in `credit.paid.stance` for the host to compare.
- `cancel: [T, ...]`: no outcome will come for these open decisions. A cancel needs `src` env or human (never the individual).

The credit step runs after the drive step and before compile, at turn t. Feedback is a learning flag on (or, with drives, a
non-zero surprise term), with the sign above:

    expire   each open n with t - n - 1 >= 4 x half_life_events            (closed as expired)
    close    each n in cancel                                               (closed as cancelled)
    if feedback:
      paid   = for if given, else t - 1
      c      = the open decision's credit, or the state's credit when paid = t - 1 is not open
      e      = 0.5^((t - paid - 1) / half_life_events), 6 decimals         (1 for t - 1)
      step(l) = e x clip(sign x rate x c(l), -step_cap, +step_cap), recentred and added as above
      close  paid                                                           (closed as paid)
    -- compile and solve --
    if await: open[t] = {credit, stance digest}; beyond max_open the oldest is closed (closed as evicted)

Eligibility scales the capped step, so a stale outcome always moves less than a fresh one: 3 events late it keeps 0.937084 of
the step, 10 late 0.805245, 32 late 0.5, 127 late 0.063869; 128 events after it was made a decision expires. At e = 1 the step
is the one above exactly: feedback that names the previous stance, or names nothing while no other decision is open, learns
what a persona without the block learns, Δ for Δ. Paying a stance a habit forced moves nothing and is marked `absorbed`. A
control line closes every open decision (section 5.7). An event is **refused whole** (one `{"error"}` at its path, no turn, no
state change; `persona turn` exits 2, `live` at the end of the run) when:

| row | refused | path |
|---|---|---|
| B-R1 | feedback without `for` while a decision other than t − 1 is open (ambiguous) | `inputs.for` |
| B-R2 | `for` naming a decision neither open nor t − 1: paid, cancelled, expired, evicted, never opened or still to come | `inputs.for` |
| B-R3 | `for` on an event without feedback | `inputs.for` |
| B-R4 | `cancel` naming a decision not open, the same turn twice, or the decision this event's feedback pays | `inputs.cancel[i]` |
| B-R5 | `for` not a whole number or `{turn, stance}`; `cancel` not a list of at most `max_open` whole numbers; `await` not true or false; a cancel without an env or human `src` | `inputs.for`, `inputs.cancel`, `inputs.await`, `inputs.src` |
| B-R6 | `for: {turn, stance}` whose digest is not that turn's stance | `inputs.for.stance` |

Reward provenance (section 2.10) is checked before these rows. A stance's digest is the sha256 of its canonical JSON without `credit`,
`state_digest` and `timing`. Without the block, `await`, `for` and `cancel` are undeclared inputs (listed in `ignored`; `live`
says so once on stderr) and every document is as before.

### 2.9 Drives (optional)

```yaml
drives:
  goals:                            # 2-7 goals: the levels of one more variable, `pursue`
    - id: chores                    # an identifier (not a YAML-reserved word); with the block, `pursue` is a reserved trait id
      say: clear the chores         # the line's clause "pursue: chores: clear the chores" (default: the id, _ as spaces)
      interest: 1.0                 # > 0 (default 1): prior weight, normalised over the goals like a trait's prior
      interest_spread: 0.3          # 0..5 (default 0): gene, s.d. (nats) of this individual's standing interest offset
      reactivity_spread: 0.3        # 0..3 (default 0): gene, gain exp(N(0, s^2)) on this goal's wanting evidence and afterglow
      starve_after: 12              # optional, 1..10000 turns: then a habit forces the goal (below)
      priority: 0                   # -100..100 (default 0): that habit's priority
    - id: safety
      floor: 0.1                    # optional, 0..0.5, all floors summed <= 0.5: pursue odds never below it while habits allow it
  wanting:     {half_life_hours: 12, cap: 3, drain: 0.8, signals: {progress: 0.6, novelty: 0.5, cue: 0.4, setback: 0.5}}
  afterglow:   {half_life_hours: 3, cap: 2}
  expectation: {rate: 0.5, half_life_hours: 48, pe_cap: 1.0, win_max: 2.0}
  pursue:      {want: 1.0, glow: -0.5, deadline: 1.5, deadline_tau_hours: 24}
  effects:                          # effect maps as an input's (section 2.3), per unit of the aggregate
    wanting:   {initiative: 0.5, verbosity: -0.3}   # x the strongest wanting over the goals
    afterglow: {valence: 0.6, humour: 0.3}          # x the strongest afterglow
    surprise:  {valence: 0.4}                       # x the turn's prediction error (ordinal shifts; a negative one pushes back)
  learn_from_surprise: 0.5          # 0..1 (default 0): the prediction error joins the learning sign (section 2.8)
```

The block needs `goals`, each with an `id`; the goal fields' defaults are in the comments, `wanting`, `afterglow`,
`expectation` and `pursue` show theirs, and `effects`, `floor` and `starve_after` have none. Bounds: half-lives, caps, `pe_cap`,
`win_max` and `deadline_tau_hours` > 0; `drain` and `rate` 0..1; signal weights and `pursue` weights -50..50.

**Goal signals** ride in the turn's inputs (or a live event) under `goals`, beside the ordinary inputs:
`{"goals": {"chores": {"deadline_hours": 30}, "fun": {"cue": true, "win": 1.5}}, "praise": true, "elapsed_hours": 2}`.
Per goal: `progress` 0..1, `novelty` 0..1 or true/false, `cue` true/false, `setback` 0..1, `win` 0..`win_max`, `deadline_hours`
(a number >= 0 sets the countdown, `null` clears it). An unknown goal or signal is an error at its path.

**The drive step.** Before the turn is compiled, for each goal in declared order (h = `elapsed_hours`, k = the goal's gain gene):

1. **Decay by clock time:** wanting W, afterglow A and expectation E each halve per their half-life (`x 0.5^(h / half_life)`);
   a set deadline counts down by h (not below 0). There is no per-turn inertia: a host that calls more often does not make a
   drive fade faster.
2. **A win of size m > 0:** the prediction error `d = clip(m - E, -pe_cap, +pe_cap)`; `E <- E + rate x (m - E)` (with an optimism gene, section 2.15, the rate above or below E); wanting is
   consumed, `W <- W x (1 - drain x min(m, 1))`; `A <- A + k x max(d, 0)`; a full win (m >= 1) clears the deadline. Without a win,
   d = 0. Repeated equal wins drive E toward m and d toward 0: they stop moving the individual.
3. **Wanting evidence:** `W <- W + k x (progress, novelty, cue, setback) . signals` (the weights above).
4. **Caps:** W in [0, cap], A in [0, cap], E in [0, win_max]; each value is rounded to 6 decimals.

**What the stance reads.** The aggregates wanting = max over goals of W, afterglow = max of A and surprise = the summed d
(clipped to ±pe_cap) act through `effects` exactly as number inputs do (trait effects in the field, mood effects in the mood
evidence), and name themselves in `why` ("wanting", "afterglow", "a win above expectation", "a win below expectation").
`pursue`'s field per goal g (section 3, step 2b) adds `want x W_g + glow x A_g + deadline / (1 + deadline_hours_g / tau)` (the
last term 0 while no deadline is set) to the prior, the interest gene, learned weights and any input or history effects a
persona declares on `pursue` (an input `security` with `effects: {pursue: {safety: 1.0}}`). Couplings, `then: {pursue: [...]}`
and rules over `pursue` use the existing grammar.

**Habits over goals.** `when` takes `goal.<id>.deadline_hours`, `goal.<id>.want`, `goal.<id>.glow`, `goal.<id>.expect` and
`since_pursued.<id>` (turns since the goal was last pursued) as number conditions (a number = at least, or `{at_least,
at_most}`); a deadline condition is false while no deadline is set. A must-do goal is a habit:
`{id: chores_due, when: {goal.chores.deadline_hours: {at_most: 24}}, then: {pursue: [chores]}, priority: 2}`. `starve_after: N`
adds the habit `starve_<goal>` (`when: {since_pursued.<goal>: {at_least: N}}`, `then: {pursue: [<goal>]}`, the goal's priority)
after the declared habits. After each turn, `since` is 0 for the goal a released stance pursues and grows by 1 for every other
goal (for all of them on a refused or fallback turn).

**Floors.** A goal's `floor` is not a rule (rules constrain stances, not odds): the compiler adds the smallest lift (to 1e-6, plus
a 1e-5 margin) to the floor goal's field so that its odds stay at least the floor whatever the levels of `pursue`'s coupling
partners, over the goals the habits in force allow (section 3, step 2b). That bound is sound when no multi-variable rule names
`pursue`, so a persona with a floor and such a rule is refused when it is read. A habit outranks a floor: on a turn whose
habits exclude the floor goal its odds are 0; `probbit persona lint` lists every habit that can exclude a floor goal.

**Learning from surprise.** With a learning block (section 2.8) and `learn_from_surprise: kappa`, a turn's learning sign is
`clip(sign + kappa x surprise, -1, 1)`: a win above expectation reinforces the stance before it, a win below expectation weakens
it, an expected win moves nothing. With `pursue` in `learning.traits`, interests drift toward the goals whose wins surprise,
never past ±total_cap. Learning still writes the Δ table and nothing else; W, A, E, deadlines and `since` are state, outside
its reach.

**Omission windows** (RFC 0002 A.4). A goal may declare `expect_by_hours: H` (> 0): after a released stance pursues the goal its
window opens at H hours (if not already open); each event counts it down by its stamped hours in the decay (step 1); a win of the
goal (m > 0) closes it in step 2; an event at which it reaches 0 without a win fires it once: `d = -min(E, pe_cap)` and
`E <- E + rate x (0 - E)` (E after its decay; with an optimism gene the rate below E, section 2.15), the window closes and `why` says `an expected win did not come (<the goal's
say>)`. The omitted d joins the turn's summed surprise, so `effects.surprise` and valence act on it, and the optional
`effects.omission` map, per unit of min(the omitted E, pe_cap) (the input `drv_omit`); it does not join the learning sign (it is
about the stance that opened the window, and as feedback it would trip B-R1 whenever another decision is open). A goal a need
binds must declare a window (section 2.12). The state keeps `drives.window` (per goal with a window: hours left, or null) and the
stance's `drives` shows it.

**Valence and liking** (RFC 0002 Design E, a simulation):

```yaml
drives:
  valence:   {mood: valence, expect: 0.2, surprise: 0.6}   # replaces effects.surprise; 0 < expect < surprise, both required
  afterglow: {half_life_hours: 3, cap: 2, liking: 0.5, liking_spread: 0.3}   # liking >= 0 (default 0); spread 0..3
```

With `valence` the named mood's evidence is `expect x E` of the goal the previous stance pursued (its expectation after this
event's decay, before its win: the input `drv_expect`, in [0, win_max]) plus `surprise x` the turn's summed prediction error
(through `drv_surprise` and `drv_letdown`, as `effects.surprise` would): valence moves with expected value and with outcomes
better or worse than expected, including expected wins that do not come. Both weights are declared (no default: the sources
fix their signs and order only, Rutledge et al. 2014). The mood's inertia k plays the forgetting factor: at k = 0.61 an event
five events back keeps 0.0845 of its push (half-life 1.402 events). `liking` adds a gain on the win itself to afterglow:
`A <- A + k x (liking_g x m + max(d, 0))`, liking_g = liking x exp(liking_spread x z(name, seed, "liking")) (the gene
`drives.genes.liking`, 4 decimals; spread 0 stores none). `liking: 0` is the afterglow of before.

The block reserves the input ids `drv_want`, `drv_glow`, `drv_surprise`, `drv_letdown` and `drv_c<N>` (the compiled form of the
aggregates and goal conditions), `drv_expect` with `valence` and `drv_omit` with a window: inputs, habit conditions, properties
and `learning.from` may not use them. A persona without the block gives every document byte for byte as before the block
existed; `goals` in its inputs is listed in `ignored`. Without `valence`, `liking` and a window the documents are those of before.

### 2.10 Reward provenance (optional)

```yaml
reward_from: [human, env]           # every reward-bearing input: from a person or a sensor, never from the individual itself
# or one list (or `any`) per reward-bearing input, naming each one:
reward_from: {praise: [human], criticism: [human], goals.fun.win: [env], goals.safety.win: any}
```

A **reward-bearing input** is one that moves what an individual learns or how a goal pays off: the learning block's flags
(`learning.from`, section 2.8) and each goal's `win` (section 2.9). An event may say who produced it with `src`:
`human` or `human:<id>` (a person), `env` or `env:<sensor>` (a sensor: a test runner, a build, a monitor), `self` (the agent's
own output, judged by the host) or `clock` (an idle tick); an id is 1-64 characters of `A-Z a-z 0-9 _ . - @ / :`. With
`reward_from`, a turn whose reward-bearing input is on (a learning flag `true`, a goal's `win` above 0) needs its `src` to be
one of the kinds listed for that input; otherwise the event is **refused whole**, as a bad event is (one `{"error"}` object at
`inputs.src`, exit 2 for `persona turn` and at the end of a `live` run; no turn, no line, no state change):

| the event | refused because |
|---|---|
| `{"praise": true, "src": "self"}` | a reward never comes from the individual itself |
| `{"praise": true}` | a reward needs a source |
| `{"goals": {"fun": {"win": 1}}, "src": "clock"}` | a source the persona does not list for it |
| `{"praise": true, "src": "human:"}` | a malformed source |

An event with no reward on is accepted from any source (`{"loss": true, "src": "self"}`: the agent reports its own failure).
`src` is read, not compiled: it is not listed in `ignored`, it comes back in the stance's `inputs`, and the strand logs it with
the other inputs, so `live verify` replays the decision. `any` (for one input, or for all) leaves that input's source unchecked.
The list holds `human` and `env` only: `self` and `clock` are refused when the persona is read, and so is the key on a persona
without a reward-bearing input; a mapping must name every reward-bearing input. `describe` lists the rule.

What it guarantees, and what it does not. A refused event changes nothing, so a run equals the same run with its refused
events removed, state and strand byte for byte (property P3, tested over 40 individuals x 300 random events mixing every
source). The source is the host's word: provenance makes a host's claim a logged, replayed fact, it does not make it true. The
host sets `src` in its own code (never from the model's text), and the labeller that sets `praise` or a `win` does not read
the agent's own reply as its evidence of success. `fuzz` and `prove` search over stances, not sources: their events stand for
events of an accepted source. A persona without the key reads `src` as any undeclared input (listed in `ignored`), and every
document it produces is the same, byte for byte, as before the key existed.

### 2.11 Senses (optional)

A sense is a reading the host measures (a tool failure, a latency, a message tempo, a silence) in declared units, from a named
source. The engine keeps a learned **normal** per sense (mean, variance, count) and turns each reading into a capped z-score
against it, which moves moods and traits as any input does; `why` names the normal a reading was judged against. Design
notes and arithmetic:
[docs/rfcs/0001-senses-and-reward.md](rfcs/0001-senses-and-reward.md).

```yaml
senses:                          # 1-24 senses; with the block, `src` is read on every event
  - id: tool_fail                # at most 32 characters; reserves the input ids sense_tool_fail_*
    kind: flag                   # flag (the normal is a rate) | number (then `range: [lo, hi]` is required)
    unit: event                  # 1-24 characters, in the digest: two units are two personas
    scale: linear                # linear | log1p (the normal is kept on ln(1 + reading); a number with range low >= 0)
    src: [env]                   # env and/or human; default [env]
    prior: {mean: 0.05, var: 0.05}   # required: the normal at init, anchor of both caps (on the scale)
    drift_cap: 0.25              # required: the mean stays within prior.mean ± drift_cap
    var_cap: 0.2                 # default 4 × prior.var: the variance stays within [0, var_cap]
    half_life_events: 10         # default 10 (1-10000)
    zcap: 6                      # default 6 (1-20)
    dead_zone: 0.5               # default 0.5 (0 <= dead_zone < zcap)
    var_floor: 0.01              # default 0.01 (> 0)
    say: tool failures           # default: the id, _ as spaces
    effects: {caution: 0.8, valence: -0.4}   # ordinal shift per unit of z beyond the dead zone; negative z pushes back
    gain_spread: 0.3             # gene: gain exp(N(0, s²)) on effects; default 0, range 0..3 (all three)
    threshold_spread: 0.3        # gene: offset N(0, s²) on dead_zone; default 0
    habituation_spread: 0.3      # gene: multiplier exp(N(0, s²)) on half_life_events; default 0
```

Why these defaults: a reading 10 events old counts half, so the normal follows a changed rate halfway within 10 readings; at
`zcap` 6 a failure after 40 quiet readings scores 3.4× the same failure after 40 readings at rate 0.3 (1.7× at `zcap` 3);
readings within half a standard deviation move nothing; `var_floor` keeps z finite when the variance collapses; at `var_cap`
4 × prior.var the spread at most doubles. `prior` and `drift_cap` have no default: no unit-free anchor is safe.

**Refused when the persona is read** (exit 2, at the field's path): a taken `sense_` id (an input, history feature or goal
whose id starts with a sense's `sense_<id>_`), an effect on an unknown trait or mood, `src` naming `self` or `clock`,
`prior.mean` outside the scaled range, `prior.var` <= 0, `drift_cap` <= 0, `var_cap` < `prior.var`, `dead_zone` >= `zcap`,
`log1p` on a flag or on a range low below 0, a cap with more than 6 decimals, an unknown field, a number sense without a range,
two senses with one id, an empty block or more than 24 senses.

**Events.** `{"senses": {"tool_fail": true, "tool_latency": {"v": 2.4, "src": "env:runner"}}, "src": "env:host"}`: a bare value
takes the event's `src`, `{v, src}` carries its own. `senses` is read, not compiled: it is not listed in `ignored` and comes back
in the stance's `inputs`.

**The sense step**, after validation and before the drive step, per declared sense with a reading, in declared order (genes
g_gain, g_thr, g_hab; each line rounded to 6 decimals):

```
x     = the reading (a flag: 1 or 0), or ln(1 + reading) for log1p
h     = clip(half_life_events × g_hab, 1, 10000)
a     = 1 − 0.5^(1/h)
z     = clip((x − mean) / sqrt(var + var_floor), −zcap, +zcap)        # against the past, before the update
d     = clip(dead_zone + g_thr, 0, zcap − 0.000001)
zd    = sign(z) × max(|z| − d, 0)
mean' = clip(mean + a × (x − mean), prior.mean − drift_cap, prior.mean + drift_cap)
var'  = clip((1 − a) × (var + a × (x − mean)²), 0, var_cap)              # with the mean before the update
n'    = min(n + 1, 1000000)
```

The turn adds g_gain × zd times each effect (an ordinal shift, section 2.3) to its target's field or mood evidence (section 3),
as a number input does. It is lowered as drives' surprise is: two reserved number inputs, `sense_<id>_up` = max(zd, 0) carrying
the effects and `sense_<id>_down` = max(−zd, 0) carrying them negated, both in [0, zcap], gain g_gain. Genes follow section
5.4's z: `round(exp(gain_spread × z(name, seed, "sense", id, "gain")), 4)`, `round(threshold_spread × z(…, "threshold"), 4)`,
`round(exp(habituation_spread × z(…, "habituation")), 4)`; a spread of 0 means gain 1, offset 0, multiplier 1 and no stored
gene. At the defaults (a flag with prior 0.05/0.05) the same failure every event gives zd 3.38, then 1.03 from reading 6 on:
the push falls to 0.30× and stays there, because the drift cap keeps a chronic failure from becoming normal.

**Rules the engine enforces.**

| rule | what it means |
|---|---|
| S1 declared, in range | a reading for an undeclared sense, a number outside its range or a flag that is not true/false refuses the event at `inputs.senses.<id>`; nothing is clipped |
| S2 provenance | each reading's source kind must be in its `src`; a missing or malformed source, `self` and `clock` refuse the event whole, and a refused event changes nothing |
| S3 the engine owns the normal | hosts send raw readings; `sense_<id>_*` inputs and a pre-judged z are refused; z, normals and genes come from logged readings only |
| S4 anchored | a state is refused, even with a recomputed digest, if a mean leaves prior ± drift_cap, a variance leaves [0, var_cap], n is not a whole number up to 1e6, the sense set differs from the persona's or a gene misses its seed |
| S5 no rule reads the normal | `when` takes `sense.<id>.raw` (true/false for a flag, declared units for a number) and `sense.<id>.z` (the z above, before dead zone and gain), never the mean or variance; a condition on a sense not read this event is false |
| S6 no clock decay | normals move only on readings; silence, idle hours and control lines leave them |
| S7 not through the agent's channel | the MCP turn tools refuse an event that carries readings (section 7) |

A rule that must hold whatever the history reads `raw`: `prove` reads `sense_<id>_up/down` as boxes [0, zcap] × gain and each
sense condition both ways, so a habit on `sense.tool_fail.raw` holds by construction for a rule with the same condition.
`fuzz` adds readings: flags both ways, numbers on `--grid` × range, one reading repeated 1, 5 and 20 times, and the range ends
alternating.

**The stance and `why`.** The stance gains `senses` (per sense read: `raw`, `src`, `z`, `zd`, and the `mean` and `n` it was
judged against) and `noticed` (the sense of the normal clause, or null). `why` becomes: the bound habits; the normal clause;
up to two other live inputs (sense inputs excluded); the learned clause; the engine status if not ok.

- **Normal clause**, at most one: the read sense with the largest |g_gain × zd| > 0 (ties: declared order),
  `tool failures far above its normal (z +6.0 over 40 readings)`: "far" at |z| >= 2, above or below by sign. Failing that, the
  read sense with the largest |z0| − |z| >= 1, z0 being the reading judged against the prior:
  `tool latency usual for it now (z +0.9 over 40 readings, +3.9 against its prior)`.
- **Learned clause**, at most one, with a learning block and `line.why_past` (default true with senses, false without, so
  existing documents keep their bytes): the released learned trait with the largest |Δ| at its taken level, if >= 0.05,
  `learned: caution +0.31 on careful (0 rewards, 4 corrections)`. The counts are state: `learned_from`, per learned trait, the
  feedback turns that moved it, by sign.

z prints with one signed decimal and Δ with two, rounded half to even from the stored value.

What it guarantees, and what it does not. A persona without the block reads `senses` as any undeclared input (listed in
`ignored`; `live` says so once on stderr) and every document it produces is the same, byte for byte, as before the block
existed. A refused reading changes nothing (P3 holds with readings: 40 individuals × 300 random events mixing sources,
readings, `self`, `clock` and out-of-range values). A source is the host's word: a forged `env` label passes every check;
readings are logged and replayed, not shown true.

### 2.12 Needs (optional)

A need is a number with a setpoint: use or time grows its deficit, and an event that reduces it pays a win sized by how much it
reduced the drive. It changes which goal the stance pursues (RFC 0002 Design A; Keramati & Gutkin 2014).

```yaml
needs:                              # needs a drives block; 1-8 items
  drive: {"n": 4, "m": 3}           # required, n > m > 1 (quoted: the YAML reader refuses a bare n, YAML 1.1's false)
  pursue: {want: 2, cap: 3}         # the pursue term: its weight, and |term| <= cap nats (defaults shown)
  success: {rate: 0.2, init: 1}     # the bound goal's success rate s: its step per win or omission, its start (defaults shown)
  r_cap: 1                          # required: |win| <= r_cap (at most drives.expectation.win_max)
  items:
    - id: context                   # at most 32 characters; reserves the input id need_context
      say: context budget           # default: the id, _ as spaces
      unit: token_share             # 1-24 characters
      cap: 1                        # the deficit stays in [0, cap]
      weight: 1                     # w > 0
      per_hour: 0                   # growth per stamped hour (>= 0)
      per_reading: {sense: tokens, rate: 1}   # optional: growth per unit of a number sense's raw reading (section 2.11)
      satisfy_from: [env]           # required: env and/or human
      amount: 0.6                   # K: the typical satisfying amount, 0 < K <= cap (the forecast's)
      bind: compact                 # the goal its reduction pays; one need per goal; the goal needs expect_by_hours
      horizon_hours: 0              # tau, 0-72 (default 0)
      forecast_per_hour: 0.06       # the growth per hour the forecast assumes (default per_hour; >= 0)
      weight_spread: 0              # gene: multiplier exp(N(0, s^2)) on weight; 0..3, default 0
      growth_spread: 0              # gene: multiplier exp(N(0, s^2)) on per_hour and per_reading.rate; 0..3, default 0
```

**Amounts** ride in the event under `needs`, beside the ordinary inputs: `{"needs": {"context": {"amount": 0.6, "src":
"env:compactor"}}, "src": "env:host"}`. A bare number takes the event's `src`; the source kind must be in the need's
`satisfy_from` (never `self` or `clock`). `needs` is read, not compiled, and `inputs` echoes it as given.

**The needs step** runs after the sense step and before the drive step, in two passes, every line rounded to 6 decimals (h =
`elapsed_hours`; genes g_w and g_gr, 1 without a spread):

```
pass 1, every need in declared order:   x = clip(x + g_gr x (per_hour x h + per_reading.rate x reading), 0, cap)
D(x) = (sum_i w_i g_w,i x_i^n)^(1/m)                                  # over all needs
pass 2, each need satisfied this event by amount a, in declared order:
  r   = clip(D(x) - D(x with x_j <- |x_j - a|), -r_cap, +r_cap);   then x_j = |x_j - a|, clipped to [0, cap]
```

`reading` is the sense's raw value this event (0 when it is not read; never the normal). r > 0 is a `win` of size r for the
bound goal, r < 0 a `setback` of min(-r, 1) (added to the host's, at most 1); both join the event's goal signals, so the drive
step treats them as any win or setback (E, wanting, afterglow, the deadline, the window). n > m > 1 gives deprivation scaling:
the same amount 0.2 pays 2.0229 times as much at deficit 0.8 as at 0.2, and an overshoot pays less than nothing (-0.117 at
deficit 0): sign(r) = sign(x_j - a/2). The deficits after an event do not depend on the declared order; the wins depend on it
only through D over the other needs, never in sign.

**The pursue term.** For each need, with s the bound goal's success rate and K its `amount`:

```
x^   = x + g_gr x forecast_per_hour x horizon_hours                  # the forecast: a declared growth rate, not clipped at cap
term = clip(want x s x (D(x^) - D(x^ with x^_j <- |x^_j - K|)), -cap, +cap)     # cap = pursue.cap
s   <- s + rate x (1 - s) on a win of the goal, and k <- 0
s   <- s x max(1 - k x rate, 0) when the goal's window fires, k <- k + 1 first   # k: omissions since the last win (section 2.9)
```

Since 0.11.0 the forecast is not clipped at the cap: the deficit stays in [0, cap], but a need sitting at its cap still forecasts
the growth its horizon brings, so the horizon also raises the term at the cap (before, x^ = x there and a horizon changed
nothing once the deficit had reached it; bench/body n10). And omissions in a run cut deeper: the k-th unanswered pursuit in a row
multiplies s by 1 - k x rate, so a run of 1/rate omissions (5 at rate 0.2) takes s to 0, while isolated omissions between wins
still cost 1 - rate each (n11); a win ends the run. The state keeps the run as `needs.misses: {<goal>: k}` for goals with k > 0 (absent when every run is 0).

It is lowered as the number input `need_<id>` (box [-pursue.cap, +pursue.cap]) with effect `{pursue: {<bind>: 1}}` (section 3,
step 2b). Pursuit is weighted by the goal's learned success, so unanswered pursuits extinguish themselves through the omission
let-down; at horizon 0 the term is positive exactly when x > K/2, the deficits at which the satisfaction pays a positive win.
A horizon > 0 lets the forecast pass K/2 before the deficit does (`lint` names it). Rules may read `need.<id>.deficit` as a
number condition (a number = at least, or `{at_least, at_most}`), so a must-do habit can fire at a deficit:
`{id: compact_when_full, when: {need.context.deficit: {at_least: 0.9}}, then: {pursue: [compact]}}`. A rule never reads
`need_<id>`.

**Refused when the persona is read** (exit 2, at the field's path): `drive` without n > m > 1, a needs block without a drives
block, no items or more than 8, two needs with one id, a taken `need_<id>` id (N-R1); `bind` naming an unknown goal, or two
needs bound to one goal (N-R2); `satisfy_from` missing, empty, or naming `self` or `clock` (N-R3); `per_reading` naming an
undeclared or flag sense, a negative `per_hour` or `rate` (N-R4); `cap`, `weight`, `amount`, `want`, `pursue.cap` or `r_cap`
<= 0, `amount` > `cap`, `r_cap` > `win_max`, `horizon_hours` outside [0, 72], `success.rate` outside (0, 1], `success.init`
outside [0, 1], a number with more than 6 decimals (N-R5); a need whose bound goal has no `expect_by_hours`, or a window <= 0
hours (N-R6).

**An event is refused whole** (one `{"error"}` at its path, no turn, no state change): an amount for an undeclared need, not a
number > 0 or above the need's cap, or one need satisfied twice (N-E1, `inputs.needs.<id>`); an amount whose source kind is not
in `satisfy_from`: missing, malformed, `self`, `clock` (N-E2, `inputs.needs.<id>.src`); an amount and its bound goal's `win` in
one event: one outcome, one payment (N-E3, `inputs.needs.<id>`); a `need_*` or `drv_omit` input, or a deficit sent with an
amount (N-E4: the engine owns them).

**State and stance.** The state keeps `needs: {genes: {weight: {id: g}, growth: {id: g}}, deficit: {id: x}, success: {goal:
s}}`: each deficit 0 and each s at `success.init` when the individual is made; genes `round(exp(weight_spread x z(name, seed,
"need", id, "weight")), 4)` and the same with `"growth"` (spread 0 stores none). The stance gains `"needs": {"context":
{"deficit": 0.55, "forecast": 0.61, "term": 1.109371}}` (the deficit after this event, the forecast and the term the field read),
and `why` gains at most one need clause, for the pursued goal when a need binds it and its term is the largest positive one:
`context budget deficit 0.55, above its setpoint`.

What it guarantees, and what it does not. A persona without the block reads `needs` as any undeclared input (listed in
`ignored`; `live` says so once on stderr) and every document it produces is the same, byte for byte, as before the block
existed. A refused amount changes nothing (P3 holds with amounts: 40 individuals × 300 random events mixing sources, amounts,
windows and payments). Amounts never come through the MCP tools (section 7). A source is the host's word: a forged `env` label
passes every check; amounts are logged and replayed, not shown true. A need is arithmetic over logged numbers, not an inner
state.

#### Viability bounds, graded pressure and need valence (optional, RFC 0008; 0.11)

Three keys, each additive: a persona without them gives every document byte for byte as before (the stance, the state, the
strand, `describe`). A bound says where a need stops being viable; the couplings carry the distance to that bound into two
moods with one weight each, with no thresholds to pick. All of it is arithmetic over the logged readings and amounts.

```yaml
needs:
  pressure: {mood: pressure, gain: 2.0, shape: linear}   # mood evidence += gain x (1 - margin_min); needs a bound
  valence: {mood: valence, tonic: 0.8, phasic: 0.6}      # mood evidence += -tonic x D(x)/D(cap) + phasic x (D(x_before) - D(x_after))
  items:
    - id: integrity
      per_reading: {sense: tool_fail, per_event: 0.08}   # a flag sense: growth per true reading (a number sense takes rate)
      viable: {at_most: 0.8, events: 4}                  # the deficit bound x_b and the run of events beyond it that ends viability
```

**Bounds (RFC 0008 A).** After the needs step, per need with `viable`, every number rounded to 6 decimals:

```
margin = round(clip(1 - x / x_b, 0, 1), 6)          # 1 at the setpoint, 0 at the bound
beyond = beyond + 1 (at most events) if x > x_b, else 0
worst  = the need with the smallest margin (ties: declared order);  margin_min = its margin
```

When some need's `beyond` reaches its `events`, the state sets `viability: "lost"` once and keeps it; nothing else changes
(the engine never stops a host). The state's `needs` gains `beyond: {id: n}` (per bounded need) and `viability: "viable" |
"lost"`, only with a bound. Rules may read `need.<id>.margin` (a need with a bound) and `needs.margin_min` as number
conditions (a number = at least, or `{at_least, at_most}`), as they read `need.<id>.deficit`.

**Pressure (RFC 0008 B).** The named mood (any declared mood) takes `gain x (1 - margin_min)` as an ordinal shift each event,
lowered as the reserved number input `need_pressure` (box [0, 1]). `shape` is `linear` (the one shape).

**Need valence (RFC 0008 C).** The named mood takes `-tonic x D(x)/D(cap)` (lowered as `need_tonic`, box [0, 1]: how far the
deficits are from their setpoints, D(cap) the drive with every need at its cap) and `phasic x (D(x before pass 1) - D(x after
pass 2))` (lowered as `need_phasic`, box [-r_cap, r_cap]: this event's drive reduction, Keramati & Gutkin's homeostatic
reward). It adds to `drives.valence` when both name the mood. A weight of 0 lowers no input.

**Flag senses as growth (RFC 0008 D).** `per_reading` on a flag sense takes `per_event` (growth per true reading, times the
growth gene); N-R4 reads "a number sense with `rate`, or a flag sense with `per_event`".

**Stance and receipts (RFC 0008 E).** `needs.<id>` gains `margin` and `beyond` (a need with a bound); the stance gains
`viability: {worst, margin_min, status}`; `why` gains `<need> margin <m>, nearest its bound` while the worst margin is under
0.25. **`block`**: with a bound, a coupling or `line.block: true`, the stance carries the text a host injects verbatim: every
declared trait, then every mood, once each as `name level` (declared order, `; ` between), then with a needs block one line
`viability: worst <need>, margin <m>` (`viability: lost, worst ...` once lost; `viability: no bounds declared` without a
bound). `line` is unchanged. `live verify` summarises `viability: {margin_min_seen, lost_at}` (the smallest margin_min after
any event replayed, the event at which viability was lost or null), recomputed from the logged readings and amounts; `live
verify --blocks FILE` writes each event's block (any persona, a block key or not). `monitor` draws a VIABILITY line (worst,
margin_min, status) with one margin bar per bounded need, the worst highlighted. `prove` boxes `need_pressure`, `need_tonic`
and `need_phasic` like any number input.

**Refused when the persona is read** (exit 2, at the field's path): `viable.at_most` outside (0, cap] or with more than 6
decimals, `viable.events` not a whole number in 1..10000, an unknown key (V8-R1); `needs.pressure` without a need with
`viable`, a `gain` <= 0 or above 50, a `shape` other than `linear`, a `mood` not declared; `needs.valence` with `tonic` or
`phasic` outside [0, 50] or a mood not declared; `per_event` on a number sense or `rate` on a flag sense (N-R4); a need whose
input id `need_<id>` is `need_pressure`, `need_tonic` or `need_phasic` while the coupling that reserves it is declared;
`need.<id>.margin` on a need without a bound; a rule on `need_pressure`, `need_tonic` or `need_phasic`. **An event is refused
whole** when it sends `need_pressure`, `need_tonic` or `need_phasic` (N-E4: the engine computes them).

What it guarantees, and what it does not. A margin is a declared bound applied to a logged deficit; "viability" is a status
of that arithmetic, not a state of the host or of anything else. A source is the host's word, as for every reading. See
docs/welfare-stance.md for what the words need, viability and damage mean here and what is not claimed.

### 2.13 Surprise (optional)

After a run of surprising outcomes the learner takes larger steps, and smaller ones again once things settle (RFC 0002 Design
B; Behrens et al. 2007; Nassar et al. 2010).

```yaml
surprise:                     # needs a learning block; every field optional (the defaults shown)
  volatility_rate: 0.3        # a: the volatility average's weight per feedback, in (0, 1)
  v0: 0.45                    # the volatility at which the gate opens, in [0, 1)
  r_ref: 4                    # the run length at which the gain is 1 (>= 1)
  r_max: 20                   # the cap on the run length (>= 1)
  g_max: 3                    # the most the gate multiplies a step, in (1, 3]
  precision: false            # true needs a senses block: each sense's effect gain x (1 - var / var_cap)
```

**The gate** runs inside the credit step (section 2.8), after the payment is settled and before the learner: for each learned
trait whose paid credit has a taken level l* (credit > 0), when the feedback's sign is not 0, in this order (each stored number
rounded to 6 decimals):

    mb      = (plus(l*) - minus(l*)) / (plus(l*) + minus(l*) + 2)
    p_same  = clip((1 + sign x mb) / 2, 0.01, 0.99)
    v       = (1 - a) x v + a x (1 - p_same)
    Ω       = clip((v - v0') / (1 - v0'), 0, 1)            v0' = clip(v0 + the volatility gene, 0, 0.999999)
    counts  = counts x (1 - Ω), then plus(l*) or minus(l*) += |sign|
    r       = min((1 - Ω) x (r + 1) + Ω, r_max)
    g       = clip((1 + Ω x r) / (r + 1) x (r_ref + 1), 1, g_max)

and the learner's step carries g (section 2.8). A new learner's gate starts closed (since 0.11.0): v = 0, r = r_ref, so g = 1,
the fixed rate, and its opening feedback at the defaults leaves (v, r, g) = (0.15, 5, 1). While Ω = 0, r only grows, so g stays
at 1; g rises above 1 only after a gate step with Ω > 0, that is once a run of contrary feedback (a lesson reversed) has raised v
past v0. Consistent feedback after that raises r and brings g back to 1 within a few turns. g
never goes below 1, so the gate never learns slower than `rate` alone; `total_cap` is untouched, so the learned box `prove`
uses is the same. The volatility average is slow on purpose: it needs a run of surprises, which a real change produces and
noise rarely does. bench/body's bars s1-s3 measure the re-learning turns, the stable-phase movement and the caps
(BENCHMARKS.md section 12).

**Precision** (`precision: true`): each sense input's effect gain is multiplied by 1 − var / var_cap of its sense after this
event's sense step (section 2.11), so a sense whose readings vary widely moves the stance less. It is off by default because it
changes every sensed document.

**State and stance.** The state keeps `surprise: {<trait>: {v, r, g, counts: [[plus, minus], ...]}}` per learned trait, one
pair per level. The stance gains `"surprise": {"<trait>": g}` for the learned traits whose gain is above 1 (`{}` once every gate
is closed), and `why` says `learning fast: feedback changed` while any gain is above 1.

Refused when the persona is read (exit 2, at the field's path): a surprise block without a learning block, `precision: true`
without a senses block (V-R1); `volatility_rate` outside (0, 1), `v0` outside [0, 1), `r_ref` or `r_max` below 1, `r_ref` above `r_max`
(a run length capped below r_ref would keep g above 1 for good), `g_max` outside (1, 3] (V-R2). A state is refused, even with a recomputed digest, when v leaves [0, 1], r leaves [1, r_max], g leaves
[1, g_max], a count is negative, the pairs do not match the trait's levels, or a number has more than 6 decimals.

### 2.14 Pace (optional)

Average reward over the last hours sets the pace of the next actions; a single reward does not (RFC 0002 Design C; Niv et al.
2007; Beierholm et al. 2013).

```yaml
pace:                          # needs reward_from (section 2.10)
  half_life_hours: 6           # the average's half-life in stamped hours (> 0)
  cap: 4                       # the average stays in [0, cap] (> 0)
  r0: 1                        # the average at which the effects are 0, in (0, cap)
  levels: [0.5, 1.5]           # required: slow below the one, brisk above the other; 0 <= slow_below < brisk_above <= cap
  effects: {initiative: 0.5, verbosity: -0.3}   # required: ordinal shift per unit of (average - r0), on traits or moods
  pace_spread: 0               # gene: a gain exp(N(0, s^2)) on the effects, 0 to 3
```

**The pace step** runs after the drive step (h = `elapsed_hours`; r_t = the sum of this event's accepted wins, the needs' wins
included, plus 1 when the learning block's reward flag is on; corrections add nothing):

    R = round(clip(R x 0.5^(h / half_life_hours) + (1 - 0.5^(1 / half_life_hours)) x r_t, 0, cap), 6)

A new individual starts at R = 0. Stance t reads R after event t: the outcome of turn t's action, carried by a later event,
moves only later stances. R enters the field as two number inputs, `pace_up` = max(R − r0, 0) (box [0, cap − r0]) and
`pace_down` = max(r0 − R, 0) (box [0, r0]), carrying `effects` (negated for `pace_down`) times the gain g_pace =
round(exp(pace_spread x z(name, seed, "pace", "gain")), 4). The two ids are reserved only with the block; the engine sets them
(an event that sends one is refused at `inputs.pace_up`), and the stance's `inputs` leave them out.

On hourly events this is a running average: a host acting a = base x (1 + κ x (R − r0)) times an hour, each action paying with
probability p, settles at R* = p x base x (1 − κ x r0) / (1 − G) while the loop gain G = p x κ x base < 1 (1.2632, 1.5, 2.25 at
G = 0.24, 0.60, 0.84) and runs to the cap that binds when G >= 1. The engine cannot see that loop (p is the world's), so `lint`
reports the largest shift pace can put on the field, sum of |effects| x max(cap − r0, r0), and warns over 1; `describe` prints
the closed form.

**State and stance.** The state keeps `pace: {rbar}`. The stance gains `"pace": {"level": "brisk", "rbar": 1.683403}` (slow,
steady or brisk by `levels`, for the host's scheduler), and `why` says `pace brisk: average reward 1.68 an hour` (R to 2
decimals) when the level is not steady.

Refused when the persona is read: a pace block without `reward_from` (P-R1: no self-reward can move it); `cap` or
`half_life_hours` <= 0, `r0` outside (0, cap), `levels` not ordered within [0, cap], a missing `levels` or `effects`, a
`pace_spread` outside 0 to 3 (P-R2); an effect on `pursue` or on an unknown trait or mood, a declared input or a habit
condition that takes `pace_up` or `pace_down` (P-R3). A state is refused, even with a recomputed digest, when R leaves [0, cap]
or has more than 6 decimals.

### 2.15 Dispositions (optional)

Each individual learns at its own rate from better- and worse-than-expected outcomes; the rates are genes (RFC 0002 Design D;
Lefebvre et al. 2017; Frank et al. 2009).

```yaml
dispositions:                 # each spread 0 to 3, default 0 (no gene)
  optimism_spread: 0.3        # o = s x z: rate x e^o above the expectation, rate x e^-o below (a learning or a drives block)
  patience_spread: 0          # p = exp(s x z): x credit.half_life_events and x deadline_tau_hours (a credit or a drives block)
  volatility_spread: 0        # an offset s x z on surprise.v0 (a surprise block)
```

The genes come from the persona hash family z(name, seed, "disposition", <gene>): `optimism` = round(s x z, 4), `patience` =
round(exp(s x z), 4), `volatility` = round(s x z, 4). They act here:

    section 2.8 step:  rate x e^(sign x o), then the step_cap clip
    section 2.9 E:     E <- E + α x (m - E), α = min(rate x e^o, 1) if m > E, min(rate x e^-o, 1) if m < E (the omission too)
    patience:          half_life_events x p, clipped to [1, 10000] (expiry at 4 half-lives); deadline_tau_hours x p

With wins b and a at equal odds, E settles at the expectile τb + (1 − τ)a, τ = α⁺ / (α⁺ + α⁻); on one stream an individual
with o > 0 keeps E at or above its spread-0 twin's at every step (the update is increasing in E and α⁺, decreasing in α⁻).

**State.** `dispositions: {optimism, patience, volatility}`, only the genes whose spread is above 0: a spread of 0 stores
nothing, so a block with every spread 0 gives the bytes of the persona without it (digests aside). There is no `why` clause;
`describe` prints the spreads and the genes at the persona's seed. Refused when the persona is read: a spread outside 0 to 3
(D-R1); `volatility_spread` without `surprise`, `optimism_spread` without a learning and without a drives block,
`patience_spread` without a credit block and without a drives block (D-R2). A state whose genes miss its seed is refused.

What the three blocks guarantee, and what they do not. A persona without them may use `pace_up` and `pace_down` as any ids,
and every document it produces is the same, byte for byte, as before the blocks existed. Every new number is capped, rounded
to 6 decimals (genes to 4) and recomputed by `live verify` from logged feedback, wins and stamped hours. The gate, the average
and the genes are arithmetic over logged numbers, not inner states.

### 2.16 Bound (optional; RFC 0004)

A certificate, not a behaviour: `bound` adds no state and no per-turn cost, and it stays out of the persona digest, so adding
it to a persona with a recorded life changes no state, strand line or turn (RFC 0004, amendment A1).

```yaml
bound:
  rest_hours: 200      # the rest probe: one event of this many idle hours, no inputs (default 30 x the longest half-life; 24 if none)
  max_epsilon: 0.5     # optional: `persona check` fails above it, or on an uncounted channel
  seeds: 0-9           # the individuals epsilon is the worst case over (default 0-99)
  count: [arousal]     # optional: moods without half_life_hours to search instead of naming them uncounted
```

`probbit persona bound PERSONA [--seeds LIST] [--json]` prints, per learned trait, **epsilon**, the attaining level set and
seed, the `ceiling` tanh(D / 4), `speed` (the built-in greedy persuader's feedback events to epsilon / 2, a witness),
`speed_floor` (proved: no shorter sequence reaches epsilon / 2), the residue `r` of decayed channels, the enumerated history
and every uncounted channel at its path. Exit 0 when every channel is searched, decayed or enumerated; 1 on an uncounted
channel or an epsilon above `max_epsilon`; 2 on a refusal (BO-R1 to BO-R6: an unknown key, `max_epsilon` outside (0, 1),
malformed `seeds`, no `learning` block, `rest_hours` outside (0, 1e6], a bad `count` entry).

**The claim, exactly** (a theorem about the engine's arithmetic, labelled mechanism): no event sequence moves a learned
trait's odds at rest (`rest_hours` idle, no inputs) by more than epsilon + r in total variation; epsilon is the exact maximum
of that change over the learned-weight box ±`total_cap` (and the counted and enumerated channels), computed from this file
by probing the box's vertices. It bounds the stance at rest, not the stance mid-conversation and not the words a model
writes. It is not a safety gate and grants nothing. Above 12 searched coordinates in a trait's component it prints the coarse
ceiling (sound, not tight), never sampled vertices.

**History.** A `recent` feature whose effects reach the trait's component, or which a habit's `when` names, is enumerated
by its values, not by stored sequences. The rest probe's own flag is false, so a feature reads min(`cap`, trues in the
last `window` − 1 stored turns) and a `streak` reads 0; the stored turns of a flag are split at each of its features'
window boundaries and every count of trues per segment is probed once per distinct tuple of capped values. Every tuple is
reachable and nothing else of the stored turns enters the rest probe, so this is the exact maximum over histories (equal,
in the test, to the search over every stored sequence). A chat persona's praise (window 4) and wins (window 6, cap 3) give 4 × 4 =
16 combinations instead of 256. Epsilon then includes what the history moves on its own (`ceiling` prints null). Limit:
more than 4096 combinations per trait and seed prints the coarse value 1 (sound), and `--audit` refuses with
`<trait>: more than 4096 enumerated history combinations: the audit needs an exact epsilon` (exit 2).

In words for a reader: "Its learned character has a published limit: no sequence of messages can move the odds of any trait
at rest by more than epsilon, a number computed from its file that anyone can recompute. The limit is arithmetic over
declared numbers. It covers the stance at rest, not the words a model writes, and it is published with how fast the limit
can be reached."

**Audit.** `probbit persona bound PERSONA --audit STRAND` replays a recorded life and reports, per learned trait, the room
used (resting TV between the individual and the same individual with its learned weights at 0, divided by epsilon at the
strand's seed): the most at any event, at which event, now, and the distance to the ceiling now; exit 1 on an exceedance
(impossible by the theorem; the audit shows it on the recorded life). The room used counts the learned weights only, at the
individual's own history; epsilon is the whole resting change, history included, so on a trait an enumerated history
reaches the room used is the learned share of that limit.

**Erasure.** `probbit live control STRAND erase --by human:ID --reason TEXT` appends a chained control line that returns
every learned weight to 0 (and closes the open credits, as every control line does); replay applies it, the next event's
state digest records it, and the audit then reports a room used of 0.

### 2.17 Expectation and trajectory (optional; RFC 0009)

Two additive keys, both off by default and both outside the persona digest (as `bound` is): a persona without them gives
every document byte for byte as 0.11.0, and adding either to a persona with a recorded life leaves every state and stance
digest as it was. Both have effects {}: they move nothing.

```yaml
expectation:            # optional; {} = the defaults
  half_life_events: 32  # the decayed combo counts' half-life, in events
  max_combos: 16        # mixture members at most, the empty combo included; the rest fold into it
engine:
  trajectory:           # optional
    k: 8                # steps, 2-16
    beta0: 0.4          # first step's inverse temperature, (0, 1); linear to 1
```

**Expectation.** On `probbit live` (the strand is the memory), per event, from the state before it: A_t = the event's
program (`persona compile` of its inputs); E_t = the program with `elapsed_hours` only; Pi_t = the mixture over the
canonical input combos seen so far (false, 0 and null dropped; a declared number input bucketed into the thirds of its
range; `elapsed_hours` dropped), each member compiled from its combo's latest inputs with this event's hours (the event's own
combo with its own inputs, so A_t is a member whenever its combo is), weights = exponentially decayed counts plus one
pseudo-count on the empty combo. Mixture marginals are linear in the members' exact marginals, so every number is exact
from at most M + 1 exact solves. The stance gains

```json
"deviation": {"self": 2.18, "pred": 1.64, "input": 1.21, "cold": false, "z": 2.3, "registers": true}
```

`self` = sum over the non-twin variables of KL(A_i || E_i); `pred` = the same against Pi_i (nats); `input` = -ln of the
mixture weight of the event's combo (its own member's, else the empty member's); `cold` = no non-time combo seen yet
(`pred` is 0 then); `z` = (ln(1 + pred) - mean) / sqrt(var + 1e-4) against a learned normal (prior mean 0, var 0.04, the same
half-life; read after the cold start, from the logged value); `registers` = z >= 1.5. The strand line carries the receipt
`"deviation": {"input", "pred", "self"}` at 5 significant digits (with `"cold": true` while cold); `live verify` recomputes
it ("the deviation differs" on an edit), `resume` folds the same memory from the logged lines, and the stance digest the
line logs excludes `deviation`. Bound (by construction): per variable, `pred_i <= input` for a member combo and
`pred_i <= self_i + input` otherwise. `persona turn` and `persona replay` carry no memory and add nothing. Before engine 0.14.0 a
strand with the block verifies from its header (`--from-checkpoint` refuses: the memory reads every event); from 0.14.0 the
memory is the canonical fold and every checkpoint line carries it (section 5.7, "Checkpoints"), so a replay and a resume start
at the last checkpoint. The deviation is absent
(`null`) when a program is not solved by the exact tier.

Every number of the stance's `deviation` is rounded to 6 decimals (the rule every other number of a stance and a state
follows), so a checkpoint line, which carries the stance, has the same bytes on every platform: `self`, `pred` and `input` come
from `ln`, whose last bits differ between libm implementations (macOS, glibc, the `libm` crate of a `nostd` build differ by a
few ULP). Engines before 0.14.0 wrote `self`, `pred` and `input` at full precision; a strand whose header names such an engine
keeps that formatting when it is replayed or continued, and `live verify` accepts its checkpoint line when it equals the
replay's at 6 decimals (the chain goes on from the logged bytes), so a strand written on one platform verifies on another.
The event line's receipt (5 significant digits) is checked exactly, as before.

**Rest pass (RFC 0015).** An optional `rest` key of the block declares the rest pass (memory consolidation) a host may run at
rest (standby, night, idle) with `Live::consolidate(at)` or `probbit live control STRAND rest [--at TIME]`:

```yaml
expectation:
  rest:                 # optional; every key optional; {} = the pass with nothing moving
    half_life: 64       # the window since the last pass re-folded with this half-life in events (default half_life_events)
    bucket: 2           # bins of a declared number input in the merge key, 1-3 (default 3 = the thirds: nothing merges)
    surprise_gain: 1    # weight gain of a context's mean z+ in the window, 0-4 (default 0)
    decay: 0.5          # factor on the learned deltas, 0-1 (default 1)
```

With the key, the canonical memory (0.14.0 on) carries a `window` (per remembered context [a, b, k, zs] in key order and
`since`; checkpoint lines carry it). The pass re-folds each count as count − a + b, weighs counts by 1 + g · min(zs/k, 1.5)/1.5
(total kept), merges contexts with the same key under B bins, multiplies the learned deltas by `decay`, and appends one
`consolidation` line: `{at, consolidation {after, before, decay, events, merged {count, keys}, moved}, n, prev, state}`.
`live verify` re-runs the pass and names the first field of a forged line; an engine before 0.14.0 refuses the line. Without
the key the pass is refused and every document is byte for byte as before. Full definition: docs/rfcs/0015-consolidation.md.

**Trajectory.** p_beta, proportional to p^beta, of the event's program at the k betas, each solved by the exact tier on
`Model::scaled(beta)`: no sampler, a pure function of the program. It is derived and never logged: `probbit monitor`
shows a `TRAJECTORY` row (mean marginal entropy per step) and its JSON frame a `trajectory` key; `probbit persona
trajectory PERSONA --state S [--inputs I]` (or `--program FILE` for any probbit-ir program) prints the steps. The joint
entropy of p_beta is non-increasing in beta (dH/dbeta = -beta Var(log w)); a single variable's marginal entropy can rise
(measured in RFC 0009).

### 2.18 Display glyphs (optional; since 0.13.0)

Since probbit 0.13.0. probbit 0.12.1 and earlier refuse a persona that has it (`display: unknown field`, exit 2).

One emoji per stance, computed by the engine from the stance document, never chosen by the model. Off by default and outside
the persona digest (as `bound` and `expectation` are): a persona without `display` gives every document byte for byte as
before, and adding the block to a persona with a recorded life leaves every state and stance digest as it was. It has
effects {}: it moves nothing.

```yaml
display:
  glyphs:
    moods: {valence: [down, even, up], pressure: [calm, steady, keyed]}   # two mood ids: rows, then columns, in table order
    table:                                  # one row per level of the first mood, one glyph per level of the second
      down: ["😔", "😕", "😣"]
      even: ["😌", "🙂", "😬"]
      up:   ["😊", "😄", "🤩"]
    surprise: {glyph: "👀", z_at_least: 2.0}  # optional modifier (default z_at_least 2.0)
    habits: {security_careful: "🔒"}          # optional: habit id -> glyph, first listed in force wins
```

**The rule order** (first base match wins; at most one base glyph and one modifier):

1. the first habit listed under `habits` that is in the stance's `habits.active`. Active means the habit applies this turn;
   bound means the habit-free stance would have broken it and the engine held it, so a habit whose inputs already take the
   stance where it points is active and not bound, and its glyph still shows. An unconditional habit (no `when`) is active
   on every turn, so listing one here shows its glyph on every turn;
2. otherwise the table's cell at the two moods' levels: `table[<level of the first mood>][<index of the second mood's level
   in its list>]`;
3. then the modifier: `surprise.glyph` appended when the stance carries `deviation` (an `expectation` block, section 2.17,
   on `probbit live`) with `registers: true` and `z >= z_at_least`. `persona turn` and `persona replay` carry no deviation,
   so they never add it.

The stance document gains `"glyph": "🙂"` and `"glyph_why"`: `"mood valence=even pressure=steady"`, `"habit
security_careful"`, or either followed by ` +surprise z=2.31` (z to 2 decimals). Both come last, after `block`; the
digest a strand line logs as `stance` excludes them, as it excludes `deviation`.

**The receipt.** On `probbit live` each event line carries `"glyph"`, and `live verify` recomputes it from the replayed
document: an edited, added or removed glyph diverges at its line as "the glyph differs". The same rules give the same
glyph in the stance document, the strand line, a host's footer and the monitor's header, so a mismatch between them is a
host bug, not a different state. A strand written with the block is read by an engine without it as `unsupported by this
engine: display` (not verified, not a divergence).

**Read strictly** (`persona check`, `persona lint` and every command that loads the persona refuse it at its path): `moods`
names two moods of the persona, each with its levels listed once; `table` has one row per level of the first mood, each a
list with one glyph per level of the second; every glyph is one Unicode code point that shows on its own, optionally
followed by U+FE0F (the emoji presentation selector, so `🛠️` and `❤️` pass) and by nothing else: the code point is not a
control or a space, and no zero-width joiner, other variation selector, keycap mark, skin-tone modifier, regional indicator,
tag or combining mark appears (so `👨‍💻` and `1️⃣` are refused; a terminal, a chat client and a device panel draw one glyph,
and a base glyph and its modifier never fuse into a third); `habits` names habits of the persona; no key is repeated;
`z_at_least` is a number >= 0. `persona lint` reports e.g. `display.glyphs.table.even[2]: one Unicode code point,
optionally followed by U+FE0F (no ZWJ, skin-tone, flag, keycap, tag or other combining sequence)`. A host that counts a
line's length in code points counts the selector as one more (`🛠️` is two, `🛠️👀` three). With the modifier the glyph is
the base followed by the modifier, each one code point plus an optional selector, so a host splits them at the first code
point (and its selector).

What it is: a lookup from the stance's mood levels, active habits and deviation to a symbol, so a person reading the chat
or the monitor sees the engine's state at a glance. What it is not: a measurement of anything beyond those fields.

### 2.19 Identity: the self keys (optional; RFC 0014)

Engine 0.14.0 and later; an engine before it refuses a persona that declares them (`identity.keep: unknown field`; `live
verify` of such a strand exits 5, `unsupported`). Four optional keys in the `identity` block declare which part of the
individual is "the self" and turn on integrity (section 5.9). They move nothing (effects {}) and stay out of the persona
digest, as `bound` does: declaring them on a persona with a recorded life leaves every state as it was.

```yaml
identity:
  name: Pip
  version: 1.1.0
  seed: 1
  keep: [no_jokes_on_loss, after_error_check]   # identity-bearing habits: reported held or not fired per event
  warn: 0.5                                     # a share of the allowance in (0, 1], default 0.8
  cue: hold the line                            # the persona's own words, handed to the host past warn (1-120 characters)
  traits: [humour, verbosity]                   # the traits measured; default learning.traits, else every trait
```

Refusals: a `keep` entry that is not a habit id of the persona, a `traits` entry that is not a trait (a mood is refused), an
entry listed twice, `warn` outside (0, 1], an empty or multi-line `cue`. The allowance per trait comes from the persona's
`bound` block (section 2.16); without one every allowance is null and `bound_used` reads 0.

### 2.20 Curiosity and the stir (optional; RFC 0016)

Engine 0.14.0 and later (an engine before it refuses the block, `curiosity: unknown field`; `live verify` of such a strand
exits 5, `unsupported`). A drive named `curiosity`, fed by the surprise the expectation block (section 2.17) measures, and
an optional self-started idle event, the **stir**, that it gates. It needs an expectation block and is in the persona digest
(its level moves the stance). Full definition, refusals and bars: docs/rfcs/0016-curiosity.md.

```yaml
curiosity:
  half_life_events: 8        # surprise_recent: the average of z+ = max(z, 0), its half-life in events (starts at quiet_z)
  quiet_z: 0.5               # below it nothing is new
  miss_z: 1.5                # a context's visit misses when z+ > miss_z
  last_k: 3                  # failed_contexts: the contexts whose last K visits all missed
  weights: {quiet: 1.0, failed: 0.5}
  cap: 2
  effects: {initiative: 0.6} # per unit of the level, on traits or moods (never pursue)
  stir: {threshold: 1.0, every_minutes: 30, class: check, when_asleep: {flag: night, every_minutes: 120, class: glance}}
```

**The level** = clip(quiet x max(0, 1 - surprise_recent / quiet_z) + failed x failed_contexts, 0, cap), 6 decimals. Event t's
turn reads the level after event t-1 as the engine-owned number input `cur_level` (box [0, cap], carrying `effects`; a host
that sends it is refused at `inputs.cur_level`; the stance's `inputs` leave it out). The stance gains
`drives.curiosity: {failed_contexts, level, surprise_recent}` and the event line the same object under `curiosity`
(`live verify` recomputes it). The drive's state is folded from the logged lines inside the expectation's memory, so a
checkpoint carries it (`expect.curiosity`) and resume and replay from a checkpoint read it back. `persona turn` and
`persona replay` carry no memory: the level is 0 there.

**The stir.** `Live::stir_due(idle_hours)` (and `probbit live control STRAND stir? [--idle-hours H]`) answers whether a
self-started event is due: the level at least `threshold`, at most one per `every_minutes` since the last stir; while the
last event had the `when_asleep.flag` on, only with `when_asleep`, at its slower rate and with its class, which never
speaks. A host that acts on it feeds an ordinary event with `"stir": "<class>"` and its own current readings; a habit in
force on those readings binds the stir's stance as any other (a persona that sleeps on a flag names that flag in
`when_asleep`). A stir the host ignores leaves no line. Without the block nothing changes: no `cur_level`, no `drives`
key, no line field, every document as before.

## 3. Compilation: persona + state + inputs → one probbit IR program

For turn t of an individual (seed s), with inputs x_t:

1. **Values.** `positional` (default): `l0 .. l(L-1)` (L = the largest number of levels) followed by the agenda slots;
   `semantic`: every level name in order of first appearance, then the slots. Both give the same distribution and stance
   (checked on every example).
2. **Trait variable** v, level l (allowed: its own levels):
   `h(v,l) = ln prior(l) + gene_v x c(l) + Δ_v(l) + sum over inputs i of gain_i x scale_i(x_t) x effect_i(v,l)
              + sum over history features f of min(value_f, cap_f) x effect_f(v,l)`,
   with `scale` = 1 for a true flag or the matching level, the value for a number input, 0 otherwise, and Δ_v the learned
   weights after this turn's feedback (section 2.8; 0 for a trait that does not learn). With a senses block (section 2.11)
   the sense step runs before this one and its lowering joins the inputs: `sense_<id>_up` and `sense_<id>_down` are number inputs with
   gain g_gain and the sense's effects (negated for `down`); a sense not read this turn adds nothing.
   2b. **`pursue`** (a drives block, section 2.9): one more trait variable over the goal ids, after the declared traits. Its
   field per goal g is step 2's (prior from `interest`, learned weights if `pursue` learns, input and history effects on
   `pursue`) plus `interest gene_g + want x W_g + glow x A_g + deadline / (1 + D_g / tau)` (each term rounded to 6 decimals,
   W, A and the deadline D after this turn's drive step), summed left to right and rounded. **The floor lift** follows: for
   each floor goal s the habits in force allow, the smallest L_s >= 0 (plus a 1e-5 margin, rounded up to 1e-6) with
   `exp(h_s + L_s + lo_s) / (exp(h_s + L_s + lo_s) + sum over allowed g != s of exp(h_g + L_g + hi_g)) >= floor_s`, where
   lo_g / hi_g are the least / most `pursue`'s couplings can add to goal g over every level of each partner. Several floor
   goals are lifted together (each lift raises the others' sums; at most 60 rounds); the lifts join the field as the
   contribution `floor` and the field is rounded again. The aggregates (wanting, afterglow, surprise) enter as number inputs
   and the goal conditions as flag inputs of the turn; `starve_<goal>` habits follow the declared habits (step 6). With a
   needs block (section 2.12) each need's term enters as the number input `need_<id>` (weight 1 on its bound goal, box
   [-pursue.cap, +pursue.cap]) and each `need.<id>.deficit` condition as a flag input; with `valence` the input `drv_expect`
   and with a window and `effects.omission` the input `drv_omit` (section 2.9) join the evidence. With a pace block (section 2.14)
   `pace_up` and `pace_down` join the inputs with gain g_pace; with `surprise.precision` each sense input's gain is multiplied
   by its precision factor (section 2.13).
3. **Mood variable** m: `h(m,l) = ln prior(l) + gene_m x c(l) + a_t(m,l)`, where the **mood accumulator**
   `a_t = k x d x a_(t-1) + (1 - k) x e_t`, e_t = the turn's input and history effects on m (with gains), k = `inertia`,
   d = 0.5 ^ (elapsed_hours / half_life_hours) (1 without them). An event's total impact over time is e_t whatever k is;
   inertia only spreads it out, so it smooths the mood without shifting the persona's average character.
4. **Agenda**: one variable per step over the slots, unary = `prefer` + step effects; `all_different` over the steps.
5. **Couplings** become `pairs` with a k x k `table` over the value alphabet.
6. **Habits in force** (conditions evaluated on x_t and the history): each `then` entry becomes a one-variable `tables`
   allow-list (`prev` resolved against the previous turn's level); `rules` are copied as written. Inactive habits are not in
   the program.
7. **Twin** (`engine.twin`): a copy of every trait, mood and step variable named `free.<id>`, with the same unaries,
   couplings and agenda structure and **no habits**, in the same program (an independent component, solved separately).
8. Every weight is rounded to 6 decimals; the program is serialised in a fixed order. Its sha256 is the turn's `program`
   digest.

**The steps of one event**, in order: validation and refusals (section 2.10, the senses' S1-S2, the credit rows, the needs'
N-E1-N-E4); the sense step (section 2.11); the needs step (section 2.12: growth, then each satisfaction's win or setback); the
drive step (section 2.9: decay and the windows' countdown, wins, an omission, wanting, caps; with an optimism gene the rate above or below E); the pace step (section 2.14); the credit step (section 2.8)
with the surprise gate (section 2.13); the
compile above; the solve, the stance and `why`; after the turn: `since`, a window opened for the goal a released stance
pursues, `await`. Nothing runs between events: growth and decay use the next event's stamped hours.

**The instruction is `probbit run --op decide` at fixed work** (`--seed S --sweeps N --polish-sweeps M --exact-limit X --chains C`),
in process: the program is built as a document and handed to `probbit run`'s own parser and instruction (no text round trip, no
child process). `probbit persona compile` prints it; `probbit run` with those flags answers it identically.
- `run`, because a persona is a general constrained categorical program (levels, soft pairs, caps, implications,
  precedences, linear budgets). `decide` is the task-router front-end (tasks to workers under quotas) and cannot express
  per-level unaries, couplings or these rules without contortion. `evaluate` is for a judge's per-question probabilities
  plus rules: it is the bridge when a model proposes the stance (section 8), not the per-turn path.
- `--op decide` answers exactly whenever the program is small or decomposable (every persona here: enumeration per
  component, about 0.2-0.7 ms of engine time) and falls back to the sampler and the diagnostics gate when it is not (a
  large, densely coupled character), which is where refusals come from.
- The whole program is enumerated before its components when its raw space (the product of every variable's levels, the
  twin's included) is at most `exact_limit`. A drives block multiplies that space by G² (`pursue` and its twin, G goals), so a
  small persona can land there: the drives fixture of the tests (four 3-level traits, a 3-level mood, four goals) has 944,784
  states and takes about 19 ms per turn, where without the block it has 59,049 (1.3 ms). Setting `engine.exact_limit` below the
  raw space (`exact_limit: 1000`) has the engine solve it per component instead: the same odds, 0.14 ms per turn (BENCHMARKS.md §9).
- Fixed work makes the answer a pure function of the program and the seed even when the sampler runs. `polish_sweeps: 0` means
  no plan polish (never the wall-clock one, which would make a sampled turn depend on the machine's speed).
- The turn seed: `S = first 4 bytes of sha256("probbit-persona/1|turn|<name>|<seed>|<turn>")`, masked to 31 bits.

## 4. The stance document (one per turn)

```json
{"probbit_persona_turn": 1,
 "persona": {"name": "Rook", "version": "1.0.0", "digest": "sha256:..."}, "seed": 3, "turn": 5,
 "status": "ok",
 "engine": {"verdict": "exact", "tier": "components", "program": "sha256:..."},
 "stance": {"verbosity": {"level": "terse", "p": 0.81, "odds": {"terse": 0.81, "short": 0.17, "full": 0.02}, "released": true}, "...": {}},
 "mood": {"valence": {"level": "down", "p": 0.44, "odds": {"down": 0.44, "even": 0.53, "up": 0.03}, "released": true}},
 "agenda": ["acknowledge", "diagnose", "fix", "verify", "report"],
 "habits": {"active": ["after_error_shorter", "high_stakes_careful"], "bound": ["after_error_shorter"],
            "violations": 0, "conflict": [], "yielded": []},
 "unsure": [], "held": [], "escalate": null,
 "inputs": {"error": true, "stakes": 0.8, "errors_in_row": 2, "...": "..."}, "ignored": [],
 "line": "Stance: shorter than last time, check more; 1-2 sentences; verify before claiming anything; no jokes.",
 "line_tokens": 24,
 "why": "shorter than last time, check more (habit); last action failed -> caution up, verbosity down",
 "state_digest": "sha256:..."}
```

- `level` is the variable's value in the engine's joint plan (the most likely stance that obeys every habit); `odds` are its
  exact marginals (or gate-checked estimates on a sampled answer). A level can differ from its own most likely value because
  the plan must fit the other traits (the IR's "plan or odds" rule): act on `level`, read uncertainty from `odds`.
- `habits.active`: habits in force this turn. `habits.bound`: the active habits the habit-free twin breaks, i.e. the ones
  that changed the stance. `violations` is always 0 when there is a stance (checked independently by the reference
  implementation on every turn).
- `status`:
  - `ok`: the engine answered `exact` or `diagnostics_passed`.
  - `partial`: some variables did not pass the gate (`released: false`); their phrases are left out of the line.
  - `refused`: nothing is vouched for; the line carries only the habits in force (hard rules are always safe to state).
  - `fallback`: no stance at all (the engine proved the habits contradict on this turn, or failed): levels = the `fallback`
    levels, line = the non-conflicting habits or `neutral`, `escalate` says why. `habits.conflict` names a minimal set of
    contradicting habits (deletion filter: one extra engine call per active habit, only on such turns).
- `unsure` / `held`: traits whose top odds are under their `vouch` floor; with a `hold` level they are re-solved clamped to
  it (a second engine call), so the stance stays consistent with every habit.
- `line`: phrases by priority — (1) bound habits, (2) other conditional habits in force, (3) unsure notes, (4) traits off
  this individual's resting level, (5) traits at rest, (6) agenda order — cut from the lowest priority until it fits
  `max_tokens`. Unconditional habits that did not bind are not repeated (the trait phrases already carry them).
- `why`: the bound habits, then the two strongest live inputs with the two traits each pushes hardest, by direction
  (`learner upset -> valence down, humour down`): a push is evidence, not the level the stance took. With a senses block the
  normal clause comes before the inputs and the learned clause after them (section 2.11).
- With a senses block (section 2.11): `"senses": {"tool_fail": {"raw": true, "src": "env:runner", "z": 3.878359, "zd":
  3.378359, "mean": 0.05, "n": 0}}` per sense read this turn (the `mean` and `n` it was judged against) and `"noticed":
  "tool_fail"` (the normal clause's sense, or null); `inputs` echoes `senses` as given.
- With a credit block (section 2.8): `"credit": {"paid": {"turn": 3, "e": 0.937084, "sign": 1, "absorbed": false, "stance":
  "sha256:..."}, "closed": [{"turn": 3, "as": "paid"}], "open": [5, 6], "stance": "sha256:..."}`: the payment this turn made
  (null when none), the decisions it closed (by a control line, expired, cancelled, paid, evicted, in that order), the turns
  still open, and this stance's own digest (what `for: {turn, stance}` names); `inputs` echoes `await`, `for` and `cancel`.
- `held`: the unsure traits held at their `hold` level; such a trait also carries `odds_unheld`, its odds before the hold.
- With a drives block (section 2.9), `pursue` leaves `stance` for its own object and the turn's drive values follow it:
  `"pursue": {"goal": "fun", "p": 0.44, "odds": {"chores": 0.10, "craft": 0.26, "fun": 0.44, "safety": 0.20}, "released": true,
  "say": "play", "lift": {}}` (`lift`: the floor lift per goal on this turn, empty when none was needed) and `"drives": {"want",
  "glow", "expect", "surprise", "deadline", "since"}` (per goal, as this turn read them). The line clause `pursue: <goal>: <say>`
  ranks as a trait phrase, ahead of the other traits unless `line.order` places it; `inputs` echoes `goals` as given. With a
  window (section 2.9) `drives` adds `"window": {"report": 1.5}` per goal with `expect_by_hours` (hours left, or null).
- With a needs block (section 2.12): `"needs": {"context": {"deficit": 0.55, "forecast": 0.61, "term": 1.109371}}` per need
  (after this event's needs step); `inputs` echoes `needs` as given.
- With a pace block (section 2.14): `"pace": {"level": "steady", "rbar": 0.97}` (the average after this event's pace step);
  `inputs` leaves out `pace_up` and `pace_down`.
- With a surprise block (section 2.13): `"surprise": {"<trait>": g}` for the learned traits whose gain is above 1 (`{}` when none
  is).
- With `display.glyphs` (section 2.18): `"glyph"` and `"glyph_why"`, last.
- Timing is never part of the stance: `--timing` adds a separate, non-canonical `timing` object (compile, engine and decode ms,
  engine calls, the 1-minute load average).

## 5. State, replay and determinism

### 5.1 State

```json
{"probbit_persona_state": 1, "persona": {"name": "...", "version": "...", "digest": "sha256:..."},
 "seed": 3, "turn": 5,
 "genes": {"shift": {"warmth": -0.2135, "...": 0}, "react": {"error": 1.1847}},
 "mood": {"valence": [-0.31, 0.0, 0.12], "arousal": [0, 0, 0]},     # accumulators a_t (log-weights per level)
 "history": {"error": [false, true, true]},                         # recent flag values (as long as the longest window / cap)
 "prev": {"verbosity": "short", "...": "...", "agenda": ["..."]},  # last turn's levels (for prev-relative habits)
 "rest": {"warmth": "cool", "...": "..."},                          # the individual's resting stance (no evidence), set at init
 "learned": {"verbosity": [-0.12, -0.31, 0.43]},                    # with a learning block (section 2.8): Δ per level
 "credit": {"verbosity": [-0.08, -0.36, 0.44]},                     # with a learning block: the last stance's onehot - odds
 "drives": {"genes": {"interest": {"fun": 0.12}, "gain": {"fun": 1.08}},   # with a drives block (section 2.9), per goal:
            "want": {"fun": 0.61}, "glow": {"fun": 0}, "expect": {"fun": 0},   # wanting, afterglow, expectation, the last
            "surprise": {"fun": 0}, "deadline": {"fun": null}, "since": {"fun": 0},    # error, deadline hours, turns since,
            "window": {"fun": null}},                                       # and (expect_by_hours) the omission windows
 "needs": {"genes": {"weight": {}, "growth": {"contact": 1.0914}},            # with a needs block (section 2.12): genes,
           "deficit": {"context": 0.3, "contact": 0.12}, "success": {"compact": 1, "report": 0.8}},   # deficits, s per goal
 "surprise": {"verbosity": {"v": 0.15, "r": 2, "g": 1.666667, "counts": [[0, 1], [0, 0], [0, 0]]}},   # with surprise (2.13)
 "pace": {"rbar": 0.97},                                                     # with a pace block (section 2.14)
 "dispositions": {"optimism": 0.1421, "patience": 1.0815},                   # genes with a spread above 0 (2.15)
 "senses": {"genes": {"gain": {"tool_fail": 1.19}, "threshold": {"tool_fail": -0.17}, "habituation": {"tool_fail": 1.46}},
            "norm": {"tool_fail": {"mean": 0.3, "var": 0.2, "n": 10}}},  # with a senses block (section 2.11): per sense
 "learned_from": {"caution": {"rewards": 3, "corrections": 2}},     # with learning and line.why_past: feedback turns by sign
 "open_credit": [{"turn": 5, "credit": {"verbosity": [0.1, -0.4, 0.3]}, "stance": "sha256:..."}],   # with a credit block
 "prev_stance": "sha256:...", "control_closed": [],                 # (section 2.8): open decisions, digests, control closures
 "digest": "sha256:..."}
```

The state is plain JSON the host stores between turns (a file, a database row, a session field). A turn refuses a state
whose persona digest differs from the persona file, whose own digest does not match, or whose genes do not match its seed, and
(with a learning block) learned weights outside ±total_cap or credits outside ±1, (with a drives block) drive genes that do not
match the seed, a drive value outside its box (wanting, afterglow and expectation within their caps, the error within ±pe_cap,
deadlines 0 to 1e6 hours or null, `since` a whole number >= 0) or a goal set that differs from the persona's, and (with a
senses block, rule S4) a mean outside prior ± drift_cap, a variance outside [0, var_cap], a count that is not a whole number up
to 1e6, a sense set that differs from the persona's or sense genes that do not match the seed, even with a recomputed digest.
Each normal starts at its prior with n = 0; a persona without the block refuses a state with a `senses` field. With a credit
block (section 2.8) the state keeps `open_credit` (the decisions awaiting an outcome: at most `max_open`, turns increasing and
before the state's turn, credits within ±1, a stance digest each), `prev_stance` (the previous stance's digest, null before any
turn) and `control_closed` (decisions a control line closed, listed by the next stance); a persona without the block
refuses a state with any of them. With a needs block the state is refused, even with a recomputed digest, when a deficit
leaves [0, cap], a success rate leaves [0, 1], a window leaves [0, `expect_by_hours`] or null, the need set or the bound goals
differ from the persona's, or a need gene misses its seed; a persona without the block refuses a state with a `needs` field. The fields of sections
2.13 to 2.15 are refused the same way, at their paths: a gate's v outside [0, 1], r outside [1, r_max], g outside [1, g_max], a
negative count; R outside [0, cap]; any of them past 6 decimals; genes that miss the seed; a field whose block the persona
lacks, or a block without its field.

### 5.2 Canonical JSON and the number rule

Every document a persona produces, and every text it hashes, is printed canonically: object keys sorted by code point, no
whitespace, UTF-8 (no `\u` escape except for control characters; `"`, `\`, `\b`, `\f`, `\n`, `\r`, `\t` escaped as in JSON),
and every number by one rule:

- a number whose value is a whole number of magnitude at most 2^53 prints as an integer: `1`, `-2`, `0` (so `1`, `1.0` and `1e0`
  are one number in every digest, and so are `0` and `-0.0`);
- any other number prints as the shortest decimal that reads back to the same double (an exact tie between two such decimals goes
  to the even one), in plain notation when its decimal exponent e satisfies -5 < e < 16 (`0.43543`, `0.0001`, `123.25`) and in
  exponent notation otherwise (`1e-05`, `1.5e-07`, `1.2e+16`): Python's `repr` of a float.

The persona digest is the sha256 of the persona document printed so; the state digest of the state without its `digest` field;
`engine.program` of the program text, which takes the same number rule but keeps the compiler's key order. A host that rewrites a
state with another JSON library (Python writes `0.0` where probbit wrote `0`) still has the same state.

### 5.3 Guarantees

- A turn is a pure function of (persona file, state, inputs, engine version): the same four give the same stance and new
  state, byte for byte (canonical JSON). Replaying a script from `init` reproduces every stance.
- The engine runs at fixed work, so this holds on the sampled tier too (the engine's documented fixed-work determinism).
- Genes use the platform's `ln`, `sqrt` and `cos` and are rounded to 4 decimals; weights to 6. A last-place difference in a
  maths library could, in principle, flip a rounding boundary on another platform (not observed; not proven impossible).
- The 0.5.0 documents equalled an independent reference implementation's (Python, written before this one, from this document) byte
  for byte on the example personas: their state0, all 20 workday stances and the final state (the 0.5.0 goldens), and on 5,400
  further turns of random inputs over every input each persona declares, three seeds each, as written, with `values: semantic`
  and forced onto the sampler, held, yielded, refused and fallback turns included (measured on 2026-10-02). 0.6.0 changed two
  things that check did not cover: `why` names a direction (`humour down`) where it named a level, and the tutor has one habit
  more. Its goldens in examples/persona/golden/ are regenerated from this implementation; the parity claim is for 0.5.0.

### 5.4 Individuality and distance

`gene_v = round(spread_v x z(name, seed, "shift", v), 4)` and `gain_i = round(exp(reactivity_spread_i x z(name, seed, "react", i)), 4)`,
with `z` a standard normal from Box-Muller on the first 16 bytes of `sha256("probbit-persona/1|" + parts joined by "|")`.
Genes depend on the name, the seed and the variable id only, so editing an unrelated part of a persona keeps its individuals.

**Distance between two individuals** over a script (the same inputs for both): the mean, over turns and the traits both
declare with the same levels, of the total-variation distance between their trait odds (0 = identical, 1 = disjoint); and
the fraction of (turn, trait) where their levels differ. Identical persona + seed + script → 0 / 0.

### 5.5 Contradicting habits

Two hard habits can contradict on some turns (for example "after an error, shorter than last time" against "at high stakes,
at least a short paragraph" when the previous reply was already short). The engine then proves that no stance obeys them all
(`infeasible`). With `on_conflict: fallback` the turn has no stance (status `fallback`, `habits.conflict` lists the
contradicting habits, the line omits them). With `on_conflict: yield` the lowest-priority habit of the conflict yields (on a
tie, the later-declared one), the turn is re-solved, and `habits.yielded` and `escalate` say so.

**Lint.** `lint` searches every single conditional habit and every pair of them (with all unconditional ones and every
previous level their `prev` restrictions read) for an empty stance, and reports each contradiction with its resolution.
Run it before shipping a persona.

**Exit codes.** `lint` exits 0 when every contradiction is resolved and, with rules (`--never RULE` / `--props FILE`, section
5.6), every rule is held by construction or proved; 1 when a contradiction is
unresolved or a rule has a counterexample; 2 on a bad persona, rule or flag; 3 (since 0.8.2) when nothing is broken but some
rule is **unproven**: `prove` left it `unknown` and the default `fuzz` search found no counterexample. Unproven is not broken,
and it is not green either: the JSON's `ok` is false and `unproven` lists each such rule as `{id, rule, seeds_unknown,
searched}` (`seeds_unknown`: the individuals the bound could not decide; `searched`: `individuals`, `scripts`, `depth`, `beam`,
`grid`, `hours`), and stderr says "unproven, not broken". Exit 1 wins over 3. A CI gate that should accept unproven rules
checks for exit 3 explicitly; up to 0.8.1 the same case exited 0 with `ok: true`.

**Plan and mode.** A stance level is the joint plan's, so on coupled traits it can differ from the trait's own most likely level
(section 4): the 0.5.0 tutor's seed 1, told the learner is upset, planned humour `playful` (odds 0.411) while `light` had 0.425,
and valence `up` (0.386) while `even` had 0.457; the two are coupled (`align: 1.0`) and the plan took their high levels
together. `lint` also probes every individual of `--seeds` (default 0-99): at rest, a quiet turn, then every declared input
alone (numbers at their maximum). `warnings` lists each trait whose planned level differs from its marginal mode on a released,
not unsure, probe turn, with the count and the earliest case (`example`). A warning is advice and leaves the exit code alone.
The options, none of which changes decoding: a habit pins the level where it matters (what the tutor does now); a `vouch` floor
with a `hold` holds the trait when its top odds are low; a host that wants each trait's own most likely level can read it from
`odds`.

### 5.6 Testing a character

A **character property** is a rule in habit syntax that a stance must never break, for example "never playful when the
learner is upset": `{when: {sentiment: negative}, then: {humour: {at_most: light}}}`. It is checked on the stance a host gets
(status `ok` or `partial`, released traits; `prev` restrictions read the previous turn's levels), never on the words a model
writes. Two commands test it over a population of individuals (`--seeds 0-99`); `--never RULE` gives one rule (JSON or one
line of YAML), `--props FILE` several (`{"props": [...]}`, each with an optional `id`).

**`fuzz`** searches event scripts built from the persona's declared inputs (flags, levels, numbers on `--grid` x their max,
idle hours on `--hours`). The grid (default 0, 0.5, 1) also gets, after the given points and each once, every threshold the
rule's `when` and the habits' `when` read on a number input, as a fraction of that input's max (since 0.8.2): a rule in force
only at stakes 0.7 to 0.8 is searched at 0.7 and 0.8; `search.grid` in the JSON lists the grid used. The search runs random
scripts, then a beam guided by the exact odds (the next events that put the most odds on a
level the rule forbids). Each individual's shortest counterexample is shrunk (drop events, then single inputs) and printed with
the breaking turn's levels, odds, why and line, and the `replay` / `explain` commands that reproduce it. The search is Philox
keyed by `--fuzz-seed` and the seed: the same inputs give the same report, byte for byte, on any number of threads. "None
found" is evidence over the scripts searched, not a proof.

**`prove`** gives each rule one verdict over the population:

- `held by construction`: habits that are in force whenever the rule applies (their conditions follow from its own) imply it,
  for every previous level. With `on_conflict: yield` a habit counts here when no turn where the rule applies ever drops it
  (a scan of every cell replays the conflicts; which habit yields depends on the habits in force and the previous levels, not on
  genes or moods).
- `proved for every event sequence`: for each individual, a bound covers every turn of every script and decides all of them.
  The mood accumulators stay in a box (per level, between the most negative and the most positive evidence one turn's inputs
  and history can add, 0 included), whatever the inputs, idle hours or `--no-inertia`. Learned weights (section 2.8) of traits
  linked to the rule's trait stay in their own box, ±total_cap per level, whatever the feedback. The rest of a turn falls into finitely
  many cells: flags and levels, each number at every threshold a habit or the rule reads and on each interval between them,
  history values that fit the turn's own flags, and the previous levels the restrictions in force read. In every cell the best
  stance the rule allows must beat the best one it forbids by more than the box, the number intervals and the 6-decimal
  rounding can move their scores. Moods linked to the rule's trait by no coupling or rule cannot move it and are left out.
  Held traits and habit conflicts are replayed as the turn resolves them.
- `unknown`: the bound could not decide some individual. The report names the earliest such cell (inputs, history, previous
  levels) and the `fuzz` command that searches it. Unknown is not broken: run `fuzz`.

Example: the tutor as it shipped in 0.5.0 (kept byte for byte as `probbit-cli/tests/fixtures/persona/tutor-0.5.0.yaml`) breaks
"never playful when the learner is upset" for 67 of seeds 0-99: 37 on the single message `{"sentiment": "negative"}`, 27 when
that message also carries praise, 3 on two messages; `prove` says unknown. 0.6.0's tutor adds one habit, `no_play_when_upset` (`when: {sentiment: negative}`, `then: {humour: {at_most:
light}}`): `prove` says held by construction and `fuzz` finds nothing. The habit fixes the stance the host gets; whether a model
writes jokes anyway is the model's (section 6).

Limits. Both commands cover the stance, not the model's words (section 6). `prove` reads exact engine answers: a cell whose
program leaves the exact tiers is `unknown`. `held by construction` relies on every plan the engine returns keeping
every rule in force. A rule with raw `rules` (not `then`) is fuzzed but not proved. The bound is per cell with the moods at
the edges of their box, so a soft rule whose margin is small stays `unknown` even when no script breaks it.

**With a drives block** (section 2.9). A rule may name `pursue` in `then` and, in `when`, the goal conditions the persona's
habits use (the same goal, signal and range). `fuzz` adds goal signals to its events: per goal a win on `--grid` x `win_max`,
a cue, a deadline on each `--hours` value. `prove` reads the drives as boxes: the aggregates as number inputs over their whole
range (wanting and afterglow 0 to their caps, the prediction error 0 to `pe_cap` each way), the goal conditions both ways, and,
when `pursue` is in the rule's component, its drive terms (wanting, afterglow, deadline urgency, the floor lift at its most over
that box) widen the bound. A goal floor is reported held by construction, with the habits that can exclude it (`floors` in the
JSON). If a checked individual's goal-field bounds (including couplings) leave the normal exponential range, its floors are
reported `unknown`: the log-space lift still runs, but the structural proof does not certify floating-point rounding over
every turn in that range. On the drives fixture of the tests: "a chore due within 24 h -> pursue chores" is held by construction (its habit);
"`since_pursued.chores` >= 12 -> pursue chores" is unknown, rightly, since `starve_safety` (priority 1) wins over `starve_chores`
(priority 0) on a turn where both are due.

**`lint --props FILE`** (or `--never RULE`) runs both for CI: `prove` on every rule, then `fuzz` (the default search) on the
ones it leaves unknown; the lint document gets `props` (each rule's `prove` entry, with a `fuzz` entry when it was unknown) and
`broken` and `unproven`; exit 1 when a contradiction is unresolved or a rule breaks, exit 3 when a rule stays unknown and the
search finds no counterexample (section 5.5).

Exit codes: `fuzz` 0 nothing found, 1 a counterexample; `prove` 0 every rule held or proved, 1 some rule unknown; both 2 bad
input. JSON: `--json` (`probbit_persona_fuzz: 1`, `probbit_persona_prove: 1`); timing goes to stderr.

**Source-restricted fixtures.** For a persona with `reward_from`, exported abstract counterexamples have no runnable
`replay` or `explain` command unless a synthetic source is explicitly authorized with
`--fixture-src env:synthetic` (or `human:synthetic`, where allowed). The exporter strictly replays that labelled script;
it rejects incompatible source rules, records `fixture_provenance`, and never bypasses the production source check.
With a senses block the labels cover readings too (a bare reading takes the event's synthetic `src`).

**Senses in fuzz and prove** (section 2.11). `fuzz` adds readings to its events (nested in `senses`): flags both ways,
numbers on `--grid` × the range, one reading repeated 1, 5 and 20 times, and the range ends alternating. `prove` reads each
`sense_<id>_up/down` input as a box [0, zcap] × gain and each sense condition both ways, so a habit on `sense.<id>.raw` holds
by construction for a rule with the same condition. A rule reads the sense conditions the persona's habits use.

**Surprise and pace in fuzz and prove** (sections 2.13, 2.14). With a pace block `fuzz` adds reward bursts: each goal's top win
and the reward flag, at elapsed 0, repeated 1, 5 and 20 times (the average runs to its cap); with a surprise block, feedback
alternating between the reward and the correction flag over 20 events (the gate opens). The alphabet never carries `pace_up`
or `pace_down` (the engine sets them). `prove` reads them as boxes [0, cap − r0] and [0, r0] times g_pace in their traits' and
moods' boxes; the learned box is unchanged by `surprise` (g scales the step, never `total_cap`).

**Needs in fuzz and prove** (section 2.12). `fuzz` adds amounts to its events (nested in `needs`) on `--grid` × (0, cap] per
need, each amount repeated 1, 5 and 20 times (satisfactions past the deficit overshoot), and each window's length as idle hours
over 6 events (a pursued goal's window expires); the hours alphabet gains the windows' lengths. An event that pays a need and
its bound goal's own win at once is refused (N-E3) and skipped. `prove` reads each `need_<id>` input as a box [-pursue.cap,
+pursue.cap] on its bound goal, `drv_expect` as [0, win_max] and `drv_omit` as [0, pe_cap] in their moods' and traits' boxes,
and each `need.<id>.deficit` condition both ways, so a habit on `need.<id>.deficit` holds by construction for a rule with the
same condition (`examples/persona/body-props.json`).

**Payments in fuzz** (section 2.8, with a credit block). `fuzz` adds `await` to its events and payments resolved against the
state each event runs on: `for` the oldest open decision (late), the newest (reordered), the previous stance, a past turn no
longer open (a duplicate, or a payment after a cancel or an expiry), a turn still to come (unknown), or an open decision bound
to another stance's digest; `cancel` the oldest open decision or a turn still to come. A refused payment changes nothing, so
the search goes on from the same state, and a reported counterexample's script holds only the events that ran, with every
payment as a concrete turn, so it replays as printed. A persona without the block is searched exactly as before.
The Python `persona_fuzz` and MCP fuzz tool accept `fixture_src` too. See the
[host regression example](../examples/agent-harness/README.md). Synthetic observations are not production evidence.

**Output formatting.** Single-document persona commands accept `--pretty` for stdout: init, turn, diff, lint, check,
compile, describe, fuzz and prove (for fuzz/prove it implies `--json`). Written states and programs remain canonical;
replay remains JSONL and explain remains text, so those commands reject `--pretty` with guidance. The
`probbit_persona_turn` field is the document schema version, not its zero-based `turn` counter.

### 5.7 Live: a resident individual

`probbit live` keeps one individual running: JSONL events in (one object of inputs per line, from `--events FILE` or stdin),
one stance per event out (canonical JSON, the document of section 4). It adds three things to `persona turn`: a clock, learning
from feedback between events (with a learning block, section 2.8), and a **strand**, a log the whole life replays from.

```sh
probbit live examples/persona/tutor.yaml --seed 2 --strand pip.strand < events.jsonl    # one stance per event; pip.strand logs it
probbit live verify pip.strand                                                         # ok, or the earliest line that differs
probbit live examples/persona/tutor.yaml --seed 2 --demo week --plain                 # a scripted week (below)
```

**Time.** With `--clock real` (the default) a monotonic clock starts with the run and stamps each event's `elapsed_hours`, the
time since the previous event (for the opening event, since the start), quantised to 1e-6 h (3.6 ms): the value the turn uses
is the value the strand logs. Under the real clock an event that carries its own `elapsed_hours` is refused. With `--clock fixed`
each event carries its own `elapsed_hours` (default 0), so a run is a pure function of its events (tests, demos, replays).

**What decays.** Moods, and with a drives block the drives (wanting, afterglow and expectation by their own half-lives,
deadlines counting down: section 2.9), nothing else. Between two events each mood accumulator is multiplied by 0.5 ^
(elapsed_hours / half_life_hours) (section 3, step 3), so quiet hours bring a mood back to the individual's resting level. A test holds it to the
closed form: an event, then quiet events after gaps g_1..g_n, leaves k^n x 0.5^((g_1 + ... + g_n) / half_life_hours) x a_1, to
the 6 decimals the state keeps.

**Goals.** An event of a persona with a drives block carries its goal signals under `goals` (section 2.9); the strand logs the
inputs as given, `goals` included, and `verify` replays them.

**What learns.** With a learning block, a turn whose reward or correction flag is on moves the learned weights Δ of the levels
the previous stance took (section 2.8), never past ±total_cap per level. Without a block nothing learns, and every stance and
state is the one `persona turn` gives.

**What never changes.** Genes, priors, couplings, input effects, history features, habits and every trait's allowed levels. The
learner's whole output is the Δ table, and habits are rules of every compiled program (section 3, step 6), so every stance obeys
every habit in force whatever Δ is: rule immunity is architectural, not trained or measured into it, and `prove` (section 5.6)
includes the learned box in its bound. Measured all the same (BENCHMARKS.md §8): an adversary that praises every joke and
criticises every joke-free stance, the ones a failure forced included, over 10,000 turns of the tutor on each of seeds 0-9,
broke no rule; on all 33,330 failure turns humour was `none`, while the learned humour weights sat at their cap from turn 8-26 on.

**How far.** Learning saturates at its box: 200 and 2,000 feedback turns leave an individual at the same distance from its initial
self, so `total_cap`, not the time in use, sets how far use can move an individual. On the tutor (learners seeds 0-19, compared
with the 100 initial individuals of seeds 0-99 by the distance of section 5.4), total_cap 0.25 left every learner nearer its own
initial self than any sibling; total_cap 1, more than the genes' spread on those traits (0.5-0.6), left 9 of 20 with a sibling as
near or nearer. Keep total_cap well under the learned traits' spread when an individual must stay recognisably itself. Feedback
credits every learned trait with the level it took, so a reward aimed at one trait also pushes the levels the others took (in the
demo below, praise for short answers also raised the light joke the tutor was making): learn the traits the feedback judges.

**The strand.** `--strand FILE` logs the life. Line 1 is the header, compact JSON:
`{"probbit_strand":1,"engine":"probbit 0.14.0","persona":{"name","version","digest"},"seed":N,"state":{...},"document":{...}}`,
with the initial state as canonical JSON (one individual has one header, whether it comes from `init` or from a state file) and
the persona document in its own key order (the compiled program, and `engine.program` with it, follows the document's key order).
Then one line per event, canonical JSON: `{"inputs":{...,"elapsed_hours":h},"n":k,"prev":"sha256:...","stance":"sha256:...",
"state":"sha256:..."}`, that is the inputs as the turn used them (the stamped hours included), the sha256 of the line before (the
header, for event 1), the sha256 of the stance's canonical JSON and the new state's digest (with `display.glyphs`, section 2.18,
the line also carries `"glyph"`, which `verify` recomputes). Lines end in `\n`. A strand is never
rewritten, just appended to: `--strand` on an existing strand continues it from `--state`, which must be the state after its last
line (that line carries its digest) of a persona with the header's digest, and the run uses the header's document. A life
continued over several runs is the strand of one run, byte for byte.

**Verify.** `probbit live verify STRAND` reads nothing but the strand. It rebuilds the persona from the header's document (its
digest must match), checks the initial state as any state is checked (section 5.1), rebuilds the header, replays every event on
the fixed clock with its logged hours and compares, line by line, `prev`, `n`, the stance digest, the state digest and the bytes.
It prints `{"ok":true,"events","persona","seed","engine","verified_by","final_state","last_line"}` and exits 0, or prints
`{"ok":false,"line":n,"diverges":"..."}` for the earliest line that differs and exits 1: a changed input diverges at its own line
(its stance or state no longer replays), a removed or reordered line at the next line's `prev`, a changed header at line 1. A copy
with `\r\n` line endings (Windows text mode, an editor) verifies and can be continued: the hashes cover each line's text without
its line ending, and a canonical JSON line never ends in `\r`. Verify replays with the binary that runs it; the header's `engine`
names the version that wrote the strand (a development build writes its `-dev` version, so its strands are told from a release's),
and `verified_by` (since 0.12.1) the version that replayed it, so a strand begun before an upgrade verifies under the new engine.
Measured (BENCHMARKS.md §8, Apple M4): a 10,000-event strand of the tutor is 3,055,367 bytes and verifies in 3.3 s, about 3,000
events per second.

**Unsupported is not diverges.** A newer engine's strand can carry what this engine does not know: a header key, a persona
block or field (`senses` and `learning.credit` for 0.8.2; RFC 0002 part 2's `load` for this one), a state field, a line kind (a `rest`
line) or a field on a known line. `verify` stops at the earliest such line, prints
`{"engine":"<this engine>","line":n,"ok":false,"strand_engine":"<the header's engine>","unsupported":"<field>"}`, says
`probbit: live verify: line n: unsupported by this engine: <field> (written by <the header's engine>; this engine is <this
engine>): not verified, and not a divergence` on stderr, and exits 5. Nothing from that line on was replayed: the strand is not
verified, and not shown to differ either; verify it with the engine that wrote it. The field is named as the persona validator
names it (`needs`, `learning.credit`), as `state.<field>` in a state, and by its path in the line otherwise (`rest`,
`inputs.senses.tool_fail.z`). What this engine knows is still checked: a value it does not accept in a field it knows diverges
(a changed input, a changed header value), and the chain is checked past an unsupported line, so an edited, removed or
reordered line diverges at the earliest line whose `prev` shows it (exit 1); `unsupported` is left only for a strand whose
chain is whole. An edit that adds a field this engine does not know, with the chain rebuilt after it, reads as unsupported:
exit 5 is not a pass either. A newer engine that computes a known field differently (another stance or state digest) cannot be
told from an edit: such a strand diverges at the earliest line where it differs, and the header's `engine` names the engine to
verify it with. The released 0.8.2 binary predates this: on a 0.9.0 strand it prints `{"diverges":"the persona document:
senses: unknown field (allowed: ...)","line":1,"ok":false}` and exits 1 (`probbit-cli/tests/fixtures/strands/` keeps that
output next to the strand). MCP's `probbit_live_verify` and Python's `live_verify` return the same document: like a
divergence, it is an answer, not an error.
`probbit check` counts an unsupported strand as one that fails the gate (exit 1) and prints `line n: unsupported by this
engine: <field> (written by <engine>)` for it, never FAIL; its `--json` entry adds `unsupported` and `strand_engine`.

**Following a file.** `--events FILE --watch` follows the file as lines are appended (as `tail -f`): a line counts once its
newline is written, and the run stops when the file is removed (tested on Linux and macOS).

**The demo.** `probbit live PERSONA --demo week [--seed N] [--strand FILE] [--plain]` runs one individual through a scripted week
on the fixed clock, events 1 h apart from 09:00 to 15:00 and quiet nights until 09:00. Campaign 1 praises every answer shorter
than the longest level and criticises the longest until the learned verbosity weights reach their cap (day 3 at the latest); an
hour later comes an upset (a failure, with a negative sentiment when the persona has one), then the night and a morning without
feedback; campaign 2 praises every joke to the end of day 7, with a failure every third hour. A persona without a learning block
gets the demo's, `{"from":["praise","criticism"],"traits":["verbosity","humour"],"rate":0.5,"step_cap":0.2,"total_cap":1}` (its
opening line says so, and the strand records the document used). The script reads nothing but the stances, so a run is a pure
function of persona, seed and version. On a colour terminal it draws bars on stderr, 1 s per hour with each night fast-forwarded
in 2 s (about 55 s in all); otherwise, or with `--plain`, it prints one line per event on stdout without waiting. The tutor, seed 2,
0.7.0: 50 events over 150 h of clock; the learned verbosity bar at its cap on day 1 at 15:00 (event 7); valence `down` at the
upset and `up`, this individual's resting level, after 17 quiet hours; P(joke) on the turns without a failure, mean per day,
0.57 on day 1, 0.78 on day 2 and 0.89 on each of days 3-7, with humour's learned weights ending at [-1, +1, -0.46] for none /
light / playful; 13 failure turns, a joke on none of them; 0 rule breaks; a 21,361-byte strand that `live verify` replays.

**One writer per strand.** `--strand FILE` takes the writer lock `FILE.lock` before it reads the strand and holds it to the end of
the run: the lock file is created exclusively (written aside and hard-linked into place, so it is never seen half written) and
holds the writer's process id and start time. A second writer finds it and exits 4 with `{"error":{"code":"locked",...}}`
naming the holder's pid, and changes nothing. A lock whose process is gone is stale and is taken over; a lock that names no
process, or a process id reused by another program, stays held until a person removes it. Before every append the writer
checks the lock is still its own, so a lock removed or taken over under a running writer stops it before it writes. Two
`live` runs started on one state and strand (the two-writer case of an earlier test round) now give exactly one exit 0 and one
exit 4, and the strand verifies. `probbit_live_event` (MCP) and `live control` take the lock for their one append, `--demo
week --strand` for its week.

**Control lines: pause, resume, retire.** A person (or a sensor) stops an individual in its own log:

```sh
probbit live control pip.strand pause --by human:owner --reason "checking the last answers"
probbit live control pip.strand resume --by human:owner --reason "checked"
probbit live control pip.strand retire --by human:owner --reason "end of the trial"
```

Each appends one control line under the writer lock, canonical JSON chained like the others:
`{"at":"2026-10-08T22:00:00Z","by":"human:owner","control":"pause","prev":"sha256:...","reason":"..."}` (`at` is the time it was
written, RFC 3339 UTC; `--at TEXT` sets it). `--by` is `human[:id]` or `env[:id]`, never the individual; the reason is 1-500
characters. Pause moves an active individual to paused, resume a paused one back, retire either to retired; nothing follows
retire. While paused or retired every event is refused: one `{"error"}` object with code `paused` or `retired` per event,
nothing written, exit 4 at the end of the run. **No credit crosses a control line**: the credit of the stance before it is
cleared (section 2.8), so feedback after a resume credits no stance from before the pause, and learning cannot tie a reward to
an interruption. With a credit block (section 2.8) a control line also closes every open decision: the next stance lists them
as closed `control`, and a payment that names one is refused (B-R2), so no outcome is paid across an interruption either.
Nothing else moves: moods, drives, learned weights, history and the turn count are the individual's as before,
and the clock goes on (with `--clock fixed` the host's hours of the next event include the pause). The individual has no input
that names a control line or unsets one: an event's `control` key is an undeclared input like any other, and the stance never
mentions the status. Control lines are not an MCP tool: keep `live control` out of the agent's own reach.

A run continues a strand that ends in control lines from `--state`, the state after the last event line (the one a run
writes), and applies them: the status, and the cleared credit. `verify` replays control lines (a control line must be a valid
move; a line after retire diverges) and adds `"controls": N, "status": "active" | "paused" | "retired"` to its summary when a
strand has any. A control line is free text where it says why, so a changed reason shows at the next line's `prev`, as a
removed line does. Measured on 50 individuals x 1,000 random events with 147 pause / resume pairs inserted at random (property
P5): every run equals, stance for stance and state for state, the same events without control lines whose credit is cleared at
the same points; 4 of the 50 end with other learned weights than the run without the pairs (the cleared credit's effect, at
most 1.2 on one level). After retire, 10,000 random events are all refused and the strand and state do not change (P10).

**Checkpoints.** Every K-th event (`--checkpoint-every K`, default 1,000; 0 = none) `live` appends a checkpoint line after the
event line: `{"checkpoint":n,"prev":"sha256:...","stance":{...},"state":{...}}`, the event count, that event's stance document
and the whole state after it, chained like the others. A full `verify` replays from the header and checks every checkpoint
against the replay (its `"checkpoints": N` is in the summary); `verify --from-checkpoint` reads the header, checks the last
checkpoint against the event line before it (prev, the stance digest, the state digest, the state read as any state is) and
replays only what follows (`"from_checkpoint": n`); the lines before it are trusted, so run a full `verify` to check them. Every
number a checkpoint line carries is rounded (6 decimals; genes 4), so the line is the same on every platform's libm; for the
stance's `deviation` this holds from engine 0.14.0 on (section 2.17).

**Checkpoints with the expectation's memory (engine 0.14.0 on).** A persona with an `expectation` block (RFC 0009) folds every
event into a memory: per input combo its decayed count and the inputs of its latest event, whether a non-time combo was seen,
and the learned normal of the deviation. From 0.14.0 the fold is canonical: after every event each count, the mean and the
variance are at 6 decimals (a count at 0 is dropped), and at most `max_combos - 1` named combos are kept: when one more enters,
the last in the mixture's order (count descending, then key) moves its count to `rest`, which decays like a count and is added to
the empty member's weight, where the mixture already put every combo past the kept ones (the one change: a combo that comes
back after it was moved out starts at count 1). The checkpoint line then carries the memory as one more key:
`"expect": {"combos": [[count, inputs], ...], "mean", "n", "rest", "var", "warm"}` (combos in key order; the key is computed from
the inputs). A full `verify` checks it byte for byte against the replay's; `verify --from-checkpoint`, `resume` and `monitor`
start at the last checkpoint and fold only the events after it. The line's size is O(max_combos): the `expect` object is at
most about 64 + (max_combos - 1) x (I + 24) bytes for events whose inputs are at most I bytes. A strand whose header names an
engine before 0.14.0 keeps the four-key checkpoint and the fold before it, byte for byte; such a strand with the block still
replays from its header. An engine before 0.14.0 reads `expect` as `unsupported by this engine: expect` (exit 5).

**Resume from a record.** A host that keeps the individual's state in its own record (a state log beside the strand) can
reboot between a strand line and its record, so the record is one or more lines behind the strand. `strand::resume` refuses a
state that is not the strand's last; `strand::resume_from_record(text, &p, state, clock, &engine)` takes any state the strand
logs (an event line's `state`, a checkpoint's, the header's) and brings it to the strand's last line. What it promises: the
individual it returns is the one a header replay reaches at the last line (state, chain head, the expectation's memory, the
status after control lines), byte for byte and digest for digest, and the next event writes the line the unbroken life writes.
How: the whole strand is chain-checked as by `resume`; the walk starts at the last checkpoint at or before the record's line
(the header without one, and for a persona with an expectation block before 0.14.0, whose checkpoints do not carry its
memory); every event line after the record's line is applied: its turn run on the logged inputs (stamped as the live writer
stamped them), its stance and state digests compared, the line written again from the turn's values and compared byte for
byte. An event line logs its stance's digest, not the stance, and the next state is decoded from the solve (the levels and
odds the moods' inertia and the credit read), so an applied line costs its turn's main solve; it does not run the
expectation's member solves: the line's logged `deviation` (and the `glyph` that reads it) is taken as written and folded into
the memory as `resume` folds it. Rest lines are recomputed against the logged ones, control lines applied as the live writer
applies them (the credit zeroed, `erase` returning the learned weights to zero). Refused, with the line: a record that is no
state the strand logs (`state`), a line whose application differs (the stance, the state, the rest line or the line's bytes:
a forged line, re-chained or not), a chain that does not hold, another persona. What it does not check: the logged deviation
and glyph of the applied lines (only `verify` recomputes the expectation); the lines before the walk's start beyond the chain
check `resume` makes.

`strand::resume_boxed_from(header, Some(checkpoint), line, trailing, &p, state, clock, &engine)` is the same walk without the
whole text (docs/embedding.md "Resume on a body"): the header line, the last checkpoint line (taken on the link the host kept,
like a segment's sealed line) and the lines after it one at a time. The record may be any state logged from that checkpoint on;
an older one is refused (`state`: resume from the whole strand).

**Reflection lines (engine 0.14.0 on, RFC 0013).** `probbit live control STRAND reflect [--last N | --since-checkpoint]`
appends the strand's self-report (section 5.8, "Self-report") as one line after its last line, under the writer lock:
`{"prev": ..., "reflection": <the document>}`. A host decides when; a body calls `reflect::reflect_window` and `reflect::line`
and appends the line itself. `verify` recomputes the document from the lines before it over the window it names and refuses one
that differs (exit 1, the line named: "the reflection differs from the lines before it"); replay, the chain check and resume
pass over the line with no state change, and `live` continues a strand that ends on one (a checkpoint or a seal still follows
an event line). The writer refuses a strand of an engine before 0.14.0; an engine before 0.14.0 reads the line as
`unsupported by this engine: reflection` (exit 5). No line is written unless asked.

**Segments.** `probbit live ... --strand NAME.strand --segment-every N` seals the strand after every N-th event and goes on in
a new file: the sealing line is a checkpoint line (the last line of the segment); the next file, `NAME.0001.strand`, then
`NAME.0002.strand` and so on (four digits at least), starts with a header that has one more key at its end,
`"segment": {"index": k, "previous": "<the previous file's name>", "sealed": "sha256:<the sealing line's>"}`, its state the
sealed state, and its second line is the sealing line itself, byte for byte. The chain goes on across the files unchanged: the
lines of all segments, each segment's header and repeated sealing line set aside, are the strand the same life writes to one
file with a checkpoint at each seal. The labels sidecar is the first file's (`NAME.labels.json`); a run on `--strand
NAME.strand` continues the newest segment that exists (a sealed file is never appended to). Before 0.14.0 a persona with an
expectation block is not segmented (its checkpoint does not carry the memory). An engine before 0.14.0 cannot read a segment
file after the first: its second line chains into the previous file, so such an engine reports a divergence at line 2 (exit
1) instead of the unknown `segment` key.

A host that decides when to seal runs `probbit live control NAME.strand seal` (under the writer lock, like any control): it
replays the newest segment (from its last checkpoint), refuses unless that file's last line is an event line (a seal follows
an event: a seal right after a seal or a checkpoint exits 2 and writes nothing), appends the sealing checkpoint, writes the
next segment file (its header, then the sealing line) and prints `{"ok": true, "control": "seal", "segment": k, "file":
"<the new file>", "line", "head"}`. It writes no control line, so it takes no `--by` or `--reason`.

`probbit monitor --dir DIR --serve` lists a life's files (`NAME.strand`, then `NAME.0001.strand`, ... without a gap) as one
strand, id `NAME`, with the life's event count, `"segment": k` (the newest file's index) and the base name's sidecar; its page
follows the newest file and goes on in the next one within the 1 s scan after a seal (the new file opens at its sealing line,
the event already shown). A segment file given on its own still opens at its sealing line.

- `live verify SEGMENT` checks one segment on its link: the header is as written, the second line's sha256 is the header's
  `sealed`, that line is a valid checkpoint (its state is the header's, its memory reads), and every line after it replays.
  Exit 0 says: this file continues, faithfully, whatever line has that sha256. The summary adds `"segment": k` and
  `"trusted_link": "sha256:..."`.
- `live verify SEGMENT --chain` also walks back through `previous` to the first file: each one verifies, its last line's
  sha256 is the `sealed` the next one links and its index is one less. Exit 1 at the first break, with `"segment_file"`
  naming the file; exit 6 when a previous file does not exist (`{"ok": false, "missing": "<file>", "segment_file": "<the file
  that links it>"}`): a segment rotated away is a known state, not a divergence. The summary adds `"segments": N`.

Compaction (replacing old segments by one synthetic start) is not part of the format: it would drop the history a verify
checks, and the decision to trust a dropped history belongs to the host.

**Senses in live** (section 2.11). An event carries `senses` as `persona turn` does; the strand logs each reading as used, with
its source, and `verify` recomputes the normals from the logged readings: its summary adds `"normals": {"<id>": {"mean", "n",
"max_move_50"}}`, the normal now and the largest move of its mean within 50 events (the monitor alerts above drift_cap / 2). A
persona without the block lists `senses` in `ignored`, as before, and `live` says so once on stderr, at the earliest event
that carries it: `probbit: live: events[i]: this persona has no senses block: ...`. The documents and the strand are unchanged.

**Surprise, pace and dispositions in live** (sections 2.13 to 2.15). `verify` recomputes the average, each gate's v, r, g
and counts and the genes from the logged feedback, wins and stamped hours: its summary adds `"pace": {"rbar", "max"}` (the
average now and the largest after any event replayed) and `"surprise": {"<trait>": {"g_max_seen"}}` (the largest gain after
any event replayed). A life split across runs equals one run, byte for byte: all three live in the state.

**Needs in live** (section 2.12). An event carries `needs` as `persona turn` does; the strand logs each amount as given, with its
source, and `verify` recomputes the deficits, the success rates and the windows from the logged amounts, readings and stamped
hours: its summary adds `"needs": {"<id>": {"deficit", "at_cap_hours"}}`, the deficit now and the stamped hours of the events
that found it at its cap (it sat there since the event before; the monitor alerts while it does). A persona without the block
lists `needs` in `ignored`, as before, and `live` says so once on stderr, at the earliest event that carries it: `probbit: live:
events[i]: this persona has no needs block: ...`. Nothing runs between events, so a later rest line (RFC 0002 part 2) is its own
strand line, replayed as a control line is.

**Credit in live** (section 2.8). Events carry `await`, `for` and `cancel` as `persona turn` does, and the strand logs them as
given; `verify` recomputes the open decisions with everything else and adds `"open": [turns]` (the decisions still awaiting
an outcome) to its summary. `live verify STRAND --actions FILE` also reads the host's action log, one `{"turn": T, "action":
{"<trait>": "<level>"}}` per line, and compares each action with the level its turn's stance released: the summary adds
`"actions": {"checked": N, "differs": null}`, or the earliest turn (then line) where the host acted otherwise, as
`{"turn", "line", "trait", "action", "released", "why"}` (another level, a trait the stance did not release, or no such turn),
and exits 1. A trait the persona does not have is an input error (exit 2); `--actions` replays from the header, so it does not
combine with `--from-checkpoint`. A persona without a credit block lists `await`, `for` and `cancel` in `ignored`, as before,
and `live` says so once on stderr: `probbit: live: events[i]: this persona has no credit block: ...`.
`monitor` starts at the last checkpoint and draws its event at once. A strand that ends in a checkpoint line is continued from
the state it carries. Before continuation or a control append, the stored chain and lifecycle are checked from the header:
a checkpoint cannot hide a retired status, an invalid transition or a broken preceding hash link. This structural scan
costs time proportional to the existing log; it does not recompute earlier inference. Full `verify` is still necessary to
check that inference. Hashes alone do not authenticate a writer or detect a wholly rewritten log without a trusted external
anchor. Historical measurement before this hardening, on an Apple M4: a 10,000-event strand of a drives persona (4 goals, a floor, learning; 3,126,444
bytes, 10 checkpoint lines) opens in `monitor --once` from its last checkpoint in 0.005 s (load 3.3), where a full `verify`
takes 192 s. A replay from a checkpoint costs what the events after it cost: 500 events past the last one took 26 s to draw
(load 6-13, about 50 ms an event for this persona), so K bounds the wait; a smaller K costs a few KB per checkpoint line. Before engine 0.14.0 a persona with an `expectation` block (RFC 0009)
is the exception: its expectation memory is a fold of every event line, and no checkpoint line carries it, so `monitor`
(`--once`, `--follow`, `--serve`, each strand of `--dir` by its own persona) replays such a strand from its header, every
line, and `verify --from-checkpoint` still refuses it. The cost is a full replay when the strand is opened: on a 16-vCPU Linux VM, a 5,000-event life of the tutor with the block (10 checkpoint lines) opens in `monitor --once` in 7.4 to 7.8 s, about 650 events a second, where the same life without the block opens from its last checkpoint in under 0.05 s; after
that, appended lines replay one by one from the state in memory, as for any strand. A strand without checkpoint or control lines (shorter than K events, or `--checkpoint-every 0`) is the
same format as in 0.8.0. New strand headers record the current engine version, so their chain hashes differ; existing
0.8.0 strands remain replayable. Binaries before 0.8.0 do not read checkpoint or control lines.

Exit codes: `live` 0 when every event got its stance; 2 for a bad persona, state or flag, and after a bad event (each bad event
prints one `{"error"}` object on stdout, changes nothing and the run goes on); 4 when the writer lock is held by another writer
(nothing read or written) and after events refused by a paused or retired individual. `verify` 0 when every line replays, 1 at
a line that differs, 5 at a line with a field or line kind this engine does not know (unsupported, above: not verified, and not
a divergence), 2 for a bad flag or an unreadable file. `control` 0 when the line is appended, 2 for a bad field or an invalid
move, 4 when the lock is held or the individual is retired.

### 5.8 Monitor: watch an individual's inner state

`probbit monitor STRAND` draws an individual's declared state as live terminal bars, or as a compact browser Activity Monitor.
It replays the strand with the rules of `live verify` (the two share one replay), recomputing every stance document from the
strand alone: it needs no other log, works for any strand on any machine, and says "replay verified" while every line checks,
or names the line that differs.

```sh
probbit monitor pip.strand --follow              # bars in the terminal, redrawn as the strand grows
probbit monitor pip.strand --open                # a channel table and inspector at http://127.0.0.1:PORT/, in the browser
probbit monitor --demo --open                    # the tutor's week, paced, for someone without a strand yet
probbit monitor --demo drives --open             # eight synthetic events with goals and drives
```

**What it shows.** The latest event (or a selected historical event in the browser). In the terminal, a header: persona and version, seed, the individual's digest, the event number, the hours
since the event before, the replay badge; with a glyph (section 2.18) the glyph after the persona's name and, on the line
under it, a GLYPH row with `glyph_why` (in plain ASCII the row keeps the rule and drops the glyph). The stance: one row per trait, its levels as segments sized by their odds, the level
taken in colour and the phrase it says. The moods, with a sparkline of the last 50 events. The inputs: the event's inputs as the
turn used them (flags lit, numbers as bars, levels by name, the clock) and the history features against their caps. The senses,
with a senses block (section 2.11): one row per sense, its normal as a tick over the range, this event's reading, z and n (a
sense not read keeps its normal), and an alert when a normal moved over drift_cap / 2 within 50 events. The habits
in force, the ones that bound in colour, and the violations counter (0 by construction, section 2.5). The learned deltas per
level as centred bars within ±total_cap (with a learning block, section 2.8), and with a credit block a CREDIT row: the open
decisions with the eligibility a payment at the next event would get, this event's payment (turn, e, sign, absorbed) and the
decisions it closed. With a needs block a NEEDS row per need: its deficit as a bar over [0, cap], its term and its goal's
window (hours left or closed), and an alert while a deficit sits at its cap. With a pace block a PACE row: the average as a dot
over [0, cap] with an r0 tick, and the level; with a surprise block a GAIN row per learned trait: its gain g (1 when closed). The drives, when a document carries `pursue` or
`drives`: documents without them draw no row, so a strand of a later engine lights them up. The stance line, and why.

**The panel.** Under the rows, a PANEL block (and in the browser an Instrument panel of dials) gives every field one line of
fixed words: `<label>: <level>, <size>, <direction>`, with the number its dial shows beside the words, never in them. The fields:
each trait (and `pursue`), each mood, each need, each surprise gate, pace, each disposition gene, each learned trait, credit
open, habits in force, each sense, and room left in the learning bound. Size words come from buckets (`slightly`, `clearly`,
`strongly`: the chosen level's odds under 0.5, under 0.8, from 0.8; other fields thirds of a magnitude over its cap); direction
words compare with the event before (`leaning toward <level>` for a trait whose odds moved by 0.02 of the level range, `rising` /
`falling` for moods and numbers, `holding` under the step, `first event` with no event before; `fixed` for genes; `above` /
`below` / `at its normal` for a sense read). A block the persona does not declare reads `not declared in this persona`; room
left in the bound reads `not computed (E1 pending)` until the engine computes the bound (RFC 0004). The map is one pure
function (`probbit-cli/src/monitor/panel.rs`): a field's words change exactly when its level, size bucket or direction bucket
changes, every bucket of every field has its own words (a test enumerates them), and the words describe arithmetic over the
replayed document and state: nothing predictive, no claim of an inner life. The page's frames carry the lines as `words`
(`key`, `label`, `level`, `size`, `dir`, `text`, `dial` in [0, 1] or null, `value`); the panel reads nothing the stance
document and state do not already hold, and changes no document.

**Checkpoints and control lines.** A strand with checkpoint lines (section 5.7) is replayed from its last checkpoint, not from
the header (`--once`, and the first read of `--follow` and `--serve`): the board starts at the checkpoint's event (its stance
is in the line) and the badge says "replay verified from checkpoint n". A paused or retired individual says so next to the
badge.

**Terminal.** `--once` prints one frame and exits (the default without `--follow`). `--follow` polls the file every 100 ms and
draws appended lines within a second (a line counts once its newline is written; a truncated, removed or rotated strand is
replayed from the start, with a warning); `--fps N` caps the redraws. While a long strand replays from its header (10,000
events: about 3.5 s on an Apple M4), the board is drawn every 250 ms with a note, at a terminal and on the page. `--plain`
draws plain ASCII; colour follows the rules of the other commands (`NO_COLOR`, `PROBBIT_THEME=plain`, `TERM=dumb`).

**Browser.** `--serve` follows the strand and binds 127.0.0.1 (std `TcpListener`; port 0, a free one, unless `--port N`) and no
other address: there is no host flag. The URL is the line it prints on stdout; `--open` serves the same and starts the default
browser (`BROWSER` if set, else `open`, `cmd /c start` or `xdg-open`), best effort. It answers GET requests addressed to
`127.0.0.1:PORT` or `localhost:PORT`, and refuses others (403), so a page of another site whose name resolves to 127.0.0.1
cannot read the documents:

| route | answer |
|---|---|
| `/` | the page: one file embedded in the binary (under 72 KB, styles and script inline; it fetches no fonts, styles or scripts), with a Content-Security-Policy that lets it connect to its own server and nowhere else |
| `/events` | server-sent events: `meta` (the layout: the strand, the engine, the persona, its traits, moods, inputs and history features in the persona's order, and `face`, the two moods the face's meters show), the latest `frame`, then a frame per event: `{n, doc, given, learned, spark, diverges, note}` (with `engine.trajectory` also `trajectory` and `settle`, below), that is the stance document, the inputs as the strand logs them, the learned deltas, the moods' recent positions, the line that differs and a warning; a heartbeat comment after 15 quiet seconds; events that land within one poll (100 ms), or while a page falls behind, come as the latest frame |
| `/doc/N` | event N's stance document, canonical JSON and a newline, as `probbit live` printed it (the sha256 of the JSON without that newline is the `stance` digest of the strand's line N + 1); the latest 10,000 are kept |
| `/strands` | the strands served, canonical JSON: `{demo, dir, strands: [{id, strand, labels, persona, n, glyph, age_s, labels_error?, segment?}]}` (the file name, never its directory; `age_s`: seconds since the strand file was last written, a life in segments: its newest file; `segment`: that file's index, when it is not the first); the demo lists none |
| `/s/ID/events`, `/s/ID/doc/N` | as `/events` and `/doc/N`, for strand ID; `/events` and `/doc/N` are the first strand's, so a single-strand page is as before |

**Who and where: labels, many strands.** A strand says which persona and engine; it does not say which agent, on which
harness, in which session. Presentation labels do, string to string: the keys `agent`, `host`, `lane` and `session` (a short,
non-secret label, never a raw session key or a chat id), and free keys; at most 8 keys, a key of 1 to 64 of `A-Z a-z 0-9 _ . -`,
a value of 1 to 64 printable characters (no newline or other control character). They live in a sidecar next to the strand,
`NAME.labels.json` for `NAME.strand` (one flat JSON object, keys sorted, a trailing newline), never in the strand: the header is
the chain's first link and an engine refuses a header key it does not know (probbit 0.12.1 says `unsupported by this engine:
labels`), so a label in the header would move every `prev` and stop older engines from verifying the strand. The sidecar moves
nothing: the strand's bytes, its chain, the persona and state digests are those of the same life without labels, and `live
verify` never reads it. `probbit live PERSONA --strand F --label agent=Nova --label host="OpenClaw 2026.9.7" --label
lane=support` merges the keys into F's sidecar (keys not named stay); a host can also write the file itself (docs/agents.md,
"How a host identifies itself"). `probbit monitor A.strand --label agent=Nova B.strand --serve` serves several strands, each
`--label` applying to the path before it, over its sidecar and for the page only (nothing is written), so a life recorded
before labels gets an identity without being rewritten; `probbit monitor --dir DIR --serve` serves every `*.strand` in DIR and
picks up new ones within a second. A strand's id is its file name without the extension (made unique with `-2`, `-3`, ...).
Above the face, the page names the agent (its `agent` label, else the persona's name), then host · lane · session, the
persona's name and version, the engine, the strand's file name and when it was last written; the tab title is `glyph agent ·
lane — probbit` (no glyph without a display block) and the favicon is the glyph (else the probbit mark). With two or more
strands a left rail lists each with its glyph, agent, lane and last write; a click, Cmd/Ctrl+1 to 9 or the arrow keys switch,
`/?strand=ID` opens one. Switching closes the old stream and drops the view, its pending history requests and settle steps.

The browser opens a compact **Activity Monitor for one individual**, not a fleet dashboard. Its utility-style toolbar,
channel table and inspector use system fonts and an automatic light/dark appearance (also selectable in the toolbar).
The desktop list scrolls inside the utility; at narrow widths the inspector sits below it. This is a browser-rendered CLI
surface, not a native Swift, WinUI or Electron app. For a native terminal snapshot, use `probbit monitor STRAND --once`;
`--plain` adds a portable text-only form. No separate application installation is needed.

**The face.** The top bar is the face: the stance document's `glyph` (section 2.18) at about 2.5 times the body size, labelled
with `glyph_why` in words so the emoji is never the only carrier, then the persona's name, version, seed, the event number and
the receiver age of the last event; a strand without a glyph draws the same bar without the glyph slot. Beside it, two mood
meters: the moods `display.glyphs` names (else the persona's first two; `meta.face`), each with every level's odds as
proportional segments, the level taken at full strength, and the moods' recent positions (`spark`) as a sparkline. A changed
glyph scales and fades for 0.4 s; a changed mood level highlights its meter for 0.6 s; under `prefers-reduced-motion` neither
moves and the state still updates. **Why this glyph** opens a panel under the bar (closed by default; the choice is kept in
the browser's `localStorage`): the glyph's rule (`glyph_why`), the inputs this event carried (`given`; a stored document shows
the inputs as the turn used them), the habits in force with the bound ones and the one that gave the glyph marked, the
surprise reading (`deviation.z`, registers yes or no) when the stance has one, and the stance line. With `engine.trajectory`
(section 2.17) a settle strip draws, once per new event, the trait odds of the event's exact trajectory step by step from
the hottest inverse temperature to beta = 1 within 600 ms ("forming", then "settled"; under reduced motion only the settled
step). Its frame field `settle` is the frame's `trajectory` by level name: `{betas, h, vars: [{id, kind, levels, odds}]}` with
`odds[k]` the odds at step k, rounded to 6 places. Every number on the face is a field of the frame: the page draws the
engine's own numbers and computes none of its own; the trajectory is the latest received event's and is not drawn for a
stored document.

**Channels.** Every declared trait and mood gets a row; when present, `pursue` and each goal's wanting, afterglow and
prediction error become drive rows. All/Traits/Moods/Drives filters, a text filter, sortable columns and keyboard selection
work for arbitrary persona names and channel declarations. A persona-confidence hold is marked **held**, with pre-hold odds in the inspector; an engine-unreleased trait is separately marked **unreleased**. Missing
values are unreported, not zero. A channel's inspector shows its actual selected level/value, each reported probability,
units, declared range, source field, optional supplied phrase and the engine's event-level `why`. That `why` is not a
per-channel causal attribution or a model's thoughts.

**Reading the graphs.** The table trace for a discrete channel follows the *currently selected level's probability* through
past documents; the inspector plots each declared level's probability separately. No semantic ordering is assumed for the
level names. Drive traces show the reported numeric values in engine units (wanting/afterglow) or reward error (prediction
error), against the declared cap (including `expectation.pe_cap` for prediction error). `Δ event` compares the same quantity to the immediately preceding
event, when its document is available; probabilities use percentage points. Unknown history is a gap, not a connecting line or fabricated zero.
Both chart axes and timeline refer to **event numbers**, not wall-clock timestamps. Quiet connections never create state,
mood decay or graph samples; SSE heartbeat comments are not events.

**Inspecting history.** The timeline loads at most the latest 50 documents from `/doc/N`, with at most four concurrent
requests. Equal-sized marks represent recorded events, not workload. Select a mark, use previous/next, or use the
keyboard-accessible scrubber to hold an event. **Hold view** freezes only the browser's inspected event; the individual
and stream continue. **Follow latest (+N)** returns to the newest event. If a held event ages out of that 50-document
window, its displayed state is retained, the out-of-window condition is named, and the scrubber is hidden until a recent
event is selected. A checkpoint bounds available history. Demo restarts, changed metadata or observed strand resets clear
the cache; metadata arriving without a following frame immediately clears the prior individual's visible state.

`/doc/N` contains a stance document, not a complete monitor frame. Historical documents therefore have no invented raw
clock inputs or learned deltas. These appear in the expandable event-evidence area only when that event's full frame was
actually received. Document inputs, history features and rules remain available. The JSON link follows the inspected event.

**Status is separate from the graph.** Live/awaiting-event, synthetic replay, held view, paused/frozen strand, permanently
retired strand, replay divergence and disconnected/retrying are distinct. After 60 seconds without a newly numbered event
received in this browser, an otherwise connected active strand is labelled **stale event**. This is *receiver age*, not an
engine timestamp and not evidence that the host or agent is idle. Disconnects retain the last received state without claiming
live activity. The replay badge describes the current stream and its checkpoint scope, even while an older event is held.

**Scope.** One endpoint has one individual. No agent fleet, thoughts, CPU, GPU, memory, token use or task activity is inferred.
To add multiple agents, a host adapter would need an explicit registry of endpoint/individual identities and independently
measured lifecycle and work telemetry, each with its own timestamp/source. Those are not supplied by the current strand or
monitor API. A future fleet surface must keep per-individual replay/history and source attribution separate; a list of
channels is not a list of agents.

The page works from 360 px wide, respects `prefers-reduced-motion`, and loads no external assets or dependencies.
No strand is changed, and nothing leaves the machine. Real Chrome/Chromium regressions (Node 22+, no npm dependencies):
`cargo build --release -p probbit-cli`, then `node probbit-cli/tests/monitor_browser.mjs`. Only authored synthetic fixtures are
used, visibly labelled in screenshots. Generated evidence defaults to a temporary directory, not the repository. The checks
cover sorting/filtering/keyboard selection, arbitrary and traits-only personas, unreleased levels, append/hold/backfill,
missing history gaps, checkpoint bounds, 50 documents/four fetches, stale/frozen/retired/disconnected states and metadata reset,
the face (the glyph and its label, the meters, a strand without a glyph, the why-panel opening, closing and being remembered,
the settle strip) and no console errors.

**Demo.** `probbit monitor --demo` replays the week of `probbit live examples/persona/tutor.yaml --seed 2 --demo week` (section
5.7; the week is written to a temporary file, read back and removed), paced 1 s per hour and each night in 2 s, under a minute:
at a terminal, or with `--serve`, over and over. `--demo --once`, or a pipe, prints its last frame.
`--demo week` explicitly selects the same tutor demo; `--demo drives` selects the embedded Scout persona's eight-event
drive demo. Both are labelled synthetic, with a loop number and restart/completion cues. The default tutor events and
canonical replay documents are unchanged. Demo bars are scripted engine state, not measurements of an attached agent.

Exit codes: 0 when every line replays, 1 at a line that differs (the frame names it and shows the event before it), 2 for a bad
flag, a port that cannot be had, or a file that cannot be read or is not a strand.

#### Self-report (RFC 0013)

`probbit reflect STRAND [--last N | --since-checkpoint] [--json | --text]` prints a canonical account of a window of the
strand's events, computed from its lines alone: no turn is run, nothing is solved, nothing is random. Per event line it reads
what the line logs (the inputs, the expectation's receipt, the glyph): the inputs set and how often, and with an expectation
block each receipt's z, recomputed from the logged receipts (the expectation's mean and variance folded as the engine folds
them), the events that registered and the K = 5 largest |z| with what the expectation predicted (`pred`) and what the event
measured (`self`). Per checkpoint line in the window it reads the stance and state the line carries: per trait the levels taken
and the odds at the first and last sample, the moods' path (each mood's position along its levels, the monitor's sparkline
value) with its turning points, the drives' peaks, the habits that bound, the learned weights' net change. Feedback that settled
the credit, control, rest and reflection lines are counted. A window without a checkpoint line reports no levels, moods,
drives or habits: those sections are `null` (an event line logs digests, not the stance), never invented. Every number is at
6 decimals, `digest` is the sha256 of the canonical text without it, and `summary` holds 6 to 12 sentences (13 at most with a declared stir, RFC 0016) from fixed
templates, part of the document. `--text` prints the sentences only. The library call is `probbit_persona::reflect::{reflect,
reflect_since_checkpoint, reflect_window}` (std and `nostd`). The format is docs/rfcs/0013-self-report.md; section 5.7,
"Reflection lines", appends it to the strand.

**Mirror.** The served page has a Mirror tab: the reflection of the last 50 events computed by the server from the strand file
at each request (`/reflection?last=N`, `/s/ID/reflection` per strand), and the latest reflection line the strand carries with
whether it recomputes equal; the tab shows the sentences, the top surprises with predicted vs measured, the habits that bound
and the moods at the checkpoint samples, refreshed with the page's scan of the strand list.

### 5.9 Integrity: the drift from the declared self (RFC 0014)

With the self keys (section 2.19), every live stance document and every event line carries `integrity`, computed after the
turn from the state it produced (docs/rfcs/0014-integrity.md):

```json
{"allowance":0.902968,"crossed":[],"cue":null,"drift":0.214,"habits_held":{"held":[],"not_fired":["after_error_check","no_jokes_on_loss"]},
 "traits":{"humour":{"allowance":0.462081,"bound_used":0.21,"drift":0.097},"verbosity":{"allowance":0.440887,"bound_used":0.27,"drift":0.117}},
 "used":0.237,"warn":0.5}
```

- `traits.<t>.drift`: the total variation between the odds at the rest probe (section 2.16) of the individual rested (the
  channels of the hour, such as moods with a half-life, the credit awaiting feedback, the last stance and the agenda's step, at the fresh individual's
  values; everything that persists as it is) and of the fresh individual of the same seed. `drift` is the sum over the traits.
- `allowance`: epsilon at the individual's seed (null without a `bound` block or for an unlearned trait); `bound_used` =
  drift / allowance; `used` = the summed drift over the traits with an allowance over the summed allowance: the monitor's
  integrity dial. By RFC 0004's theorem `bound_used` stays at or below 1 (up to the 1e-6 rounding of amendment A7).
- `crossed`: the traits at or past `warn`; `cue`: `identity.cue` while `crossed` is not empty, else null. The host appends the
  cue to the line it hands the model; the engine does not edit `line`, which the state's remembered stance digest covers.
- `habits_held`: of the `keep` habits, those that bound at this event and those that did not fire.

Every number is at 6 decimals. Both stance digests leave `integrity` out; `live verify` recomputes it on every event line and
refuses an edited value naming the line ("the integrity differs"); `resume` takes it as written. The library call is
`probbit_persona::integrity::{compute, compute_cached, Integrity}` (std and `nostd`). In the monitor the integrity dial takes
the place of "room left in the bound" on the instrument panel, and the Mirror tab adds the per-trait drift table of the event
on view.

## 6. What a persona can NOT do

- It does not write, read or check text. Text-level rules (banned phrases, exact formats, facts) stay in the prompt or in
  a checker; the persona only chooses the stance the text is written in.
- It cannot see what the host does not tell it. Inputs are the host's labels; a wrong label gives a confidently wrong stance
  (the same limit as any rule system: hard rules are only as good as the facts they read).
- It is not a safety gate. Plain rules give identical rule safety at a fraction of the cost; keep approvals, permissions
  and destructive-action checks in plain code (and a model that reads the content).
- Its odds are the persona's own model odds, not a measured probability that the user will like the reply. They are only as
  good as the persona's weights; calibrate them on labelled turns before reading them as probabilities.
- Its drives do not want or enjoy anything: wanting, afterglow and expectation are numbers that the host's goal signals move
  and the stance reads. They see what the host labels (a wrong `win` gives a confidently wrong afterglow), their magnitudes are
  the author's scale, not measured value, and `pursue` says where the next unit of effort should go: the host decides whether
  to act on it, and a goal floor is not a safety gate.
- A sense is arithmetic, not an inner state: a normal is a mean, a variance and a count, kept by fixed arithmetic from the
  readings the host logs. A reading is the host's word (a forged `env` label passes every check), and a normal follows what it is shown:
  the drift cap bounds how far, it does not make the readings true. No test can see an inner state, and none is claimed.
- A need is arithmetic, not a motive: a deficit is a number the host's amounts, readings and stamped hours move by fixed rules,
  and its term is capped (`pursue.cap`) and weighted by the goal's success rate, so a mis-declared need is bounded and a goal
  never paid stops being pursued. An amount is the host's word, like a reading.
- It does not guarantee the model follows the line. Whether a given model writes in the stance it is given has to be
  measured per model (the stance is model-independent; the obedience is not).

## 7. Commands and surfaces

Before each model call: build the turn's inputs (the standard ones above, plus the persona's own), call `turn`, put `line`
into the prompt **after** the cached prefix (the line changes every turn; placed early it would invalidate a prompt cache),
store the new state. On `refused` or `fallback`, use the line as given (habits only) and log `escalate`.

**One outcome per stance.** With a credit block (section 2.8) a stance that governs two actions can be paid once: an outcome
addressed to it pays it, and a second one is refused whole ("turn N is neither open nor the previous stance", rule
B-R2; the event changes nothing and is not logged). Open one stance per action (one event with `await: true` per tool call)
and address each outcome to its own stance (`for: N`, or `for: {turn, stance}`).

**Host event ids and duplicate delivery.** The event schema has no event id: a key such as `id` is an undeclared input, listed
in `ignored`. The engine refuses a second payment of a decision (B-R2, above), but a duplicate observation event (a retried
hook, a redelivered callback) is a new event: it is appended to the strand and counted twice (two turns; a reading moves the
normal twice, a flag counts twice in a history streak). The host keeps an idempotency key per event, its own event id mapped to
the receipt (the stance's `turn` and the new state's `digest`, which the strand line logs), and does not resend an event it
has a receipt for.

**Restart time.** `--clock real` starts its clock with the process: a run's opening event is stamped with the time since the
start, and an event that carries its own `elapsed_hours` is refused, so downtime across a restart is not counted. A host that
needs downtime counted runs `--clock fixed` and stamps each event's `elapsed_hours` from its own clock (the hours since the
previous event, the restart included). `probbit_live_event` (MCP) and Python's `live_event` always run the fixed clock.

| command | what it does |
|---|---|
| `probbit persona init PERSONA [--seed N] [--out STATE]` | a new individual: genes from the seed, the resting stance (one engine call) |
| `probbit persona turn PERSONA --state STATE [--inputs JSON\|FILE] [--out STATE2] [--timing] [--no-inertia]` | the stance on stdout; the new state replaces STATE (written, then renamed: a killed turn never leaves half a state) or goes to `--out` |
| `probbit persona replay PERSONA [--seed N] --script JSON\|FILE [--out TRACE] [--no-inertia] [--timing]` | `init`, then every turn of a script: one stance per line; stderr: the trace's sha256 and the final state digest |
| `probbit persona explain PERSONA [--seed N] --script ... --turn K` | turn K in words: every contribution to every field, the joint odds, the habit-free twin, the line, the why |
| `probbit persona diff PERSONA [--seed A] [--other PERSONA2] [--seed2 B] --script ...` | the distance between two individuals (section 5.4) and their most different turn |
| `probbit persona lint PERSONA [--props FILE \| --never RULE] [--seeds 0-99] [--threads N]` | contradicting habits and planned levels that are not their trait's most likely one (section 5.5); with rules, prove them and fuzz the unknown ones (section 5.6); with a drives block, `floors`: the habits that can exclude a floor goal and the floors whose lift can reach 50; with a needs block, `needs`: each horizon whose forecast can pass K/2 before the deficit does and every habit that can exclude a bound goal (notes, not errors); with a pace block, `pace`: the largest field shift pace can cause, with a warning over 1; exit 1 if a contradiction is unresolved or a rule breaks, 3 if a rule is unproven (unknown, no counterexample found) |
| `probbit persona lint PERSONA --host MANIFEST [--strict]` | the dead-rule lint: `dead` lists each habit whose `when` needs an input or sense the host never supplies (that input stays at its default, a history feature of it at 0, so the condition never holds): `dead on this host`, a warning, an error with `--strict` (exit 1). A manifest is `{"host": NAME, "inputs": [ids], "senses": [ids]}`; `examples/hosts/` has two derived from real integrations |
| `probbit persona bound PERSONA [--seeds LIST] [--json] [--audit STRAND]` | RFC 0004 (section 2.16): the certificate per learned trait, read-only; `--audit` replays a recorded life and reports the room used; exit 1 on an uncounted channel, an epsilon above `max_epsilon` or an exceedance |
| `probbit persona fuzz PERSONA (--never RULE \| --props FILE) [--seeds 0-99] [--fuzz-seed N] [--scripts N] [--depth N] [--beam N] [--grid LIST] [--hours LIST] [--threads N] [--json]` | search event scripts for each individual's shortest counterexample to a character property, shrunk and replayable (section 5.6); exit 1 if one is found |
| `probbit persona prove PERSONA (--never RULE \| --props FILE) [--seeds 0-99] [--threads N] [--json]` | per rule: held by construction, proved for every event sequence, or unknown with the cell that failed (section 5.6); exit 1 if one is unknown |
| `probbit persona check PERSONA` / `describe PERSONA` | valid, its digest and sizes / its traits, moods, inputs, habits and agenda (and senses: unit, range, scale, sources, prior, caps, spreads, reserved ids; the learning block with its credit block; needs: the drive's n and m, the pursue term, the success rate, per need its unit, cap, weight, growth, sources, amount, bound goal, horizon, spreads and reserved id; the windows, valence's weights and liking; surprise's settings and a new learner's start; pace's closed form, levels, effects, gain and largest field shift; the disposition spreads and genes) |
| `probbit persona compile PERSONA --state STATE [--inputs ...]` | the turn's probbit-ir program; its sha256 is the stance's `engine.program` |
| `probbit live PERSONA [--seed N \| --state FILE] [--strand FILE] [--events FILE [--watch]] [--clock real\|fixed]` | a resident individual: JSONL events in, one stance per event out, `elapsed_hours` stamped by the clock, the life logged to a strand (section 5.7); `--state` is rewritten after every event |
| `probbit live PERSONA --demo week [--seed N] [--strand FILE] [--plain]` | one individual's scripted week on the fixed clock: learning to its cap, a quiet night, habits that hold (section 5.7) |
| `probbit live verify STRAND [--from-checkpoint] [--chain] [--actions FILE]` | replay a strand from its header alone: ok, the earliest line that differs (exit 1), or the earliest line with a field or line kind this engine does not know (`unsupported`, exit 5: a newer engine's strand, not a divergence); with `--actions`, also the earliest turn whose host action differs from the level its stance released; with `--chain`, a segment and every previous segment file (exit 6: one is missing) (section 5.7) |
| `probbit monitor STRAND [--follow] [--once] [--plain] [--fps N] [--serve] [--open] [--port N]`, `probbit monitor --demo [...]` | watch an individual's inner state: the strand replayed as live bars in the terminal, or as a page on 127.0.0.1 (section 5.8) |

A script is a JSON list of per-turn input objects (or `{"turns": [...]}`; a turn may be `{"inputs": {...}, ...}`). Exit codes
(`live`'s are in section 5.7, `monitor`'s in section 5.8):
0 done (every turn status is an answer), 1 `lint` found an unresolved contradiction, `fuzz` a counterexample or `prove` an
unknown rule, 2 a bad persona, state, input, script or rule
(ONE `{"error": {"code": "persona", "path", "message"}}` object on stdout) or a bad flag (stderr). The engine's resource controls
are `probbit run`'s (`PROBBIT_THREADS`, `PROBBIT_CPU_LIMIT`, `PROBBIT_MEM_LIMIT_MB`, `PROBBIT_PRIORITY`, the config file); they
never change a document. The persona's own `engine.chains` is used whatever `PROBBIT_CHAINS` says (it is part of the answer).

- **MCP** (`probbit mcp`): `probbit_persona_init {persona | persona_path, seed}` -> the state; `probbit_persona_turn {persona |
  persona_path, state, inputs, flags: {timing, no_inertia}}` -> `{stance, state}`; `probbit_persona_fuzz {persona | persona_path,
  never | props, seeds, fuzz_seed, scripts, depth, beam, grid, hours, threads}` -> the `fuzz --json` document (section 5.6);
  `probbit_live_event {persona | persona_path, state | seed, event, strand_path}` -> `{stance, state}`, one event of a resident
  individual (`event` is its inputs, `elapsed_hours` from the host's clock, quantised to 1e-6 h; with `strand_path` the event is
  logged to that strand on the server's disk, a new file getting the header, and the answer gets `strand: {path, events, head}`);
  `probbit_live_verify {strand | strand_path}` -> `live verify`'s document (section 5.7; a divergence is an answer, and so is
  an unsupported strand: `{ok: false, line, unsupported, engine, strand_engine}`).
  For a persona with a senses block, `probbit_persona_turn` and `probbit_live_event` refuse an event that carries readings
  (rule S7, section 2.11: readings come from the host's instruments, never through the agent's channel); a persona without
  the block lists `senses` in `ignored`, as before. For a persona with a credit block they refuse an event that carries
  `cancel` (section 2.8: a cancel comes from the executor or a person, never through the agent's channel, so the agent cannot
  cancel a decision to dodge its correction); `await` and `for` pass. For a persona with a needs block they refuse an event
  that carries `needs` amounts, from any source (section 2.12: an amount comes from the host's `satisfy_from` sources, never
  through the agent's channel, so the agent cannot pay its own needs); a persona without the block lists `needs` in `ignored`.
  A persona with a drives block takes `goals` in `inputs` / `event` (section 2.9); the schemas declare it.
  Stateless; the documents are the CLI's. A refusal, a fallback, a counterexample, a divergent strand or an unsupported one is
  an answer; a bad persona, state, input, event or rule is a tool error carrying the error object.
- **Python** (python/probbit.py, standard library only): `probbit.persona_init(persona, seed=None)` -> the state;
  `probbit.persona_turn(persona, state, inputs, timing=False, no_inertia=False)` -> `{"stance", "state"}`;
  `probbit.persona_replay(persona, script, seed=None)` -> the stances; `probbit.persona_fuzz(persona, never=None, props=None,
  seeds="0-99", **flags)` and `probbit.persona_prove(...)` -> the `--json` documents of section 5.6 (a counterexample or an
  unknown rule is an answer); `probbit.live_event(persona, state=None, event=None, seed=None, strand=None)` -> `{"stance",
  "state"}` (+ `"strand"` with a strand path), the MCP tool's shape; `probbit.live_verify(strand)` -> `live verify`'s document
  (a divergence, exit 1, and an unsupported strand, exit 5, are documents too, as over MCP).
  `persona` is a file path or the document as a dict; bad input raises `ProbbitInputError` (`.code == "persona"`, `.path`,
  `.message`).
- **Browser** (probbit-wasm, no threads): `probbit_call` op 4 = persona init, op 5 = persona turn, with the MCP tools' arguments
  (the persona inline). The playground's "meet three individuals from one persona" runs three seeds of one persona side by side.

## 8. A model-proposed stance (the `evaluate` bridge)

When a judge model reads the conversation and proposes a stance, the persona decides: one `choice` question per trait (the
criteria are the levels' `say` phrases), the judge's probabilities combined with the persona's own unaries
(`logw = ln max(p_judge, 1e-6) + h(v,l)`), and the habits in force as the request's rules block. `evaluate` returns the most
likely stance that obeys every habit, the odds under the rules, and per trait whether the persona moved the judge's choice. With
the judge's weight at 0 the request is the turn's own program (its unaries as `probbit.logw`, its pairs and habits as
`probbit.rules`) and `probbit evaluate` gives the stance `probbit persona turn` gives, level and odds within 2e-6, on every example
turn (tests/persona.rs).

## 9. Versioning

`probbit_persona: 1` is this document. Additive fields keep the number; a change of meaning of an existing field increments
it. The stance document (`probbit_persona_turn`), the state (`probbit_persona_state`) and the strand header (`probbit_strand`)
carry their own format numbers. The learning block (section 2.8) is additive: `probbit_persona` stays 1, and a persona without it
gives the documents it gave before the block existed.
