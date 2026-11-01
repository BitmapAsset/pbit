# RFC 0011: an amplitude register, an exact simulated qubit register as an optional solver tier for small stance programs

Status: **accepted as optional tier (off by default; `engine: {tier: amplitude}` the opt-in; in a release since 0.14.0)**, round 3. Verdict by the registered rule (B4 ≥ 90 %): 96.29 %
of 2,176,371 trait decisions on the recorded corpus agree with the exact default tier; B5 passes (three designed programs differ
from the thermal odds by 0.25-0.27 at the node; 0 of 4,100 bound cases exceed a certificate computed with the tier). The tier
stays off unless a persona asks for it. Turning it on changes the bound
certificate: compute it again with the tier (section 5). Round 4 adds `engine.amplify` (off by default): exact amplitude
amplification in place of the projection, which lands on the projection's numbers with feasible mass 1 (section 5,
"Amplification"). Round 5 adds `engine.schedule` (absent = the default sweep): a longer sweep separates the top two levels
where the thermal tier is indifferent and agrees more elsewhere (section 5, "Schedules and the indifferent set"). Round 6 shows
one such decision end to end, with a demo persona, its two recorded strands and the monitor's two faces (section 5, "The demo").

## 1. What it is, in one paragraph

A register of q qubits (q ≤ 16 on the host, q ≤ 12 on a small microcontroller) simulated **exactly** on a CPU as a state
vector of 2^q complex amplitudes: unitary gates with phase, interference between basis states, measurement by the Born rule,
every step logged so the strand replays byte for byte. It is a simulation of a tiny quantum system and nothing else: no
quantum hardware, no speedup claim, no scaling past what 2^q amplitudes allow (10 qubits 16 KB, 16 qubits 1 MB, 20 qubits
16 MB; a microcontroller with 8 MB of PSRAM reaches about 16; a laptop about 25-28). Sampling a thermal distribution cannot
produce this behaviour (amplitudes carry signs and phases that probabilities do not); the register gets it by paying 2^q
memory and time, which is fine for the programs probbit actually solves (tens of two-valued variables, most of them not
coupled all at once).

## 2. Why bother (the question the measurement answers)

probbit's stance is one exact program over coupled p-bits per turn: biases and couplings → exact marginals → a level per
trait with its odds → a stance line, habits as hard rules on top. The p-bit tier gives a thermal (Boltzmann) distribution.
An amplitude register fed the same program as a Hamiltonian gives a **coherent** distribution: options can cancel (destructive
interference) as well as add, and a short unitary evolution (a transverse-field sweep or a few QAOA-style layers) reaches its
distribution by dynamics, not by equilibrium. Two concrete things that might matter to an agent:

1. **Conflict handling.** When two drives push a trait opposite ways, the thermal answer is a flattened mixture; a coherent
   answer can be a sharpened choice or a cancellation. Whether that is better for a stance is exactly what has to be measured.
2. **Reflex arcs.** A sub-turn surge needs a cheap, deterministic, bounded state shift. A q-qubit register evolved by a fixed
   small circuit is O(2^q · gates) with no sampling noise and no convergence question, which is a candidate mechanism for the
   arc, to be compared with a rule-based register.

It might also matter to nobody: the register may reproduce the p-bit marginals within noise, in which case it is a toy and the
RFC is closed with that number. Both outcomes are publishable with receipts.

## 3. Design (minimal)

- Module `probbit-decide::amp` (std, and no_std with `alloc`; the state vector on the heap): `Register::new(q)`, gates
  `H, X, Z, RX(θ), RZ(θ), CNOT, CZ, RZZ(θ)` (enough for Ising-type programs and QAOA layers), `evolve_ising(program,
  schedule)` (a Trotterised transverse-field sweep, fixed step count), `qaoa(program, p, params)`, `marginals()` (exact, no
  sampling), `measure(seed)` (the Born rule with the engine's deterministic RNG), `digest()` (sha256 of the canonical state at
  6 decimals, like every other emitted float). Amplitudes are two f64: 16 · 2^q bytes exactly.
