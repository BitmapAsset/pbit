# RFC 0014: Integrity, the drift from the declared self

Status: implemented (library and strand line), off by default; since 0.14.0 · Changes: docs/persona.md
(the `identity` block's self keys, additive and outside the digest), one engine-gated field on event lines · Depends on:
RFC 0004 (the allowance is its epsilon), RFC 0013 (the Mirror tab, where the per-trait table is to be shown)

## Summary

Per event, one exact number and its decomposition, from the state and the persona document alone: how far the individual's
resting odds have moved from those of the self its persona declares, and how much of the persuadability certificate's
allowance that movement uses. Labelled **mechanism**: arithmetic over declared numbers, replayable from the strand,
recomputed by `live verify`. It adds nothing to the state and changes no turn.

## The declaration

Four optional keys in the existing `identity` block (the "self keys"):

- `keep: [habit ids]`: the habits that are identity-bearing.
- `warn: <share>`: a share of the allowance in (0, 1], default 0.8.
- `cue: <text>`: a short line (1-120 characters) in the persona's own words, handed to the host when a trait reaches `warn`.
  The engine never writes cue text of its own.
- `traits: [trait ids]`: the traits measured; default the learned traits (`learning.traits`), else every trait.

Declaring any of them turns integrity on. The keys are read against the persona built without them and stay out of the
persona digest (as RFC 0004's `bound`, amendment A1): every state is byte for byte as without them. A persona that declares
none of them emits nothing, so every existing strand, stance and corpus is unchanged.

## The numbers

- **The rest probe** (RFC 0004's): one event of `bound.rest_hours` idle hours (default 30 × the longest declared half-life,
  24 without one), no inputs.
- **The rested individual**: the current state with the channels of the hour at the fresh individual's values (moods with
  `half_life_hours`, the pace average, the load, the rest pressure, the eligibility trace, the credit awaiting feedback and
  its open decisions, the levels and stance taken last, the agenda's step) and everything that persists as it is (learned
  deltas, history, moods without a half-life, drives, senses, the surprise gates, genes). The credit is of the hour: it is
  what the next feedback would act on, zeroed by every control line, and a rest probe has no feedback.
- **drift**, per trait: the total variation, half the L1 distance over the trait's levels, between the probe's odds for the
  rested individual and for the fresh individual of the same seed (the declared persona at birth); `drift` at the top is the
  sum over the traits. The fresh individual is its own rested state, so its drift is 0 exactly. Total variation is chosen over the
  plain L1 because it is the certificate's own unit: `bound_used` is then a ratio of like quantities, and the theorem of
  RFC 0004 reads directly as `bound_used ≤ 1` (up to the residue r of decayed channels and the 1e-6 rounding of amendment A7).
- **allowance**, per trait: epsilon at the individual's own seed, computed once per seed from the persona's `bound` block
  (the same vertex search as `persona bound --audit`); null for a trait that is not learned or without a `bound` block.
- **bound_used** = drift / allowance (0 without an allowance). **used** (the dial) = the summed drift over the traits with an
  allowance, divided by the summed allowance (0 without one).
- **crossed**: the traits whose `bound_used` reached `warn`; **cue**: `identity.cue` when `crossed` is not empty, else null.
- **habits_held**: of the `keep` habits, `held` (bound this event) and `not_fired`.

Every number is rounded to 6 decimals.

## Where it appears

- The live stance document gets `integrity`; the event line carries the same document as its `integrity` field. Both stance
  digests leave it out (it is computed after the turn, from the state the turn produced, so the state's remembered stance
  digest and `for` credit binding are unchanged).
- `live verify` recomputes it on every event line and refuses an edited value naming the line ("the integrity differs").
  `resume` takes it as written, as it takes `deviation` and `glyph`.
- Engine-gated: a persona with self keys is refused by an engine before 0.14.0 as an unknown field, which `live verify`
  reports as `unsupported` (exit 5), never as a divergence.

## Cost

At most one extra solve per event: the rest probe of the rested individual. The fresh individual's probe and the
allowances are computed once per seed. The probe is cached twice (RFC 0014-b):

- **By program.** The probe's turn steps run as the turn runs them, its first program is compiled and keyed by its structure
  without the `comment` (the key the expectation's cache gives its members). An equal key answers from the row. Otherwise the
  turn finishes on that same compiled program, so a miss compiles once. A row is kept only when the program's answer was exact
  and the stance is that answer's (no yield, no hold's clamped solve, no conflict): exact marginals read the program's model
  alone, so a row is the uncached computation's value bit for bit. A body whose senses, drives or needs move every event hits
  whenever its resting program did not move.
- **By state**, as before: the rested state's body without `turn` and `digest`, used while no program row is held (after a
  sampled answer). While a row is held an equal rested state would compile the same program, so the state key is not built.

Measured (docs/embedding.md "Integrity on a body"): on a body's recorded day of 1,160 events the resting program moved on 4
events (the `pursue` goal's drive terms), so the probe was answered from the cache 1,156 times (99.66 %). The cost fell from
+40.1 % to +16.1 % of a turn on the host (std and the `nostd` proxy alike): what remains on a hit is building the probe's program (the turn's steps before the
compiler, the compile, the key). A miss is a whole turn and carries its stack (13,656 B on the `nostd` build, against
12,520 B for the bare rest-probe turn); a hit by program reaches the same depth (13,656 B): the turn's steps before the solve
are the deepest part of a turn. The bar of this RFC (2 % of a turn, 8 KB per miss) is still not
met. The floor of a program key is one program build per event. The remaining lever is to skip the build when nothing the
probe's program reads has moved (a key on the compiler's inputs: the learned deltas, the credit, the history windows, the drives'
terms, the senses' gains). That key is not built here: its exactness needs the slice proved field by field, the way the
expectation's members would need it (T2's NEXT 1). A per-miss stack under 8 KB needs a rest probe lighter than a turn.

## Not in this build

- The cost bar (2 % of a turn, 8 KB per miss): the cache now hits on a body's ordinary event; what remains is one program
  build per event and a turn's stack per miss (see Cost).

- Appending the cue to the stance's `line` (the line is inside the stance digest the state remembers); the host appends
  `integrity.cue` itself.
- The monitor's integrity dial and the Mirror's per-trait table.
