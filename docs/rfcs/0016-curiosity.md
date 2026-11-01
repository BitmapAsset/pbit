# RFC 0016: Curiosity, a drive fed by the measured surprise, and the self-start it gates

Status: implemented (library, strand line, `live control STRAND stir?`), off by default; since 0.14.0 ·
Changes: docs/persona.md (§2.20, a new optional top-level block, in the digest), one engine-gated field on event lines, one
key in the expectation's checkpoint memory · Depends on: RFC 0009 (the deviation's z and the memory fold), RFC 0004 (the
bound), RFC 0013 (the Mirror's account)

## Summary

A persona may declare a drive named `curiosity`. Its level is arithmetic over two numbers the expectation block already
measures: `surprise_recent`, an exponentially weighted average of z+ = max(z, 0), and `failed_contexts`, the remembered input contexts
whose last K visits all missed. The level rises while nothing new arrives and while contexts keep failing, and it falls when
a surprising event arrives. It moves the stance through one engine-owned number input, as `pace` does. It can also gate a
self-started idle event (a **stir**): `Live::stir_due(idle_hours)` answers, exactly from the drive's state and the idle
hours, whether one is due, at a declared rate, with a quieter class and a lower rate while the individual is asleep.
Labelled **mechanism and simulation**: the name is the drive's declared name; the drive is a function of logged numbers,
replayable from the strand and recomputed by `live verify`.

## The declaration

```yaml
curiosity:                     # needs an expectation block (the surprise it reads)
  half_life_events: 8          # the average of z+ = max(z, 0): its half-life in events (>= 1)
  quiet_z: 0.5                 # surprise_recent below this is "nothing new" (> 0); the average starts here
  miss_z: 1.5                  # a visit misses when z+ > miss_z (> 0)
  last_k: 3                    # a context fails when its last K visits all missed (1-16)
  weights: {quiet: 1.0, failed: 0.5}   # 0-50 each
  cap: 2                       # the level stays in [0, cap] (> 0)
  effects: {initiative: 0.6}   # required: ordinal shift per unit of the level, on traits or moods (never pursue)
  stir:                        # optional: the self-start
    threshold: 1.0             # due only at a level >= threshold, in (0, cap]
    every_minutes: 30          # at most one stir per 30 minutes (>= 1)
    class: check               # the stir's class (an identifier; default check)
    speaks: true               # whether that class may speak (default true)
    when_asleep:               # optional; without it no stir while asleep
      flag: night              # a declared flag input: the last event had it on = asleep
      every_minutes: 120       # >= every_minutes
      class: glance            # the asleep class (default glance); it never speaks
```

The block is in the persona digest (it moves the stance). Refused when the persona is read, at the field's path: no
expectation block (C-R1); a bound outside its range, `effects` empty or on `pursue` or on an unknown id, a `when_asleep.flag`
that is not a declared flag input, an asleep rate faster than the awake one, an asleep class equal to a speaking awake class
(C-R2); a declared input named `cur_level` or `stir`, a habit condition on either (C-R3).

## The drive

Per event, from the logged line alone (so `verify`, `resume` and a restart from a checkpoint fold the same numbers):

    z     = (ln(1 + pred) - mean) / sqrt(var + 1e-4)       the deviation's z: the logged pred against the normal before the event
    z+    = r6(max(z, 0))                                  one-sided, as `registers`: an error below the normal is no surprise
    s     <- r6(lam x s + (1 - lam) x z+),  lam = 0.5^(1 / half_life_events)        (cold events: unchanged)
    miss  = z+ > miss_z, appended to the event's context (its expectation combo), the last K kept
    F     = the contexts whose last K visits all missed (contexts the expectation drops are dropped)
    level = r6(clip(quiet x max(0, 1 - s / quiet_z) + failed x F, 0, cap))

Event t's turn reads the level after event t-1 as the number input `cur_level` (box [0, cap], carrying `effects`), and so
does every member of the expectation's mixture (the drive is part of what the engine expects of itself). A host never sends
`cur_level` (an event that does is refused at `inputs.cur_level`); the stance's `inputs` leave it out. `persona turn` and
`persona replay` carry no memory: the level is 0 there.

The fold lives in the expectation's canonical memory (engine 0.14.0 on): a checkpoint line's `expect` gains
`curiosity: {asleep, ctx: [[key, [miss, ...]], ...], s, since}`, every number at 6 decimals, so a replay and a resume start at
the last checkpoint. A strand of an engine before 0.14.0 has no drive (its level is 0).

## Where it appears

- The stance: `drives.curiosity = {failed_contexts, level, surprise_recent}` (the values the turn read; on a stir also
  `stir: <class>`). Outside the stance digest, as `deviation`.
- The event line: the same object under `curiosity`; `live verify` recomputes it ("the line differs" on an edit). An engine
  before this one reports the line as unsupported (exit 5), never as a divergence.
- `probbit monitor`: the drives rows show `curiosity` with its level, its two inputs, and the stir's class on a stir.
- The Mirror account (RFC 0013): with a declared stir, the reflection document gains `stirs: {class: count}` over the window
  and one sentence (count, classes); every other reflection document is byte for byte as before.

**Why one-sided (measured).** The first build averaged |z|, as the draft named it. On a steady quiet stream the learned
normal's floor (var + 1e-4) turns a slowly falling prediction error into z of about -1 on every event, so |z| read a dark
night as surprising and the quiet term never fired (unit test: the level stayed at or below 0.064833 over 48 identical
night events). A prediction error below the learned normal is the opposite of surprise; the drive reads z+.

## At rest: integrity and the bound

The level moves per event, not with the hours, so it is a channel of the moment. RFC 0014's rested state takes the fresh
individual's level (0): integrity's drift measures what persists, not the drive. RFC 0004's certificate cannot assume the
level decays over the rest hours (it does not), so with the block its scope adds the drive's whole shift to the residual:
`d_r += 2 x sum(|effects| on the trait's component) x cap`, as `pace` adds its decayed shift. A persona without the block
gets the certificate of before.

## The stir

`Live::stir_due(idle_hours) -> Option<Stir>` (and `probbit live control STRAND stir? [--idle-hours H]`, which replays the strand
and prints `{due, events, stir}`; nothing is written):

1. None without a stir block, while paused or retired, or when the level is below `threshold`.
2. Asleep (the last event had `when_asleep.flag` on): None without `when_asleep`; else the asleep class (never speaks) at the
   asleep rate. Awake: the declared class at the declared rate.
3. The rate cap: due when no stir was fed yet, or when the hours since the last stir (folded from the stamped hours) plus
   `idle_hours` reach the class's minutes.

A host that acts on a stir feeds an ordinary event carrying `"stir": "<class>"` together with its own current readings (a
body's sensed inputs, the night flag); the turn runs as usual, the inputs on the line carry the marker (the self-started
event, source self), and the fold restarts the rate cap there. A stir the host ignores leaves no line. **Habit precedence:**
the stir is an event like any other, so a habit in force on its inputs binds its stance; a persona that sleeps on a flag
declares the same flag in `when_asleep`, so the only class offered while that habit binds is the quiet one.

## Bars (measured on a draft PR; numbers in the PR section)

- B1 exact and replayable: personas without the block unchanged byte for byte (corpora); with it, lives replay byte for byte
  on x86_64 and the no_std proxy, checkpoint resume equal, the previous engine reports the strand unsupported (exit 5).
- B2 a dark night: the sensed fixture with a quiet-at-night habit and the drive, over a synthetic night of identical sensed
  events: stirs rise from 0 to the declared asleep rate, and the habit binds on every stir.
- B3 the bound: the RFC 0004 certificate recomputed with the drive declared holds (0 exceedances).
- B4 cost: the fold and `stir_due` against a turn.

## Not in this build

A drive toward a chosen context (the level is global, not per context); a stir class that names what to look at; a body's
silicon measurement (the stack and cost numbers are from the proxy build).