- A solver tier `amplitude` beside `components` / `knapsack` / `exact`: selected by the persona flag `engine.tier: amplitude`
  (off when absent) and only when every variable has an allowed value and the largest connected component of the program
  has at most `engine.q_max` encoded qubits (default 12, 1-16); otherwise the existing tiers run and the stance document says
  why (`engine.amplitude: "declined: ..."`).
- How a program becomes registers (round 2). **Encoding:** a variable with d candidate values (allowed, and the clamp)
  takes ⌈log2 d⌉ qubits; code c < d is its c-th value, the codes ≥ d are unused (d = 3: codes 00, 01, 10; 11 unused). The start
  is the uniform superposition over the valid codes (for two-valued variables, H on every qubit). **Mixer** (`complete-graph`,
  fixed): per variable exp(i t A_d) with A_d = J − I, the adjacency of the complete graph on its d codes, and the identity on
  the unused codes; closed form e^{−it}(I + (e^{itd} − 1)/d · J), O(d) per block; for d = 2 it is exp(i t X), round 1's mixer
  exactly. **Phase layer:** one general diagonal layer, a_x ← e^{−iγE(x)} a_x with E = −log w of the plan x encodes, on every
  valid basis state (O(2^q) per layer). Pair tables between multi-level variables give 3- and 4-body terms in any qubit
  encoding; the diagonal layer takes them as they are, with no Walsh transform, no residual and no decline. Round 1's RZ/RZZ
  decomposition of the cost is gone: on a 16-variable two-valued chain one general phase layer took 333 µs against 1,938 µs
  for the RZ/RZZ layer, and the whole sweep 48.4 ms against 51.9 ms (x86_64, median of 20); the gates stay in the module. The unused codes keep amplitude exactly 0: the
  engine checks their mass (≤ 1e-15) after every layer and never renormalises. **Components:** the program is split into its
  connected components (two variables are linked by a pair or by sharing a capacity rule, as the `components` tier splits
  it) and each component runs its own register. This is exact: E is a sum over components and the mixer acts per variable,
  so the joint state is the product of the components' states (the repository's example personas need 52-70 encoded qubits
  as one register, 10-15 in their largest component). The sweep is the fixed default `tf-linear-16x4` (16 Trotter steps,
  total time 4, `H(s) = −(1−s) Σ A_i + s E`). The rules (habits, caps, clamps) are a projection per component: the Born
  probabilities of the plans that break one are set to 0 and the rest renormalised, so a stance obeys every habit in force
  exactly as on the other tiers. Plan = per component its most probable feasible plan.
- Receipt fields in the stance document: `engine.tier = "amplitude"`, `engine.qubits` (encoded qubits, summed over the
  components), `engine.components` (each component's qubits), `engine.encoding` (digest of the variable → qubits map with each
  variable's number of valid codes), `engine.mixer` (`complete-graph`), `engine.circuit` (digest of the layer list),
  `engine.schedule` (digest) and `engine.schedule_name`, `engine.state` (digest of the final amplitudes at 6 decimals),
  `engine.feasible_mass` (the Born mass on plans that obey the rules), and the marginals at 6 decimals (the stance odds).
  `live verify` recomputes all of it from the strand alone (the same rule as every other tier). An engine without the tier
  reads the persona's `engine.tier` as a field it does not know: `unsupported by this engine` (exit 5), never a divergence.
- No change to habits (hard rules), the strand format for other tiers, or any public wording. The bound certificate is
  computed from the persona file, so with `engine.tier: amplitude` in the file `persona bound` probes through the tier and
  its epsilon changes; a certificate computed without the flag does not cover the tier (measured in round 3, section 5). Public text, if any: "an exact simulated qubit register as an optional
  solver tier for small programs; measured against the default tier; no quantum hardware".
- **Amplification (round 4, `engine.amplify: true`, off by default).** Instead of projecting, each component's swept state
  |ψ⟩ is rotated onto its feasible plans: the rules are the oracle (a phase on the plans that obey them), the diffusion is
  about the stored |ψ⟩, k = ⌊π/(4θ) − 1/2⌋ standard iterations with θ = asin √a₀ (a₀ the feasible mass) and one final
  iteration whose two phases are chosen so that it lands exactly (`amp::amplify_exact`; mass after asserted ≥ 1 − 1e-12).
  Plan and marginals are read from the register's own Born probabilities. The receipt adds `engine.amplify` (iterations and
  the final phases per component, the mass before and after) and `feasible_mass` is the mass after (1.000000); the layer list
  logs `oracle` and `diffuse` lines. The register holds a second copy of |ψ⟩ while it amplifies (16 · 2^q bytes more).
