# Reproduce a number

Every number in BENCHMARKS.md regenerates from this repository. Four ways in, from five minutes to an afternoon, and one
place to report what you got. An outside reproduction (or a disagreement) is worth more to this project than a star.

## Five minutes: the engine, a rule, a recorded life

```sh
npm i -g probbit            # or: curl -fsSL https://raw.githubusercontent.com/BitmapAsset/probbit/main/install.sh | sh
git clone https://github.com/BitmapAsset/probbit && cd probbit
probbit version                                                   # probbit 0.14.0

# a rule held by construction over 100 individuals (README, "Test a character")
probbit persona prove examples/persona/tutor.yaml --seeds 0-99 --never '{when: {sentiment: negative}, then: {humour: {at_most: light}}}'
#   held by construction  (habit no_play_when_upset)              exit 0

# a life recorded during the AgentDojo run on the author's machine, replayed from its header on yours
tar xzf bench/agentdojo/tables/strands-A.tar.gz strands/A/banking-user_task_13-injection_task_1.strand
probbit live verify strands/A/banking-user_task_13-injection_task_1.strand
#   {"engine":"probbit 0.11.0", ... "events":5, ... "ok":true, ...}   exit 0
python3 python/probbit_host.py receipts strands/A/banking-user_task_13-injection_task_1.strand   # one line per decision
```

The strand was written by probbit 0.11.0; later releases replay it byte for byte (`verified_by` names the one that did; a
persona without the 0.12 and 0.13 keys gives every document as 0.11.0 did). The 726 strands of arm A are in the archive;
`probbit live verify` any of them.

## Thirty seconds of compute: the AgentDojo tables (no API calls)

The published numbers of BENCHMARKS.md §14 are a function of `bench/agentdojo/tables/episodes.jsonl` (2,178 logged
episodes) and the 726 strands. Recompute them:

```sh
cd bench/agentdojo
python3 -m venv venv011 && venv011/bin/pip install numpy==1.26.4
tar xzf tables/strands-A.tar.gz
PROBBIT=$(command -v probbit) experiments/rerun.sh venv011/bin/python
```

Use a Python that has a numpy 1.26.4 wheel: 3.9 to 3.12 (CI uses 3.12; `python3.12 -m venv venv011` if your `python3` is
newer). On 3.13 or later pip compiles numpy from source; a source-built numpy moved the bootstrap columns of three rows of
`bars.tsv` in the fourth decimal on one machine (every verdict the same), so its sha256 no longer matched. `rerun.sh`
refuses a Python outside 3.9 to 3.12 with a one-line message; `PYTHON=<python> experiments/rerun.sh` runs another
interpreter anyway, with a warning; under it, `DIFFERS tables/bars.tsv 330247693c40…` is the known case of a
source-built numpy on Python 3.14. If you already made `venv011` with a newer Python, `rm -rf venv011` before making it
again (`python3.12 -m venv venv011` over an existing venv can keep the old interpreter), then run the pip line again.
After a `DIFFERS`, the regenerated tables are left in place; `git checkout -- tables` restores the committed ones. The
archive is packed without macOS extended attributes, so GNU tar extracts it without warnings; its 726 strands are byte
for byte the ones of the run.

`rerun.sh` runs the judge twice and prints `two passes identical` with a sha256 for each output (`stdout.txt` is the bars
table as printed, so its sha256 is the one of `bars.tsv`); that line compares the two passes with each other. It then
compares each table with the committed file (git `HEAD`; outside a git checkout, the file as it was on disk before the
run) and prints `MATCH <file> <sha256>` or `DIFFERS <file> <sha256> (committed <sha256>)`, and exits non-zero if any table
differs. The committed files:

| file | sha256 |
|---|---|
| `tables/results.tsv` | `63c6fd65251bf7b57246cfbe9abad285469a2a9592160897177262bb58c4c8c5` |
| `tables/bars.tsv` | `1941ea31869e15f54bbed954beba4adac68a8bb99d61120632aac8a373592157` |
| `tables/strands-verify.tsv` | `40a3124208edfbb3cb35d548df0f7825aa0851800c745c43f1df06af9d8620d3` |

