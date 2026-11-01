# RFC 0015: Memory consolidation, a rest pass that compacts what was lived and logs what changed

Status: implemented (library `Live::consolidate` / `consolidate_boxed`, the `consolidation` strand line, `live control STRAND
rest`), off by default; since 0.14.0 · Changes: docs/persona.md (§2.17, an optional `rest` key of the
`expectation` block, outside the digest as the whole block is), one engine-gated line kind, one key in the expectation's checkpoint memory · Depends on:
RFC 0009 (the expectation's memory and its z), RFC 0013 (the reflection reports the pass), RFC 0016 (its fold is left as it is)

## Summary

At rest, when a host decides (standby, night, idle), the engine runs a **rest pass** over the expectation's memory: it
re-folds what the events since the last pass added with a declared half-life, weighs each remembered context by how much it
kept surprising, merges near-duplicate contexts, and applies a declared decay to the learned deltas. It writes one
`consolidation` line that logs the memory's digest before and after, the contexts merged, the weights moved and the decay.
Labelled **mechanism and simulation**: the pass is arithmetic over numbers the strand already logs; the line is
checkpoint-like and `live verify` re-runs the pass from the lines before it. No new state the strand cannot recompute.

## The declaration

```yaml
expectation:
  half_life_events: 32
  max_combos: 6
  rest:                    # optional; without it the pass is refused and nothing is folded for it
    half_life: 64          # the window re-folded with this half-life in events (default: half_life_events, a no-op)
    bucket: 2              # bins of a declared number input in the merge key, 1-3 (default 3: the waking key, nothing merges)
    surprise_gain: 1       # weight gain of a context's mean z+ (0-4; default 0: none)
    decay: 0.5             # factor on the learned deltas, 0-1 (default 1: the state is left as it is)
```

Every key is optional and every default changes nothing: `rest: {}` declares the pass, and a pass under it logs `moved 0`,
`merged 0` and leaves the state digest as it was. The key is outside the persona digest, as the whole `expectation` block
is: the pass runs only when a host calls it, and the strand's header carries the document every replay and resume reads,
so the line's `state` digest pins what a `decay` did. Two documents that differ only in `rest` share a digest.

## The window

With a `rest` block, the canonical memory (engine 0.14.0 on) carries one more key, `window`: `since` (events since the last
pass) and, per remembered context in key order, `[a, b, k, zs]`: `a` the context's visits in the window decayed by the
expectation's half-life (the part of its count the window added), `b` the same visits decayed by `rest.half_life`, `k` the
visits, `zs` the sum of the visits' z+ = r6(max(z, 0)) (z as the deviation computed it: the logged prediction against the
learned normal before the event; a cold event adds 0). Each row is at 6 decimals after every event; a context the memory
evicts or prunes leaves the window with it. A checkpoint carries the window, so a resume and `verify --from-checkpoint` start
there.

## The pass

In this order, on the memory as the last event left it:

1. **Re-fold**: each count becomes r6(max(0, count − a + b)): the window's part re-weighted with the rest half-life (a longer
   half-life keeps more of the window's early visits; the expectation's own half-life leaves the count as it is).
2. **Surprise**: with gain g > 0, each context gets the weight f = 1 + g · min(zs/k, 1.5)/1.5 (1.5 = the deviation's
   registration threshold; a context without visits in the window has f = 1), and the counts are scaled back to their
   total before (r6 each): contexts that kept surprising keep more weight, the total is unchanged.
3. **Merge**: with `bucket` B < 3, contexts whose key is the same with each declared number input in B equal bins (instead of
   the waking key's thirds) become one: the inputs of the one with the largest count (ties: key order), the counts summed.
4. **Decay**: every learned delta becomes r6(decay · Δ) (the state digest moves; 1 leaves the state as it is).

The window is then empty. The expectation's learned normal (mean, variance), its `rest` count and the curiosity fold (RFC
0016) are left as they are; a context merged away leaves the curiosity fold's failed set at the next event by the fold's own
prune. The memory holds at most `max_combos − 1` named contexts after every pass (the merge only removes, the other steps
only reweigh). The expectation's cache is emptied (its rows are keyed by a program's text, so no receipt changes).

## The line

```json
{"at":"2026-10-28T04:00:00Z","consolidation":{"after":"sha256:...","before":"sha256:...","decay":0.5,"events":50,
 "merged":{"count":1,"keys":["{...}"]},"moved":2.186238},"n":50,"prev":"sha256:...","state":"sha256:..."}
```

`at` the host's time stamp (1-64 characters, taken as written); `n` the events before the pass; `before`/`after` the sha256 of
the memory's checkpoint form; `events` the window's length; `merged` the keys merged away; `moved` the sum over every key of
|count after − count before| (6 decimals); `state` the digest after the decay. Engine-gated: a strand of an engine before
0.14.0 refuses the line (`CONSOLIDATE_OLD`), and an engine before 0.14.0 reads a persona with `expectation.rest` as unsupported
(`verify` exit 5). `live verify`, the chain check, `resume` and `resume_from_record` re-run the pass; a forged line is refused
at its line with the first field that differs (`the consolidation line differs at consolidation.moved`).

## Interactions

- RFC 0009 expect cache: invalidated by the pass; every receipt is the same with the cache on or off.
- RFC 0013 reflection: a window holding a consolidation line gets `consolidation {lines, merged, moved}` and one sentence
  ("The rest pass ran N times in this window: M contexts merged, the remembered weights moved by X in total."); every other
  reflection document keeps its bytes.
- RFC 0016 curiosity: its fold is not touched by the pass.
- RFC 0014 integrity: reads the state after the pass as any state (a decay moves the learned deltas and so the drift).

## Surfaces

- Library: `Live::consolidate(at)`; `Live::consolidate_boxed(&mut Box<Live>, at)` on a body (the same call, in place).
- CLI: `probbit live control STRAND rest [--at TIME] [--state FILE]`: the strand replayed from its header, the line appended
  under the writer lock; `--state` names the host's stored individual (`live --state FILE`), checked against the strand's
  last state and rewritten after the pass, so the next `live --state FILE` continues the strand.
- Monitor: a `REST` row with the last pass (time stamp, event, line, contexts merged, weights moved, decay, window length),
  and `last_rest` in the page's frame, once the strand holds a consolidation line.

## Cost

O(contexts) per event for the window fold (at most `max_combos` rows) and O(contexts²) per pass for the merge (at most 64
contexts); no solve. Numbers on the host and the `no_std` proxy are in docs/embedding.md; chip numbers are the body lane's.

## Not built

`rest.decay` naming the curiosity fold (a decay of failed contexts at rest) is not built: the pass leaves the drive's fold as
it is. The monitor shows the last rest (a `REST` row and the frame's `last_rest`); its history view does not mark rest lines.