- Scope (rounds 1-2): the tier answers the turn's solves (the stance and its habit checks). The rest position at the start, the
  expectation's members and the trajectory stay on the existing tiers.

## 4. Bars (registered before any run; PASS/FAIL per bar, failures printed as failures)

- **B1 exactness.** Marginals from the register agree with an independent reference (a NumPy state-vector implementation
  outside the repo) within 1e-9 for q ≤ 12 on 200 random programs and 20 designed ones; gates unitary to 1e-12.
- **B2 replay.** 100 strands with the amplitude tier replay byte for byte on x86_64 and on the no_std build; the 6-decimal
  rounding rule holds for every emitted float; a strand recorded with the tier verifies on an engine without it with the
  documented `unsupported by this engine` exit (5), never a false divergence.
- **B3 cost.** Host: µs per gate and per `evolve_ising` for q = 8, 12, 16; memory = 16 · 2^q bytes exactly; no_std stack
  high-water for q = 8 and 12 ≤ 8 KB (the vector on the heap). Silicon numbers come later.
- **B4 agreement with the p-bit tier (the honest comparison).** On the recorded corpus (653 programs): the level chosen per
  trait agrees with the exact p-bit tier in ≥ 90 % of trait decisions under the default schedule, and the cases that differ are
  listed with both odds. KEEP as an optional tier if ≥ 90 %; DISCARD as a product tier if < 70 % (kept as a lab experiment);
  in between, the verdict is "needs a schedule study", stated as such.
- **B5 interference exists and is bounded.** Three designed programs where two paths cancel: the register's odds differ from the
  thermal odds by ≥ 0.2 at the designed node, and the persuadability bound still holds on every trait (0 exceedances on the
  247-program certificate set).
- **B6 the language.** Every public sentence uses "simulated", "exact", "on a CPU"; no claim of quantum hardware or of an
  advantage over classical computation; no claim of experience.

## 5. Measurements so far

Round 1 (two-valued programs only) and round 2 (multi-level) results are recorded in the change log's Research entries and
the pull request; the verdict on B4 and B5 belongs to round 3.

Round 2 (multi-level programs, simulated exactly on a CPU; x86_64 host, median of 20):

| bar | measured |
|---|---|
| B1 exactness | 220 programs mixing two- and three-level variables (q ≤ 12 encoded; 20 designed, among them one where the unused code would be favoured if it were allowed) against an independent mixed-radix reference (one tensor axis of size d per variable, mixer from an eigendecomposition): max \|Δ marginal\| per level 2.7e-13 (bar 1e-9); unused-code mass after every layer 0 (bar 1e-15); mixer unitarity 8.9e-16 for d = 1-8 (bar 1e-12); round 1's 220 two-valued programs through the new layers: 5.3e-15 |
| B2 replay | 100 strands of the tutor example with the tier (three-level traits; 60 encoded qubits in 12 components, the largest 10): recorded twice byte-equal 100/100, verify 100/100 on x86_64 and on the no_std build, re-written byte-equal 100/100, 0 floats past 6 decimals; an engine without the tier: exit 5 on 100/100 |
| B3 cost | one sweep of 16 steps with level marginals: 42 µs (q 8, four three-level traits), 0.82 ms (q 12, six), 16.3 ms (q 16, eight); all two-valued: 97 µs, 2.1 ms, 43 ms; memory 16 · 2^q bytes of amplitudes plus 8 · 2^q of energies; no_std stack high-water 2,016 B at q 8 and 12 |

The Born mass on plans that obey the rules can be small after the default sweep (as low as 7.3e-05 on the tutor example:
the product over its 12 components; its four-step order component alone allows 24 of its 256 codes); the projection renormalises it. B4 (agreement with the default
tiers, its first pass recorded with the pull request) and B5 have their verdict in round 3.

