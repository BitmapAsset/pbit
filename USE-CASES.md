# probbit use cases: what the measurements support

**One processor, many kinds of decision.** If you can write a decision as choices, scores and rules that must never break,
it runs on the same p-bit engine: tasks to agents, steps to tools, jobs to time slots, items under a budget, cells of a
puzzle, the temperament of an agent. A feasible answer supplies a rule-abiding candidate plan,
odds (exact whenever the structure lets probbit count them), and a verdict that refuses when its
diagnostics fail. And every shape gets the same honesty: where a solver, an annealer or plain backtracking wins, the table says so.
An infeasible or resource-limited problem need not produce a plan. A retained candidate on refusal is diagnostic only;
check `plan_status`, `released_plan` and the escalation set before host actions. This is a finite-domain probabilistic
decision processor, not a replacement for arbitrary CPU computation or a guarantee that any application will fit its limits.

Every "gives" cell below points to a measured table in `BENCHMARKS.md` (§ numbers) or a separate run; every "do not promise"
cell points to where probbit lost or was not tested. All measurements are on one Apple M4 Mac mini with synthetic or generated
problems. **Nothing here was measured on a customer's data.** Tags: *(inferred)* = reasoned, not run; *(unmeasured)* = not run.

## When to reach for probbit, and when not to

Use it when all three hold:
1. the decision has **hard rules** (who may do what, quotas, one-of constraints) that must never be broken;
2. you want **odds for each part** of the decision, not only one answer;
3. you want the tool to **say "I can't vouch for this part"** when its diagnostics fail (a heuristic gate with published
   counterexamples, not a proof), on a laptop, offline.

Do not use it when:
- **you only need one best plan.** An ILP solver (HiGHS) proved the optimum of the 300-task router in 52-352 ms and was faster
  than probbit's default path on 5 of 6 queues; probbit's plan was 0.45-2.0 nats short (§2.2, three runs). Use ILP / CP-SAT, or greedy + polish.
- **the instance is tiny.** Then probbit answers exactly anyway (12 tasks: 7.1-8.0 ms p50, §2.3), so its sampler adds nothing.
- **you need raw search speed on a known puzzle class.** Backtracking counts sudoku solutions 15-102x faster per puzzle than probbit's
  exact tier, and probbit's sampler refuses every generated sudoku (20/20; in the current build a puzzle that naked singles solve passes the gate (`diagnostics_passed`), §4).
- **you need the best cut or lowest energy.** Simulated annealing found better max-cuts at equal time (§3).

## Catalogue by problem shape

