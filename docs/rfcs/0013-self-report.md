# RFC 0013: self-report, a canonical account of the last N events from the strand alone

Status: **implemented, off by default** (since 0.14.0: `probbit reflect`, the library's `reflect` module, the engine-gated
`reflection` line, the monitor's Mirror tab) · Changes: docs/persona.md §5.7 (one more line kind, 0.14.0+) and §5.8
("Self-report", "Mirror"), `probbit live control`, `live verify`, `probbit monitor` · Depends on: RFC 0009 (the expectation
block and its receipts)

Mechanism and simulation only. The self layer is a declared, exact, replayable account an individual gives of its own recent
events: arithmetic over the lines of its strand, recomputable by anyone who holds the strand. Nothing here is run, solved or
sampled; nothing is written unless a host asks.

## What it is

`reflect(lines, window)` reads a strand's lines and returns one canonical document, `reflection`, over a window of its event
lines: the last N (`--last N`), the events after the last checkpoint line (`--since-checkpoint`, that checkpoint the first
sample), or every event. It reads what each line logs and nothing else:

| Section | Read from | What |
| --- | --- | --- |
| `window` | every line | first and last event n, the events, the lines read, the checkpoint samples, the strand's engine, the window asked for |
| `inputs` | event lines | per input, the events where it was true or a non-zero number (history and sensed readings are not counted) |
| `surprise` | event lines (`deviation` receipts) | per receipt z recomputed from the expectation's memory, a fold of the logged receipts (its mean and variance only, as `persona::expect_step` folds them); the events scored, the ones that registered (z ≥ 1.5), the K = 5 largest \|z\| with `pred` (what the expectation predicted), `self` (what the event measured), `input`, the glyph and the inputs set; `null` without an expectation block |
| `traits` | checkpoint lines in the window | per trait (and `pursue`) the levels taken, the odds at the first and the last sample, the smallest and largest p of the level taken |
| `moods` | checkpoint lines | per mood its position along its levels (0 the lowest, 1 the highest: the monitor's sparkline value) at each sample: start, end, min, max, the turning points (sign changes of the first difference; all counted, the first 8 listed) and the last 50 samples |
| `drives`, `habits` | checkpoint lines | per goal the largest want and its event; per habit the samples where it bound |
| `credit` | event and control lines | feedback that settled the credit (reward, correction, both on: cancelled) and the control lines that zeroed it; `null` without a learning block |
| `learned` | checkpoint lines | the learned weights' net change between the first and the last sample, and the sum of its absolute values |
| `controls` | control, rest, reflection lines, the header | pause, resume, retire, erase; `zero_credit` (every control line zeroes the credit); rest lines; earlier reflection lines; the segment index |
| `summary` | the document's numbers | 6 to 12 sentences from fixed templates (13 at most with a declared stir, RFC 0016), part of the canonical document |

A section the window has no line for is `null`: an event line logs digests, not the stance, so a window without a checkpoint
line reports no levels, moods, drives or habits (absent, never invented). A strand of an older engine is read the same way;
the fields its lines lack are absent. Every number is at 6 decimals (the P0-b rule), the document is canonical JSON, and
`digest` is the sha256 of the canonical text without `digest`.

The z of a receipt is the expectation's own: on a 0.14.0 strand the fold starts at the last checkpoint before the window that
carries the expectation's memory (its mean and variance are the fold's, at 6 decimals), else at the header; it equals the z
the checkpoint lines' stances carry (a unit test checks every checkpoint of the fixture).

## The templates

The sentences are the document's numbers in fixed wording, so a model can voice them or a person can read them; nothing in
them is generated. They describe mechanism. Over the expectation fixture of the test suite
(`probbit-cli/tests/fixtures/monitor/tutor-expect.strand`, 22 events, a checkpoint every 5): "This account covers events 1 to
22: 22 events and 26 lines read, 4 of them checkpoint samples.", "The expectation scored 22 events and 11 of them registered as
surprising (z at least 1.500000).", "The largest surprise was event 2: z 6.710933, the expectation predicted 2.842300 and the
event measured 2.211600.", "The mood valence moved from 0.705807 to 0.465881 on a scale of 0 to 1 (min 0.255617, max 0.705807,
2 turning points)." A unit test
holds every template's output to a list of words it never contains.

## The reflection line (0.14.0+)

`probbit live control STRAND reflect [--last N | --since-checkpoint]` appends the reflection as one line after the strand's
last line, under the writer lock: `{"prev": <sha256 of the line before>, "reflection": <the document>}`, canonical, chained like
every line. A host decides when (after a checkpoint, at the end of a day); a body in standby calls `reflect::reflect_window`
and `reflect::line` and appends the line itself.

- `live verify` recomputes the document from the lines before it, over the window it names, and refuses one that differs (a
  changed number, a changed sentence, a moved window): exit 1 naming the line, "the reflection differs from the lines before
  it". Replay treats the line like a control line: no state changes, the chain goes on from it; `live` continues a strand that
  ends on one.
- Engine-gated: the writer refuses a strand whose header names an engine before 0.14.0, and a 0.14.0 replay of such a strand
  diverges at a reflection line. An engine before 0.14.0 reads the line as a key it does not know: `unsupported by this engine:
  reflection`, exit 5 (not verified, and not a divergence).
- No line is written unless asked: every strand without reflection lines, and every corpus, is byte for byte what it was.

## The Mirror tab

`probbit monitor --serve` serves `/reflection?last=N` (and `/s/ID/reflection` per strand): the reflection of the last N events
(default 50) computed from the strand file as it is at the request, and the latest reflection line the strand carries with
whether it recomputes equal. The page's Mirror tab shows the sentences, the top surprises with predicted vs measured, the
habits that bound and the moods at the checkpoint samples, refreshed with the page's scan.

## Cost

One pass over the lines from the fold's start: each event line parsed once, each checkpoint line in the window parsed once,
no solve. Host microseconds and the stack high-water on the no_std proxy are in docs/embedding.md ("Self-report in standby").
Silicon is not measured.

## Not in this RFC

Integrity (how far the individual drifted from the persona it declared), consolidation and curiosity are later RFCs of the
self layer. The reflection judges nothing: it counts and folds what the strand logs.