Round 3 (the verdict round; simulated exactly on a CPU, x86_64 host):

| bar | measured | result |
|---|---|---|
| B4 agreement | every program the recorded corpus's 653 outputs solve on the default tiers (replay, live, init, lint, prove, fuzz, the continuation; strand verification left out), solved again by the tier at q_max 16: 338,567 exact solves (4,304 not exact on the default tiers, excluded), 0 declined; the level per trait agrees in **2,095,627 of 2,176,371 decisions = 96.29 %** (the stance path alone, replay and live, 96.52 %; distinct programs 95.94 %; worst traits valence 81.8 %, emoji 86.6 %, arousal 89.7 %). The corpus itself stays byte-identical with the comparison running beside it (653 outputs, 0 differ) | **PASS (≥ 90 %)** |
| B4 where they differ | 80,744 differing decisions, each listed with both odds. By the default tier's odds on its own level: ≥ 0.60 → 99.97 % agree, 0.50-0.60 → 98.3 %, 0.40-0.50 → 93.6 %, < 0.40 → 90.9 %; in 91 % of the differing cases the default level holds less than half the mass (median 0.385): the two disagree where the thermal answer is a plurality, not a majority. By the trait's component's feasible mass: 99.1 % of the differing cases sit in components with mass ≥ 0.1, so they are not an artefact of the projection | stated |
| feasible mass | the receipt's product over components is < 1e-2 on 36.7 % of the solves (min 2e-06), but per component on 0.04 % (min 2.6e-04, median 1): the product multiplies up to 12 components. A rule-penalty term (E' = E + 4 · violations, the projection kept) measured in the lab lifts every component above 7e-03 and drops B4 to 83.44 % (the order rules' step variables move most): not adopted; the projection stays | measured, not changed |
| B5 interference | three designed programs where two paths into a node differ in energy by D (the default sweep turns the difference into a relative phase), designed and predicted by the reference before the engine ran: register vs thermal odds at the node 0.748 vs 1.000, 0.744 vs 0.990, 0.697 vs 0.969 (gaps 0.252, 0.246, 0.272; the same programs at D = 0: 0.001, 0.000, 0.037); engine vs reference within 1.4e-14 | **PASS** |
| B5 bound | the bound harness (7 personas, seeds 0-9, greedy, mixed and self-sourced campaigns) with the tier on: certificate computed from the persona file with the flag, **0 of 4,100 (campaign, trait) cases above epsilon + r** (largest ratio 1.000), self-sourced events refused 10/10; with the certificate computed without the flag, 1,598 of 4,100 above (largest ratio 1.46): the tier moves resting odds further than the thermal tier, so a certificate must be computed with it. Tightness (≥ 0.9 of epsilon reached) fails on one fixture with the tier (0.85) | **PASS** (with that caveat) |

Verdict: **KEEP as an optional tier** (research; off by default). It reproduces the stance semantics in 96 % of decisions and
differs where the thermal answer is nearly indifferent; it adds interference where designed programs have two paths. Not
verified: silicon cost and replay on a chip.

### Amplification (round 4)

Why it changes nothing in the stance, and why that is the point. Write the swept state as |ψ⟩ = sin θ |g⟩ + cos θ |b⟩, with
|g⟩ its normalised part on the plans that obey the rules and |b⟩ the rest. A phase on the feasible plans and the diffusion
about |ψ⟩ both map the plane of |g⟩ and |b⟩ into itself, so every state the amplification visits is α|g⟩ + β|b⟩; one final
iteration with matched phases makes β exactly 0, and the result is |g⟩ up to a global phase. Its Born probability on a
feasible plan x is |ψ_x|² / a₀, which is what the projection computes by renormalising. So the plan and the odds are the
projection's, and the feasible mass is 1: the projection was never an approximation layered over the register, it is the
distribution exact amplification produces by a unitary evolution, simulated exactly on a CPU. Nothing in the argument depends
on the program or the rules; it needs only the exact landing.

Measured (x86_64 host): on 220 programs (200 random mixed two- and three-level programs and 20 designed ones, a rule each:
seeded random plan subsets at densities 0.03-0.8 or "at most one variable at its top level"; a₀ from 1.7e-06 to 1, 0 to 611
iterations) the marginals with amplification differ from the projection's by at most 1.1e-12 (bar 1e-9), the feasible mass
after is at least 1 − 3.3e-13 (bar 1 − 1e-12; the largest drift, 1.1e-12 above 1, after 611 iterations), and the most
probable plan is the same on 220 of 220. On every program the recorded corpus solves (339,389 exact solves, each run both
ways) the plans are equal on 339,389 of 339,389 and the marginals differ by at most 2.1e-13, so B4 is round 3's to the
decision: 2,095,627 of 2,176,371 = 96.29 %. A 287-event recorded
day of a 52-qubit persona gives the same stance line on 287 of 287 events (per component 1-2 iterations: the per-component
mass is mostly near 1 and the receipt's small product, median 0.058, multiplies 11 components). Cost: 1.02× the projected
solve on that day (1.19 vs 1.17 ms per event) and 1.06× on a 16-qubit component at a₀ = 0.022 (5 iterations); no_std stack
high-water 2,016 B at q 8 and 12. 100 lives with amplification replay byte for byte on x86_64 and on the no_std build, 0
floats past 6 decimals, and an engine without the tier exits 5. Turning it on therefore buys a receipt whose register holds feasible mass 1 and logs
how it got there; it does not buy a different stance.

### Schedules and the indifferent set (round 5)

Round 4 showed amplification computes what the projection computed, so the only place the register can decide differently is
its evolution. `engine.schedule` names it (needs the tier; absent = `tf-linear-16x4`, byte for byte): `tf-linear-16x4` (the
default sweep, 16 steps over time 4), `tf-linear-64x16` (the same transverse-field sweep, 64 steps over time 16: four times the
layers) or `qaoa-p2` (two QAOA layers, γ 0.500000 β 0.500000 then γ 1.500000 β 0.250000, fixed by a grid search on a held-out
third of the corpus's programs and then frozen). The receipt's `schedule` digest and `schedule_name` name it; an unknown name
is refused.

The study: on the recorded corpus (339,389 exact solves, 130,088 distinct programs, 2,183,612 trait decisions), the thermal
tier is **indifferent** (its top two levels' odds within 0.05; its own gap a median 0.028) on **234,004 decisions = 10.72 %**
(fuzz 9.9 %, replay and live 15.5-15.7 %, lint 17.5 %). On those decisions, measured on the two thirds of programs the grid
did not see, the register's gap between its top two levels:

| schedule | median gap | gap > 0.10 | gap > 0.25 | agreement where the thermal tier is not indifferent |
|---|---|---|---|---|
| tf-linear-16x4 (default) | 0.048628 | 9.5 % | 0.1 % | 97.58 % |
| tf-linear-64x16 | **0.151709** | **64.6 %** | **16.0 %** | **98.17 %** |
| qaoa-p2 | 0.094292 | 47.4 % | 7.0 % | 92.99 % |

Two disjoint samples of programs give medians within 0.000697 of each other, so the long sweep's margin (0.103) is about 150
times the noise. By the registered rule (the largest median gap subject to agreement ≥ 90 % on the decisions that are not
indifferent) the choice is `tf-linear-64x16`; it is not the default, because the default is what every strand recorded so far
replays. What the longer sweep buys a decision, stated plainly: a slower sweep keeps more of the simulated register's amplitude
on the most probable plans, so it commits where the thermal answer is a near tie (it takes the thermal plurality on 78.5 % of
those decisions, the runner-up on 20.1 %, a third level on 1.4 %), and it keeps the thermal choice where there is one. It does
not create a preference the weights do not hold. Its cost is the layers: 1.5× the default solve per event on a recorded day of a
52-qubit persona (3.1-3.5× on a lone 8-12 qubit component), 0.8× for `qaoa-p2`; no_std stack at most 2,960 B on every schedule. With amplification on, 165 of the 130,088
programs reach a component mass near 5e-5 under the long sweep and 100+ iterations, where rounding leaves the landing about
1e-12 short of its bar; that component now declines to the default tiers (`amplification missed its 1 - 1e-12 landing`)
instead of stopping the turn (0.12 % of decisions; 0 with the other two schedules). Every number here is a register simulated exactly on a CPU;
the gap is a property of the evolution's length and phases, not a claim about any device.
Replay, measured, and a failure printed as one: 100 lives per schedule record twice byte for byte, verify on the same build and
emit 0 floats past 6 decimals; an engine without the tier exits 5. The no_std build (the chip's math library) re-verifies
the default schedule's lives 100 of 100 on x86_64, but **the long sweep's 0 of 100 and qaoa-p2's 98 of 100**: the platform's
math functions and the no_std library differ in the last bits, and over four times the layers a 6-decimal odd flips. On an arm64
host even the default fails there (0 of 5 on round 4's own path), so that part is not new. Round 7 (below, "One math path")
makes the tier compute with the same math functions on every build: all three schedules now replay 100 of 100 on the no_std
build on x86_64.

### The demo (round 6)

One decision where the thermal tier is a near tie, shown end to end on the recording (std) build: the program, the two odds,
the receipts and the monitor's two faces. Every number is a register simulated exactly on a CPU.

**The program, from the corpus.** The recorded corpus's tutor fixture (seed 1, script 2, the 58th exact solve of its replay)
solves program `a7fae99b114ea2d2` (first 8 bytes of the sha256 of its compact JSON). Humour, regenerated from the corpus:
thermal `playful` 0.452774 vs `light` 0.442258 (gap 0.010516); the register with the default sweep, gap 0.080846; with the
long sweep, `playful` 0.975625 (gap 0.951467). The runner-up, `07d909f824f224e4` (the same fixture, script 3): thermal
`light` 0.444788 vs `playful` 0.425243 (gap 0.019545), long sweep `playful` 0.972868 (gap 0.945965): here the register takes
the thermal runner-up, as it does on 20.1 % of the indifferent decisions.

**The persona and the life** (`examples/persona/demo/`). A fictional tutor reduced to its humour component (warmth, humour,
formality, emoji, the valence mood, their couplings and the two habits that bind them) and the two drives that pull humour
in opposite directions: praise up, criticism (a correction) down. `register.yaml` carries `engine: {tier: amplitude, amplify:
true, schedule: tf-linear-64x16}`; `thermal.yaml` is the same persona without the engine block (`amplify` and `schedule` need
the tier). The life, 17 events at seed 1 on a fixed clock, opens with a run of corrections; at event 9 praise and a correction
arrive together. Both strands verify, record twice byte for byte, emit 0 floats past 6 decimals; event 9 is line 10 of both,
and its two documents solve the same program.

| event 9, humour | none | light | playful | top-two gap |
|---|---|---|---|---|
| thermal (default tiers) | 0.059462 | 0.466563 | 0.473975 | **0.007412** |
| register, default sweep (comparison) | 0.013180 | 0.566151 | 0.420670 | 0.145481 |
| register, long sweep (`register.yaml`) | 0.007171 | 0.249010 | 0.743818 | **0.494808** |

The register document's receipts at event 9: `tier` amplitude, `qubits` 20 (`components` [10, 10]), `mixer` complete-graph,
`schedule_name` tf-linear-64x16, `amplify` iterations [10, 1] with the mass before 0.006196 and after 1, `feasible_mass` 1, the
`state` digest `sha256:6dbfd5a4…1bbe11`, the program digest `sha256:06517240…ab9a66` (the thermal document's too). Events 2 and
11 are near ties of the same kind (thermal gaps 0.033093 and 0.048852, register gaps 0.656343 and 0.725300). The register is not
always the more decisive face: at event 14 the thermal gap is 0.072051 and the register's 0.009221. At event 9 the default sweep
leans to `light` (gap 0.145481); the long sweep commits to `playful`, the thermal plurality.

**The two faces.** The Mood Monitor (`probbit monitor`, section 5.8 of docs/persona.md) on each strand at event 9, replayed
from the strand alone ("replay verified"; the document it serves for event 9 is the recorded one, byte for byte), humour
selected. Thermal: `playful` 47.4 %, `light` 46.7 %, the panel's words "playful, slightly". Register: `playful` 74.4 %,
`light` 24.9 %, "playful, clearly".

![The Mood Monitor on the thermal strand at event 9, humour selected](0011-demo-thermal.png)

![The Mood Monitor on the register strand at event 9, humour selected](0011-demo-register.png)

**The sentence.** On one recorded decision of a fictional tutor, the default solver's odds for its two leading humour levels
sit 0.007412 apart; the same program, solved by a qubit register simulated exactly on a CPU, separates them by 0.494808.
Since round 7 both strands re-record byte for byte and re-verify on the no_std build as well ("One math path", below).

**Cost** (x86_64, best of 5, the life repeated 20 times, 340 events): thermal 2,184 µs per event, the register with the long
sweep 1,822 µs (0.83× the thermal copy), the register with the default sweep 913 µs (the long sweep 2.00× the default sweep).
On this five-variable persona the thermal copy's exact enumeration costs more than the register.

### One math path (round 7)

What an optional tier needs: the same odds on every build, no declines, the numbers re-measured. The register, simulated
exactly on a CPU, now evaluates every function past + − × ÷ (cos, sin, sqrt, asin, acos, atan2, floor) through the `libm`
crate on the std and the no_std build alike: pure Rust, the same code and evaluation order on every target, no platform math
library, no fused multiply-add. Measured on x86_64 (100 fresh tutor lives per schedule recorded on the std build, each
re-verified and re-written by a no_std build of the persona crate on the host):

| schedule | recorded twice byte-equal | verify (std) | verify (no_std) | no_std re-write byte-equal | strands equal to the round-6 engine's |
|---|---|---|---|---|---|
| `tf-linear-16x4` (default) | 100/100 | 100/100 | **100/100** (round 5: 100/100) | 100/100 | 100/100 |
| `tf-linear-64x16` | 100/100 | 100/100 | **100/100** (round 5: 0/100) | 100/100 | 0/100 |
| `qaoa-p2` | 100/100 | 100/100 | **100/100** (round 5: 98/100) | 100/100 | 98/100 |

The tutor's default-schedule strands are byte-identical to the round-6 engine's and both demo strands re-record byte for byte,
but the change is in the last bits and a larger persona's default strands can move: on a recorded day of a 52-qubit persona
they did (qaoa-p2's did not); the long sweep's tutor strands all moved. An unreleased tier allows it; no shipped fixture moved.
The study of round 5 re-run on this tree: 0 programs decline under any schedule (round 5: 165 under the long sweep), the long
sweep's median gap on the indifferent decisions 0.151709 and agreement on the others 0.981671 (round 5: 0.151709, 0.981717,
now over the formerly declined decisions too), the default's and qaoa-p2's rows unchanged to 6 decimals; cost per event on that
day 1.50x the default for the long sweep (branch vs the round-6 engine 0.95-1.00x); no_std stack at most 2,944 B per schedule. Both demo strands re-record byte for byte. The arm64 leg is not measured (no arm64 runner for the replay).

The missed landing: the diffusion about a swept state of squared norm 1 + ε is not unitary (|Dv|² = |v|² + 4ε|⟨ψ|v⟩|²), so
the error grows with every iteration, linearly in their number; at a tiny feasible mass (hundreds of iterations) the long
sweep's ε left 165 corpus programs about 1e-12 short of the landing, and the same ε of the other sign is round 4's drift above
1. `amplify_exact` now renormalises the swept state once before the standard iterations (squared norm by a compensated sum;
only when there are iterations, so the one matched step of a mass above 1/4 is unchanged), and a mass that is not a number
declines like a miss. A rotation in the plane would have hidden the cause; renormalising keeps every iteration the same
unitary step. The receipt's mass before stays the swept state's; `feasible_mass` stays 1.

## 6. What this RFC does not claim

That a CPU can do what quantum hardware does at scale (it cannot: section 1), that the register has experiences (it is
arithmetic), or that it beats the p-bit tier (B4 decides). What it can honestly give: superposition with real phases and
interference inside the agent's decision, exact and replayable, simulated on a CPU at a size a laptop and a small chip can
afford.