| problem shape | how it maps to the probbit instruction set (`probbit-ir`) | what probbit gives (measured) | do not promise | who might use it *(inferred)* |
|---|---|---|---|---|
| **Assign items to resources under rules** (tasks to agents/models/people, tickets to agents, jobs to machines) with quotas and a same-group preference | one categorical variable per item over its allowed resources; unary log-weights = your scores; at-most-`cap` per resource; a Potts pair per same-group pair (`probbit decide` lowers to this) | **0 rule violations by construction** (300-task `agent_router` example: per-task argmax breaks 144 rules incl. 60 PII tasks sent to cloud models; probbit 0); **per-item odds you can check**: 1,731 released tasks on queues with an exact answer, 0 wrong (§2.1, current gate, re-run on 0.2.0; 1,916 on the 0.1.0 gate); **refusal/escalation** by budget (§2.3, demo 2 below); **exact odds in 0.17-28 ms** up to ~24 tasks (§2.1, §2.3); what-if clamps, including clamps that fill a worker (acceptance test `forced_caps_do_not_freeze_the_gate`: 3 queues whose clamps fill a worker + 1 forced queue each pass the gate within 0.05 TV of exact, green on the current build; a 0.1.0 record of 10/10 passed, 0 false, was not re-run) | the single best plan (ILP wins, §2.2); full coverage on saturated or strong-affinity queues (51% of tasks released on the oracle queues on the current gate, 58% on 0.1.0, §2.1); speed (the sampled path spends its ~0.3 s budget by design, §2.3) | AI platform teams routing LLM tasks; support operations; internal-tools developers |
| **Binary pairwise models** (Ising, max-cut, binary Markov random fields) | one two-valued variable per spin; `table` pairs = couplings; unary = fields | **per-spin odds whose gate verdict was checked against brute force**: 144 runs, 61 passed the gate, 0 false whole answers, 929 released spins, 0 wrong (§3, re-run on 0.2.0; the 0.1.0 gate: 72 passed, 1,346 released, 0 wrong); exact odds and log Z by the exact tiers on small models; at equal time, cuts within 0.06-0.7% of simulated annealing in anneal-only mode and 0.17-1.75% short when sampling (the mode that gives odds, §3) | better cuts than simulated annealing (it won by 4-7 edges, §3); log Z from the sampler (not implemented); correctness beyond 16 spins (the brute-force oracle stops there) | researchers and teachers of p-bit / Ising computing; people prototyping before renting annealer time |
| **Constraint satisfaction with odds** (graph colouring, one-hot puzzles, slot assignment) | a categorical variable per node over colours/digits; negative Potts or at-most-1 caps for "differ"; clamps for givens | **exact per-variable odds and solution counts** from the exact tier (sudoku odds equal a backtracking count on 20 puzzles; colouring 0.00-5.70 ms median, §4); the sampler released 525 vertices, 0 outside tolerance (§4; 468 in an earlier build) | speed against backtracking (15-102x slower per puzzle, §4); sampler coverage on frozen or few-solution instances (sudoku: refused 20/20; 3-colouring: 9 of 14 refused on the default set in the current build, 59 of 82 over 6 sets, §4) | CSP and puzzle tooling, teaching, frequency/register-assignment prototypes *(unmeasured on real instances)* |
| **Scheduling with capacity per time slot** (unit-time jobs, release/deadline windows, precedence) | a variable per job over slots; allowed = window; one cap per slot; precedence i -> j as pair caps "not (i at a and j at b)" for a >= b (O(T^2) caps per edge) | **exact odds on small instances** and a gate that released 428 jobs on 39 oracle instances, 0 outside tolerance; at 200-500 jobs, anneal from a greedy plan beat restarted greedy + repair at equal 100 ms on 4/5 seeds each (§4.3) | optimal schedules at scale (an ILP solver, HiGHS, proves the 200-job optimum in 0.68-0.95 s; probbit's plan is 3.3-4.3 nats short, §4.3); checked odds at scale (at 200 jobs the sampler needs 0.7 s just to start, refuses at 1 s, releases 153-171/200 at 5 s on 3 instances in an earlier build (re-run on a later build, N = 3: 156-173 on two, the third refused) (earlier build, start inside the budget; 2 runs each) with no oracle to check them); multi-period durations (unit jobs only) | planners prototyping small shift/slot problems *(inferred)* |
| **Knapsack / budgeted selection** (pick items under a weight or cost budget; bin packing: one budget per bin) | one variable per item over {out, in} (or over bins); unary = value of taking it; one IR `linear` rule per budget, `[item, "in", weight]` terms (a weighted cap, never replicated members) | **exact odds and the optimum on small programs, gated odds beyond enumeration** (§4.4; bench/knapsack.py vs an independent log-space DP, Apple M4, load 10-14): n = 20: exact enumeration 26-28 ms, plan = DP optimum; n = 40 / 80 (sampled at defaults, ~420-460 ms): plan = DP optimum on 6/6 instances (greedy by value/weight 0-0.29 nats short), sampler 24/24 `diagnostics_passed`, 1,440 items released, max \|odds - DP\| 0.0060, 0 outside 0.05 | beating a knapsack DP: on a pure knapsack the O(n x budget) DP gives exact odds and the optimum directly, and probbit is the slower path; probbit adds the same rule inside programs a DP does not cover (pair terms, quotas, precedences, several budgets at once) | procurement / portfolio shortlists, ad or compute budget allocation, bin packing of jobs onto machines *(inferred)* |
| **Bin packing with preferences** (jobs onto machines, items into containers, each with a size; one capacity per bin) | one variable per item over the bins; unary = preference; one `linear` rule per bin with `[item, bin, size]` terms | (§4.4; bench/binpacking.py vs an exact DP over bin-load vectors) n = 24 / 36 items x 3 bins (2.8e11 / 1.5e17 assignments): default plan = DP optimum 4/4 (largest-first greedy 0.34-2.45 nats short), sampler 16/16 `diagnostics_passed`, max \|odds - DP\| 0.0051 | anything about many bins or large capacities (not run); a dedicated bin-packing solver for the best packing alone | cluster schedulers, logistics loading *(inferred)* |
| **Image denoising / binary Markov random fields on grids** (the classic p-bit demo) | one {0, 1} variable per pixel; unary = log-odds of the observed pixel; Potts pairs between neighbours | **the exact posterior and MAP image, checked**: 8 x 12 grids vs a row transfer matrix (§4.5; bench/denoise.py): 12/12 sampled answers `diagnostics_passed`, 1,152 pixels released, max \|odds - exact\| 0.0048; the default plan equals the exact MAP on 3/3 images | better images than a classical method: pixel error depends on the model; ICM was closer to the clean image on 1 of 3 images; a transfer matrix is exact and faster on narrow grids | teaching / demos of probabilistic computing, small vision or sensor-fusion MRFs *(inferred)* |
| **Tree-structured probabilistic models** (hierarchies, phylogeny-like or parse-tree factor graphs, chains) | one variable per node; one `table` pair per edge | **exact odds and log Z at defaults** (the forest tier): random 3-value trees n = 200 / 1,000 match an independent sum-product to print precision in 4-12 ms; the gated sampler on the same trees 16/16 `diagnostics_passed`, 9,600 items, worst 0.0249 (§4.6; bench/tree_infer.py) | speed against a hand-written sum-product (the same algorithm) | any team that already has a tree model and wants one tool for it and its loopy variants *(inferred)* |
| **Budgeted selection with correlations** (portfolio / feature / project shortlists with a cost budget and pairwise synergies or overlaps) | in / out per candidate; unary = value; one `linear` budget; Potts or table pairs for synergy (+) or overlap (-) | (§4.7; bench/portfolio.py vs an exact chain x budget DP) n = 30 / 60: default plan = DP optimum 4/4 (pair-blind greedy 1.7-4.8 nats short), sampler 16/16 `diagnostics_passed`, 720 released, max \|odds - DP\| 0.0036 | dense real-world covariance (not run), returns or financial advice | product / R&D portfolio triage, feature selection under a compute budget *(inferred)* |
| **SAT: counting and odds over the solutions** (configuration / feasibility questions: how many valid configurations, how often is option x on) | one {F, T} variable per proposition; each hard clause = an IR `tables` forbid of its falsifying tuple | (§4.8; bench/sat.py, planted 3-SAT at ratio 4.0) n <= 18: exact solution counts and odds at defaults in 2.6-3.5 ms | sampling solutions of hard formulas: the sampler refused 16/16 there (isolated solutions); MaxSAT (no soft ternary terms in v1); finding one solution (use a SAT solver / WalkSAT) | product-configuration and compliance checks over small rule sets *(inferred)* |
| **Agent tool-call planning** (which tool at which step under call quotas, ordering rules and a token budget) | one variable per step over the tools; unary = the policy's scores; `table` pairs = transition preferences; value caps, `implies`, `forbid` and a `linear` token budget | (§4.9; bench/agent_planner.py vs brute force) 6-7 steps x 6 tools: the exact best plan (= brute-force optimum 4/4; greedy step-by-step 1.5-2.2 nats short) and per-step action odds in 3.0-5.4 ms; sampler 16/16 `diagnostics_passed`, max \|odds - brute\| 0.0029 | long horizons (not run past 7 steps), anything about real agent policies (synthetic scores) | agent frameworks that want a checked plan + odds under budgets *(inferred)* |
| **Agent temperament** (the individuality layer: a persona that lives outside the model) | one variable per trait or mood over its levels; unary = priors, genes from a seed, the turn's evidence; soft pairs; habits as hard rules (`probbit persona` compiles all of it to one probbit-ir program per turn; docs/persona.md) | (README "The individuality layer", Apple M4, 2026-10-02) a whole turn in process median 0.20-0.32 ms; 0 habit violations over 2,160 turns of 12 random personas, each of the 481 binding habits broken by the habit-free twin; 1,000-turn replays byte-identical across processes; three seeds of the tutor persona differ by a mean TV of 0.12-0.18 in their trait odds, the same seed by 0 | that a model writes in the stance it is given (not measured here); odds that predict a user's reaction; text rules or safety (keep those in the prompt, a checker or plain code) | agent frameworks, companion and tutor apps, multi-agent products that want a stable character per agent *(inferred)* |
| **Assignment with side constraints** (tasks onto people / machines with effort budgets, conflicts, if-then rules, quotas) | one variable per task over the workers; `linear` effort budget per worker, `all_different` per conflict pair, `implies`, value caps | (§4.10; bench/assign_side.py vs brute force) n = 8 / 12 tasks x 3 workers: exact at defaults in 2.8-6.1 ms, plan = optimum 4/4 (greedy 0-1.71 nats short); sampler 16/16 `diagnostics_passed`, max \|odds - brute\| 0.0029 | ILP-solver comparisons for this family (no scipy on the measuring machine); large n (enumeration-sized only) | staffing, ticket routing with rules *(inferred)* |

## Industry and user map

| who | problem | shape | status |
|---|---|---|---|
| AI agent platforms, agent-harness builders | route tasks to models or humans under PII / data-residency rules, quotas and workflow affinity | assignment | the flagship demo is this shape, on synthetic data; **not measured on real traffic** |
| Customer support, IT service desks | ticket to agent under skills and queue caps | assignment | same shape *(inferred)*; unmeasured on real tickets |
| Cloud, HPC and CI operators | job to machine or pool under quotas and co-location preference | assignment | same shape *(inferred)*; unmeasured; the router has no time dimension (probbit-ir handles unit-time slots, §4.3) |
| Workforce planning (one period) | person to shift under caps and rules | assignment | same shape *(inferred)*; multi-period rostering: only unit-time slot scheduling was measured (synthetic, §4.3) |
| Research labs, universities | sample Ising/Potts models with a checkable error bound; teach probabilistic computing | binary pairwise, CSP | **measured** (§1, §3, §4); the most honest fit today |
| Teams evaluating Ising / annealing hardware | prototype locally, then hand the model over: `probbit ir` prints a router document's one-hot QUBO form (energy = -log weight); it reads router documents only (a `probbit run` program has no QUBO export yet) | binary pairwise | the lowering exists; **no hardware was run**; rules hold by construction only on probbit's own sampler: a p-bit sampler on the QUBO form leaked infeasible samples at low penalty weights (§7, 3 instances, one run each, this CPU) |
| Telecom, compiler teams | frequency or register assignment as colouring, with odds | CSP | measured only on random graphs up to 14 vertices; exact tier practical only at small sizes |
| Individual developers | any of the above on a laptop, offline | all | default: `libm` for amplitude-register math; `nostd` adds `hashbrown` and `portable-atomic`; offline builds need cached dependencies |

## Integration (what exists today)
- **CLI + JSON** on stdin/stdout, exit codes 0 plan / 1 infeasible / 2 bad input / 3 refused. Every `bench/` script drives it.
  Process overhead is ~2-5 ms (§2.3: 9.0 ms wall vs 7.1 ms inside probbit at 12 tasks, 303.1 vs 299.0 at 300; 0.2.0 re-run 10.1 vs 8.0
  and 314.3 vs 309.4).
- **Python**: `python/probbit.py`, a stdlib-only subprocess wrapper for decisions, evaluation, personas and live events;
  typed errors and deadlines. CI runs unittest discovery over `python/`. No native bindings (one process per call).
- **Schema**: `docs/probbit-ir.schema.json` (JSON Schema of the probbit-ir wire format; a test keeps it equal to the parser);
  `probbit <command> --help` lists every flag with its default.
- **Rust**: the `probbit-decide` crate (router front-end) and `probbit-ir` (general programs, JSON v1 in `docs/probbit-ir-json.md`).
- **MCP server**: `probbit mcp` serves the ten documented tools over stdio; see [docs/agents.md](docs/agents.md).
- **OpenClaw plugin**: [plugins/openclaw](plugins/openclaw/README.md), `openclaw plugins install npm:probbit-stance --accept-capabilities`: one
  individual per listed session, one event per user message, the stance block in the turn, the footer in the chat, a
  strand per lane, a shadow gate for destructive tool calls.
- **Beside an agent loop**: `python/probbit_host.py` and `examples/node/stance-host.mjs` (one individual per conversation, one
  event per model call, the stance block in the system prompt, the strand as the receipt), bound to Claude Code hooks, the
  OpenAI Agents SDK, LangChain and the Vercel AI SDK in [docs/adapters.md](docs/adapters.md); tested against the binary
  here, the framework paths written to their contracts and marked where not run.
- **Receipts for every decision**: `probbit live verify` replays a strand from its header; `python3 python/probbit_host.py
  receipts STRAND` renders one record per decision (inputs as used, stance, habits, the chain). [docs/receipts.md](docs/receipts.md)
  says what that proves (the engine's decision and its inputs) and what it does not (the model's obedience, the words).
- **Host-enforced incident regression**: [examples/agent-harness](examples/agent-harness/README.md) records a local tool
  failure, reproduces history-dependent retries, gates dispatch and checks a repaired rule. A bounded example, not a
  production authorization system.
- **Running beside other work**: `--threads`, `--cpu-limit`, `--priority low`, `--mem-limit-mb`, `--progress`, `probbit stats` (§5).
  Reproducible runs: fixed `--sweeps` plus `--polish-sweeps`. The defaults are wall-clock (sampling and polish), so the odds,
  the verdict, the released set and the plan can all vary run to run and machine to machine.

## Three flagship 60-second demos (each re-run on the final build on the M4)

**1. The router that won't break policy**: `cargo run --release -p probbit-decide --example agent_router` (3.1 s in an earlier run, 4.7 s in a later one).
You see: a naive per-task router on 300 agent tasks breaks 144 rules (60 PII tasks to cloud models, 3 production-DB
migrations to cheap models, a worker 78 over quota); probbit breaks 0, passes the whole 300-task answer through the gate at 200 ms, prints
per-task odds, and re-plans a what-if ("this customer demands a human"). On a 12-task queue with an exact answer every
released task is within 0.0067-0.0074 of the exact odds over the recorded runs (0.0073 and 0.0074 in two earlier runs, 0.0074 in a later one, 0.0067-0.0072 in three 0.2.0 re-runs; the budget is wall-clock). Shows, on
these synthetic queues: rules by construction, checkable odds, what-if. Does not show:
anything about real traffic, cost or latency savings; the plan is not claimed optimal (§2.2).

**2. It says when it can't vouch**: `probbit demo --tasks 300 --hard | probbit decide --sweeps S --polish-sweeps 400` with
S = 100 / 700 / 3300 gives `refused` (exit 3, 0 released) / `partial` (83 released, 217 escalated on the current build, 0.2.0 re-run; 86 / 214 in an earlier build) / `diagnostics_passed` (300 released; 0.1.0 printed `certified`).
Fixed work, so the whole output except timing is identical on every run and at `--threads` 1 and 4 (checked);
0.14 / 0.32 / 1.12 s on the M4 with 4 threads. The wall-clock form `--budget-ms 50 / 200 / 1000` gave refused
5/5, partial 5/5 (91-96 released) and passed 5/5 (`certified` in 0.1.0; on an earlier binary; re-run on a later binary: the same, 91-95 released at 200 ms, wall
0.21 / 0.36 / 1.17 s; re-run on the final binary, 3 runs at load ~6: refused / 90-92 released / passed; 0.2.0 build, 5 runs each at load 2-3: refused 5/5 / 87-91 released / passed 5/5), but shifts to longer budgets on a slower machine. Note: the refusal threshold is a cliff, not a ramp, and
more work can turn `partial` into `refused`: 168 sweeps gave `partial` (R-hat 1.00197) and 250 gave `refused` (R-hat 1.00208 > 1.002).
Shows: tasks whose diagnostics fail are escalated, not released, and more work moves the verdict from refused to partial to
diagnostics_passed. Does not show: that released tasks are always right (the diagnostics are heuristics that adversarial
families can pass: BENCHMARKS "Known failure modes"), or that escalated tasks were hard
(some are refused needlessly: §2.1 saturated queues).

**3. A trust meter checked against brute force**: `cargo run --release -p probbit-ir --example maxcut_oracle` (0.8 s once built).
You see: 144 sampling runs on frustrated 12- and 16-spin max-cut / Ising models, each checked against the exact odds from
enumerating every state: 61 passed the gate, **0 false whole answers**, 929 released spins, **0 wrong** (0.2.0 gate, re-run on the final build; the 0.1.0 gate: 72 passed, 1,346 released, 0 wrong); with too little work (500 sweeps) the gate releases nothing
rather than something wrong (the 0.1.0 gate still released 417 spins at 500 sweeps, none wrong). Shows: the gate's verdict
held on models small enough to check; 0 false of 61 bounds this family's false-pass rate below ~5% (95%, rule of three), not
at zero. Does not show: anything beyond 16 spins, or better cuts than simulated annealing (§3).