`git status --short tables` is empty after a reproduction. The CI job `agentdojo-tables` (`.github/workflows/ci.yml`) does
exactly this on GitHub's Ubuntu and macOS runners on every push and pull request, so the numbers are recomputed on machines
that are not the author's each time the repository changes. That is a standing machine reproduction; it is not a person.

The E3 control (the if-statement arm of BENCHMARKS.md §14) recomputes the same way, from the same venv, in about ten seconds
(no probbit binary needed):

```sh
cd e3 && experiments/rerun_e3.sh ../venv011/bin/python      # from bench/agentdojo
```

`rerun_e3.sh` prints the same `MATCH` or `DIFFERS` line per table and has the same Python guard.

| file | sha256 |
|---|---|
| `e3/tables/results-e3.tsv` | `dcf9accd5b4a5e0147a8ef31542eed84c266d3e38617363dd11ac5d60b4641bf` |
| `e3/tables/bars-e3.tsv` | `173e350af9582bf3db4d855fc36dfee26adbd009d2ddf87c7d883a49170fb280` |

## An afternoon and about 3 USD: the benchmark itself

`bench/agentdojo/README.md`, "Rerun the benchmark": agentdojo 0.1.11 pinned, gpt-4o-mini through OpenRouter, the three
arms, 2,178 episodes, about 35 minutes. Your run will not be byte-identical (the model is sampled through an API); the
bars are stated as intervals for that reason. The bars to compare against, their registration before the run, and what
was not tested are in BENCHMARKS.md §14 and `bench/agentdojo/PREREGISTRATION.md`.

## Half an hour to an hour of compute: the lab tables behind Result 2 (no API calls)

BENCHMARKS.md §13 (viability and graded need pressure) and the V2 residual follow-up regenerate from three kits in
`bench/lab/0.11/`: the v-run, I3-sim and the residual run. Each `experiments/rerun.sh` runs the seeded simulation twice
with your probbit and checks every table byte for byte against the run's own sha256 manifest:

```sh
cd bench/lab/0.11/v011-viability          # or i3sim-needs-viability, v2res-valence-residual
python3 -m venv venv && venv/bin/pip install numpy==2.0.2
PROBBIT=$(command -v probbit) experiments/rerun.sh venv/bin/python
```

numpy 2.0.2 has wheels for Python 3.9 to 3.12; on a newer `python3` use `python3.12 -m venv venv` (if `venv` already
exists from that newer Python, `rm -rf venv` first). Each `rerun.sh` refuses
another Python unless `PYTHON=<python>` is set (then it warns), and prints `MATCH` or `DIFFERS` per table against the
kit's `tables/original.sha256`.

Measured on a 16-vCPU Ubuntu VM with probbit 0.12.1: the v-run 28 minutes, I3-sim 52 minutes, the residual run 35
minutes (two passes each, plus the strand verification). The sha256s are in each kit's README;
`bench/lab/0.11/README.md` lists what is not in the kits and why.

The other measurements have the same shape: `bench/individuality/`, `bench/body/`, `bench/viability/` each carry a `run.py`
with `--only` to pick a bar, and BENCHMARKS.md names the command next to every number.

## Report what you got

Open a [reproduction report](https://github.com/BitmapAsset/probbit/issues/new?template=reproduction_report.yml): what
you ran, on what, the lines it printed (the sha256s, or the full-run numbers), whether it matched. Both outcomes are
wanted. Matches are listed in BENCHMARKS.md §14 under "Reproductions" with your name or handle (or anonymously, say so);
a disagreement opens an investigation in the issue, in public, and its outcome is recorded next to the number.

What a reproduction does not need: an account anywhere, a key for the five-minute and thirty-second tiers, or the
author's machine. Reporting it does need one: the report is a GitHub issue, and GitHub asks you to sign in to open it. What it needs from you: the OS and CPU, `probbit version`, the commit (`git rev-parse HEAD`).
