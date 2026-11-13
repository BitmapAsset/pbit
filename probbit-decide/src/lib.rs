//! probbit-decide: JOINT constrained decisions on a p-bit (Gibbs) substrate.
//!
//! Model (tasks -> agents, a constrained Potts field):
//!   log w(x) = sum_i h[i][x_i] + lam * #{(i,j) same group, x_i == x_j}
//!   hard: allowed[i][x_i] (skills), load[a] <= cap[a] (capacity), clamps (what-if).
//! Router: exact enumeration when the feasible set is small, else constraint-preserving
//! Gibbs (site heat-bath restricted to feasible values + Metropolis swap moves) with
//! multi-chain split-R-hat gate -> "UNMIXED: escalate" refusal.
#![cfg_attr(not(feature = "std"), no_std)]
#[cfg(not(feature = "std"))]
#[allow(unused_imports)]
#[macro_use]
extern crate probbit_nostd as std;
#[cfg(not(feature = "std"))]
#[allow(unused_imports)]
#[macro_use]
extern crate alloc;
#[allow(unused_imports)]
use std::prelude::rust_2021::*;
use probbit_core::Philox4x32;
use std::collections::HashMap;
pub mod oracle;
pub mod ir;
pub mod amp;

/// Tasks up to which `Problem::logw` uses the pairwise loop. R19.8 re-measured (`examples/logw_threshold.rs`, 6 workers, groups
/// of 4 / 8, median of 7 alternating rounds, load ~8.6): linear / pairwise ns per call = 1.52 / 1.25 at 96 tasks, 0.96 / 0.92 at
/// 128, 0.97 / 0.68 at 160, 0.78 / 0.63 at 192: the crossover is between 96 and 128, so 96 stays (raising it to 150-200 would be slower).
pub const LOGW_PAIRWISE_MAX: usize = 96;
#[derive(Clone, Debug)]
pub struct Problem {
    pub t: usize, pub a: usize,
    pub h: Vec<f64>,         // t*a judge logits
    pub allowed: Vec<bool>,  // t*a skill mask
    pub cap: Vec<usize>,     // per agent
    pub group: Vec<usize>,   // group id per task (usize::MAX = none)
    pub lam: f64,
    pub clamp: Vec<Option<usize>>,
    /// exact block heat-bath over each same-customer group per sweep (mixing accelerator for strong affinity)
    pub block_moves: bool,
    /// Metropolis group-pair swaps: members of group g1 on agent a <-> members of g2 on agent b (inter-group modes)
    pub pair_swaps: bool,
    /// R19 P1.1(a): one global two-value flip per sweep (every free task with exactly two allowed workers switches at once;
    /// `probbit_ir::Chain::global_flip`). Lowered as `Model::collective`; the two samplers stay bit-identical with it on.
    pub collective: bool,
    /// R19 P1.1(d), WIP: one Wolff cluster move per sweep over same-group affinity bonds (`probbit_ir::Chain::cluster_move`;
    /// lowered as `Model::cluster`; bit-identical through the lowering). Off by default.
    pub cluster: bool,
    /// R19 P1.1(c), WIP: max(t/4, 1) three-cycle rotations per sweep (`probbit_ir::Chain::cycle3`; lowered as `Model::cycles`;
    /// bit-identical through the lowering). Off by default.
    pub cycles: bool,
}

impl Problem {
    /// Per task, the other tasks of its group (ascending; none if ungrouped). Bucketed by group; it scanned every task for
    /// every task (O(t^2): 3.7 s of a 100,000-task decision). Same lists.
    pub fn mates(&self) -> Vec<Vec<usize>> {
        let mut by: std::collections::HashMap<usize, Vec<usize>> = std::collections::HashMap::new();
        for (j, &g) in self.group.iter().enumerate() { if g != usize::MAX { by.entry(g).or_default().push(j); } }
        (0..self.t).map(|i| if self.group[i] == usize::MAX { vec![] } else { by[&self.group[i]].iter().copied().filter(|&j| j != i).collect() }).collect()
    }
    pub fn ok(&self, i: usize, a: usize) -> bool {
        self.allowed[i * self.a + a] && self.clamp[i].map_or(true, |c| c == a)
    }
    /// It was O(t^2) (every pair of tasks tested for a shared group): ~2 s per call at 100,000 tasks in groups of 4, paid by
    /// the polish once per chain before its clock started and once per sweep (`--polish-ms 50` took 2.3 s, 8.1 s at `--threads 1`).
    /// Now O(t + same-worker mate pairs), adding the same terms in the same order (h_i, then lam once per LATER task of i's group on
    /// the same worker), so the value is bit-identical. Up to 96 tasks the pairwise loop stays (the map made the polish ~1.5x
    /// slower per sweep at 20-30 tasks and ~1.5x faster at 300; 96 ~ the geometric middle).
    /// R19.8 (P2.3 item 3): the pairwise loop up to LOGW_PAIRWISE_MAX tasks, the linear path above (same bits either way).
    pub fn logw(&self, x: &[usize]) -> f64 { self.logw_by(x, self.t <= LOGW_PAIRWISE_MAX) }
    /// `logw` by the pairwise loop (`pairwise`) or the linear path: bit-identical, only the cost differs (for the threshold
    /// benchmark `examples/logw_threshold.rs`).
    pub fn logw_by(&self, x: &[usize], pairwise: bool) -> f64 {
        if pairwise { let mut e = 0.0;
            for i in 0..self.t { e += self.h[i * self.a + x[i]];
                for j in i + 1..self.t { if self.group[i] != usize::MAX && self.group[i] == self.group[j] && x[i] == x[j] { e += self.lam; } } }
            return e; }
        let mut later = vec![0u32; self.t]; let mut seen: std::collections::HashMap<(usize, usize), u32> = std::collections::HashMap::new();
        for i in (0..self.t).rev() { if self.group[i] != usize::MAX { let c = seen.entry((self.group[i], x[i])).or_insert(0); later[i] = *c; *c += 1; } }
        let mut e = 0.0;
        for i in 0..self.t { e += self.h[i * self.a + x[i]]; for _ in 0..later[i] { e += self.lam; } }
        e
    }
    pub fn violations(&self, x: &[usize]) -> usize {
        let mut load = vec![0usize; self.a]; let mut v = 0;
        for i in 0..self.t { load[x[i]] += 1; if !self.ok(i, x[i]) { v += 1; } }
        for a in 0..self.a { if load[a] > self.cap[a] { v += load[a] - self.cap[a]; } }
        v
    }
    pub fn with_clamp(&self, i: usize, a: usize) -> Problem { let mut p = self.clone(); p.clamp[i] = Some(a); p }
    /// Lower this assignment front-end to the general IR (`probbit_ir::Model`): one variable per task over the agents,
    /// the judge logits as unary log-weights, a Potts coupling (+lam when equal) per same-group pair, and one capacity
    /// constraint per agent over its allowed (task, agent) pairs; clamps stay clamps. Through this lowering the IR's
    /// enumeration, sampler (site + swap moves) and gate are bit-identical to this crate's (acceptance `ir_lowering_bit_identical`).
    /// Panics if the IR rejects the model (more than 65,535 workers; `lower_until` declines instead, and the CLI's loader refuses them).
    pub fn lower(&self) -> probbit_ir::Model { self.lower_until(None).expect("a Problem always lowers") }
    /// `lower` that gives up (None) once `hard` passes: the clock is read once per group member while the O(group size^2)
    /// Potts pairs are built, and before and after `Model::new` (R19.8, P2.3 item 2: the CLI's `--exact-ms` cap did not
    /// cover this lowering, ~200 ms on one 3,000-task group). None = `lower`. Also None when the IR rejects the model (more than
    /// 65,535 workers): an `expect` here aborted `probbit decide` (exit 134, empty stdout) on a 65,536-worker document.
    pub fn lower_until(&self, hard: Option<probbit_core::rt::Instant>) -> Option<probbit_ir::Model> {
        let late = || hard.is_some_and(|h| probbit_core::rt::Instant::now() >= h);
        let mut gm: std::collections::BTreeMap<usize, Vec<usize>> = Default::default();
        for i in 0..self.t { if self.group[i] != usize::MAX { gm.entry(self.group[i]).or_default().push(i); } }
        let mut pairs = vec![];
        for mem in gm.values() { for (q, &i) in mem.iter().enumerate() { if late() { return None; } for &j in &mem[q + 1..] { pairs.push(probbit_ir::Pair { i, j, c: probbit_ir::Coupling::Potts(self.lam) }); } } }
        let caps = (0..self.a).map(|a| probbit_ir::Cap { weights: vec![], members: (0..self.t).filter(|&i| self.allowed[i * self.a + a]).map(|i| (i, a)).collect(), limit: self.cap[a] }).collect();
        if late() { return None; }
        let mut m = probbit_ir::Model::new(self.t, self.a, self.h.clone(), self.allowed.clone(), self.clamp.clone(), pairs, caps).ok()?;
        m.collective = self.collective; m.cluster = self.cluster; m.cycles = self.cycles; if late() { None } else { Some(m) }
    }

    /// Feasible initial state via capacitated bipartite matching (augmenting paths), best-logit first.
    pub fn feasible_init(&self) -> Option<Vec<usize>> { self.feasible_init_keyed(None) }
    /// Over-dispersed feasible start. Same augmenting-path matching, but tasks are inserted in a random order and
    /// each task tries agents in a random order, so different chains start in different capacity "macro-modes"
    /// (which group owns which shared agent). With the deterministic best-logit start all chains began in the same
    /// mode, and under strong affinity the gate then certified answers off by TV 0.4-0.6.
    pub fn feasible_init_rand(&self, rng: &mut Philox4x32) -> Option<Vec<usize>> {
        let key: Vec<f64> = (0..self.t * self.a + self.t).map(|_| rng.f64()).collect(); self.feasible_init_keyed(Some(&key))
    }
    fn feasible_init_keyed(&self, key: Option<&[f64]>) -> Option<Vec<usize>> {
        let mut x = vec![usize::MAX; self.t]; let mut on: Vec<Vec<usize>> = vec![vec![]; self.a];
        fn aug(p: &Problem, i: usize, seen: &mut [bool], x: &mut [usize], on: &mut [Vec<usize>], key: Option<&[f64]>) -> bool {
            let mut order: Vec<usize> = (0..p.a).filter(|&a| p.ok(i, a)).collect();
            match key { None => order.sort_by(|&u, &v| p.h[i * p.a + v].partial_cmp(&p.h[i * p.a + u]).unwrap()),
                Some(k) => order.sort_by(|&u, &v| k[i * p.a + u].partial_cmp(&k[i * p.a + v]).unwrap()) }
            for &a in &order { if on[a].len() < p.cap[a] { x[i] = a; on[a].push(i); return true; } }
            for &a in &order { if seen[a] { continue; } seen[a] = true;
                for k in 0..on[a].len() { let j = on[a][k];
                    if aug(p, j, seen, x, on, key) { // j moved away (pushed onto another agent)
                        let pos = on[a].iter().position(|&z| z == j).unwrap(); on[a][pos] = i; x[i] = a; return true; } } }
            false
        }
        let mut tasks: Vec<usize> = (0..self.t).collect();
        if let Some(k) = key { let o = self.t * self.a; tasks.sort_by(|&u, &v| k[o + u].partial_cmp(&k[o + v]).unwrap()); }
        for &i in &tasks { let mut seen = vec![false; self.a];
            // j re-pushed itself; remove stale entry handled by position replace
            if !aug(self, i, &mut seen, &mut x, &mut on, key) { return None; } }
        // repair duplicates from recursive re-push (j appears on new agent; old slot replaced by i)
        Some(x)
    }
}

// ---------------- exact enumeration ----------------
pub struct Exact { pub logz: f64, pub marg: Vec<f64>, pub top: Vec<(f64, Vec<usize>)>, pub n_feasible: u64 }

pub fn exact(p: &Problem, k: usize, limit: u64) -> Option<Exact> { exact_within(p, k, limit, None) }
/// `exact` that declines (None) once `hard` passes (clock read every 64 nodes, both passes): the opt-in `--exact-ms`
/// cap of the CLI. None = `exact`.
pub fn exact_within(p: &Problem, k: usize, limit: u64, hard: Option<probbit_core::rt::Instant>) -> Option<Exact> {
    probbit_ir::deep(p.t, || enumerate(p, k, limit, hard, MEMO_MAX)) // the DFS recurses once per task; big inputs get their own stack
}
/// `exact` with the enumeration's affinity memo capped at `memo_max` entries (tests: the past-the-chunk fold is bit-identical).
#[doc(hidden)]
pub fn exact_memo_capped(p: &Problem, k: usize, limit: u64, memo_max: usize) -> Option<Exact> {
    probbit_ir::deep(p.t, || enumerate(p, k, limit, None, memo_max))
}
/// Cap on the enumeration's affinity memo (f64 entries, 128 MB); larger programs get shorter chunks per (task, worker).
const MEMO_MAX: usize = 1 << 24;
fn enumerate(p: &Problem, k: usize, limit: u64, hard: Option<probbit_core::rt::Instant>, memo_max: usize) -> Option<Exact> {
    if hard.is_some_and(|h| probbit_core::rt::Instant::now() >= h) { return None; }
    // dense group index per task (usize::MAX = ungrouped: no mates, so no affinity term)
    let mut ids: std::collections::HashMap<usize, usize> = std::collections::HashMap::new();
    let gi: Vec<usize> = p.group.iter().map(|&g| if g == usize::MAX { usize::MAX } else { let n = ids.len(); *ids.entry(g).or_insert(n) }).collect();
    // memo chunk per (task, worker): c <= min(mates before i, cap - 1); one flat zeroed arena (pages resident only once
    // touched), at most MEMO_MAX entries: past a chunk's end the fold continues from its last entry (same bits)
    let mut seen = vec![0usize; ids.len()]; let mut full = vec![0usize; p.t * p.a];
    for i in 0..p.t { if gi[i] == usize::MAX { continue; } let b = seen[gi[i]]; seen[gi[i]] += 1;
        for a in 0..p.a { if p.ok(i, a) { full[i * p.a + a] = b.min(p.cap[a].saturating_sub(1)); } } }
    let total: usize = full.iter().sum(); let lim = if total <= memo_max { usize::MAX } else { memo_max / (p.t * p.a).max(1) };
    let (mut moff, mut off) = (vec![0u32; p.t * p.a], 0usize);
    let mcap: Vec<u32> = full.iter().enumerate().map(|(k, &f)| { let c = f.min(lim); moff[k] = off as u32; off += c; c as u32 }).collect();
    let mut st = Ex { p, gi, cnt: vec![0; ids.len() * p.a], memo: vec![0.0; off], moff, mcap, mlen: vec![0; p.t * p.a], x: vec![0; p.t], load: vec![0; p.a], mx: f64::NEG_INFINITY, sum: 0.0,
        marg: vec![0.0; p.t * p.a], top: vec![], k, n: 0, limit, pass: 0, nodes: 0, node_limit: limit.saturating_mul(64).max(1_000_000), hard, cut: false, stop: 0 };
    st.stop = st.arm();
    // pass 0: find max logw (for stable exp) & count; pass 1: accumulate
    // Also bound INTERNAL nodes. With limit 0 the probe used to backtrack exponentially on large tight
    // instances before reaching its first leaf (T=1000 x 10 agents, saturated legal capacity: hung > 2 min).
    st.dfs(0, 0.0); if st.cut || st.n > limit || st.nodes > st.node_limit { return None; }
    if st.n == 0 { return Some(Exact { logz: f64::NEG_INFINITY, marg: vec![0.0; p.t * p.a], top: vec![], n_feasible: 0 }); }
    st.pass = 1; st.n = 0; st.nodes = 0; st.stop = st.arm(); st.dfs(0, 0.0); // pass 1 restarts the node count (it used to inherit pass 0's and cut short)
    if st.cut { return None; } // a pass cut by the hard stop is a partial sum, not an answer
    let z = st.sum; for m in st.marg.iter_mut() { *m /= z; }
    let logz = st.mx + z.ln();
    let top = st.top.into_iter().map(|(lw, x)| ((lw - logz).exp(), x)).collect();
    Some(Exact { logz, marg: st.marg, top, n_feasible: st.n })
}
/// R19.9 (finding 1): the affinity term of task i on worker a was a scan of ALL of i's group mates per node
/// (`d = h[i][a]; for j in mates[i] { if j < i && x[j] == a { d += lam } }`: O(group size); 199.9 s on a near-saturated
/// 3,000-task group). The DFS assigns tasks in index order, so the mates j < i are exactly the assigned ones and the scan
/// adds lam c times, c = assigned mates on a. Now `cnt` keeps c per (group, worker) (push / pop with the DFS) and `memo`
/// holds the SAME sequential fold per (task, worker): f(c) = fl(..fl(fl(h + lam) + lam)..) (c adds), filled lazily into one
/// flat arena (`moff` chunk start, `mcap` chunk length, `mlen` entries filled; a Vec per pair cost +423 MB peak RSS on the
/// 3,000-task group from growth slack and freed fragments). Every addend is the same lam, so the value depends only on c,
/// never on mate order: bit-identical sums.
struct Ex<'a> { p: &'a Problem, gi: Vec<usize>, cnt: Vec<usize>, memo: Vec<f64>, moff: Vec<u32>, mcap: Vec<u32>, mlen: Vec<u32>, x: Vec<usize>, load: Vec<usize>, mx: f64, sum: f64, marg: Vec<f64>,
    top: Vec<(f64, Vec<usize>)>, k: usize, n: u64, limit: u64, pass: u8, nodes: u64, node_limit: u64,
    /// Hard stop (`exact_within`), whether it fired, and the node count that triggers the next check (`halt`): the node
    /// limit without a hard stop, so the default path keeps its single comparison (a separate clock branch cost +2.0%)
    hard: Option<probbit_core::rt::Instant>, cut: bool, stop: u64 }
impl<'a> Ex<'a> {
    fn arm(&self) -> u64 { if self.hard.is_some() { self.node_limit.min(self.nodes + 63) } else { self.node_limit } }
    /// The real stop conditions, reached only past `stop`: plan / node limit, a cut, the hard stop (clock every 64 nodes).
    #[cold]
    fn halt(&mut self) -> bool {
        if self.n > self.limit || self.nodes > self.node_limit || self.cut { return true; }
        if self.hard.is_some_and(|h| probbit_core::rt::Instant::now() >= h) { self.cut = true; return true; }
        self.stop = self.arm(); false
    }
    fn dfs(&mut self, i: usize, lw: f64) {
        if (self.n > self.limit || self.nodes > self.stop) && self.halt() { return; }
        self.nodes += 1; let p = self.p;
        if i == p.t {
            self.n += 1;
            if self.pass == 0 { if lw > self.mx { self.mx = lw; } return; }
            let w = (lw - self.mx).exp(); self.sum += w;
            for j in 0..p.t { self.marg[j * p.a + self.x[j]] += w; }
            if self.top.len() < self.k || lw > self.top.last().unwrap().0 {
                self.top.push((lw, self.x.clone())); self.top.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap()); self.top.truncate(self.k); }
            return;
        }
        let g = self.gi[i];
        for a in 0..p.a {
            if !p.ok(i, a) || self.load[a] >= p.cap[a] { continue; }
            let c = if g == usize::MAX { 0 } else { self.cnt[g * p.a + a] };
            let k = i * p.a + a; let len = self.mlen[k] as usize;
            let d = if c == 0 { p.h[k] } else if c <= len { self.memo[self.moff[k] as usize + c - 1] } else {
                let (off, upto) = (self.moff[k] as usize, c.min(self.mcap[k] as usize));
                let mut v = if len == 0 { p.h[k] } else { self.memo[off + len - 1] };
                for j in len..upto { v += p.lam; self.memo[off + j] = v; }
                if upto > len { self.mlen[k] = upto as u32; }
                for _ in upto.max(len)..c { v += p.lam; } // past the chunk: the same fold, not stored
                v };
            self.x[i] = a; self.load[a] += 1; if g != usize::MAX { self.cnt[g * p.a + a] += 1; }
            self.dfs(i + 1, lw + d);
            self.load[a] -= 1; if g != usize::MAX { self.cnt[g * p.a + a] -= 1; }
        }
    }
}

// ---------------- constraint-preserving Gibbs ----------------
/// Chains start from an over-dispersed random feasible state (default true). Set false only to reproduce earlier numbers from the deterministic best-logit start (`feasible_init`).
pub static DISPERSED_INIT: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(true);
pub struct Chain<'a> { pub p: &'a Problem, mates: Vec<Vec<usize>>, /// allowed agents per task (sparse site update)
    cand_of: Vec<Vec<usize>>, groups: Vec<Vec<usize>>, cand: Vec<(f64, Vec<usize>)>, pub x: Vec<usize>, load: Vec<usize>, rng: Philox4x32, w: Vec<f64>,
    /// Tables for the exact two-group joint heat-bath (None = move disabled)
    gp: Option<GroupPairs>,
    /// Extra heat-bath passes per sweep over these tasks ("focus" = tickets whose error bar is still wide).
    /// Each site update leaves pi invariant, so any fixed focus set keeps the chain exact.
    pub focus: Vec<usize>, pub focus_reps: usize,
    /// Tables + window size + moves per sweep for the frontier-DP k-group exact block heat-bath (None = off)
    win: Option<GroupPairs>, win_k: usize, win_reps: usize,
    /// The global flip's precomputed structure (`Problem::collective`; built on first use)
    flip: Option<Box<RouterFlip>>,
    /// relabel scratch (`Problem::collective`, > 2 workers): per task a changed flag, the changed tasks
    rl: (Vec<bool>, Vec<usize>),
    /// see `probbit_ir::Chain::moves` (same counts on the same draws: the lowering identity test checks them)
    pub moves: [u64; 2] }
/// `probbit_ir`'s FlipPlan for the router: (task, first worker, second worker, h[second] - h[first]) per free two-worker task,
/// mate pairs with one flipped endpoint (task, mate), mate pairs of flipped tasks whose affinity term can change
/// (vars index of i, of j, change by [s_i * 2 + s_j]), and a per-worker load-change scratch.
struct RouterFlip { vars: Vec<(usize, usize, usize, f64)>, pos: Vec<usize>, one: Vec<(usize, usize)>, both: Vec<(usize, usize, [f64; 4])>, dl: Vec<i64> }
impl<'a> Chain<'a> {
    pub fn new(p: &'a Problem, seed: u64, stream: u64) -> Option<Self> {
        let mut rng = Philox4x32::new(seed, stream);
        let x = if DISPERSED_INIT.load(std::sync::atomic::Ordering::Relaxed) { p.feasible_init_rand(&mut rng)? } else { p.feasible_init()? };
        let mut load = vec![0; p.a]; for &a in &x { load[a] += 1; }
        let mut gm: std::collections::BTreeMap<usize, Vec<usize>> = Default::default();
        for i in 0..p.t { if p.group[i] != usize::MAX { gm.entry(p.group[i]).or_default().push(i); } }
        let cand_of = (0..p.t).map(|i| (0..p.a).filter(|&a| p.allowed[i * p.a + a]).collect()).collect();
        let mut c = Chain { p, mates: p.mates(), cand_of, groups: gm.into_values().collect(), cand: vec![], x, load, rng, w: vec![f64::NEG_INFINITY; p.a], gp: None, focus: vec![], focus_reps: 0, win: None, win_k: 0, win_reps: 0, flip: None, rl: (vec![false; p.t], vec![]), moves: [0; 2] };
        // randomise start: a few random feasible sweeps at beta=0 (h ignored) for over-dispersion
        for _ in 0..5 { c.sweep_uniform(); }
        Some(c)
    }
    fn sweep_uniform(&mut self) {
        let p = self.p;
        for i in 0..p.t { let a0 = self.x[i]; let mut n = 0;
            for a in 0..p.a { if p.ok(i, a) && (a == a0 || self.load[a] < p.cap[a]) { n += 1; } }
            let mut r = self.rng.below(n);
            for a in 0..p.a { if p.ok(i, a) && (a == a0 || self.load[a] < p.cap[a]) { if r == 0 { self.load[a0] -= 1; self.load[a] += 1; self.x[i] = a; break; } r -= 1; } } }
    }
    /// Heat-bath over the feasible values of task i. Iterates only the task's allowed agents (bit-identical to the
    /// dense loop: disallowed agents had weight exactly 0 and never changed mx, tot or the pick sequence).
    #[inline]
    fn site(&mut self, i: usize) {
        let p = self.p; let na = p.a; let a0 = self.x[i]; let cand = &self.cand_of[i];
        let mut mx = f64::NEG_INFINITY;
        for &a in cand { self.w[a] = if p.ok(i, a) && (a == a0 || self.load[a] < p.cap[a]) { p.h[i * na + a] } else { f64::NEG_INFINITY }; }
        // w[b] of a non-candidate b may hold a stale value from another task; it is never read below (loops use cand)
        for &j in &self.mates[i] { let b = self.x[j]; if p.allowed[i * na + b] && self.w[b] > f64::NEG_INFINITY { self.w[b] += p.lam; } }
        for &a in cand { if self.w[a] > mx { mx = self.w[a]; } }
        let mut tot = 0.0; for &a in cand { let v = if self.w[a] == f64::NEG_INFINITY { 0.0 } else { (self.w[a] - mx).exp() }; self.w[a] = v; tot += v; }
        let mut r = self.rng.f64() * tot; let mut pick = a0;
        for &a in cand { if self.w[a] > 0.0 { pick = a; if r < self.w[a] { break; } r -= self.w[a]; } }
        self.load[a0] -= 1; self.load[pick] += 1; self.x[i] = pick;
    }
    /// The exact two-group joint heat-bath tables (`GroupPairs::new`; None = move off), as `sample_on` sets them when its
    /// `group_pairs` argument is on: for callers that step chains themselves (the CLI's `demo --live`).
    pub fn set_group_pairs(&mut self, gp: Option<GroupPairs>) { self.gp = gp; }
    /// O(T * group size) log-weight of the current state
    pub fn logw(&self) -> f64 { let p = self.p; let mut e = 0.0; for i in 0..p.t { e += p.h[i * p.a + self.x[i]]; for &j in &self.mates[i] { if j > i && self.x[j] == self.x[i] { e += p.lam; } } } e }
    fn pair_e(&self, i: usize, a: usize, skip: usize) -> f64 {
        let mut e = self.p.h[i * self.p.a + a];
        for &j in &self.mates[i] { if j != skip && self.x[j] == a { e += self.p.lam; } } e
    }
    #[inline]
    fn swap(&mut self) {
        let p = self.p; let i = self.rng.below(p.t); let j = self.rng.below(p.t);
        let (ai, aj) = (self.x[i], self.x[j]);
        if i == j || ai == aj || !p.ok(i, aj) || !p.ok(j, ai) { return; }
        let e0 = self.pair_e(i, ai, j) + self.pair_e(j, aj, i);
        let e1 = self.pair_e(i, aj, j) + self.pair_e(j, ai, i);
        // mates i<->j same group: pair term (i,j) same-agent status unchanged by a swap (both differ before & after)
        if e1 >= e0 || self.rng.f64() < (e1 - e0).exp() { self.x[i] = aj; self.x[j] = ai; }
    }
    /// one sweep = T site updates + T/2 swap attempts
    pub fn sweep(&mut self) {
        for i in 0..self.p.t { self.site(i); } for _ in 0..(self.p.t / 2).max(2) { self.swap(); }
        if self.p.collective { self.global_flip(); if self.p.a > 2 { self.relabel(); } }
        if self.p.cluster && self.rng.f64() < 0.5 { self.cluster_move(); } // see probbit_ir::Model::cluster
        if self.p.cycles && self.p.a > 2 { for _ in 0..(self.p.t / 4).max(1) { self.cycle3(); } } // as the IR: k > 2 and caps (every lowered router has a cap per agent)
        if self.p.block_moves { for g in 0..self.groups.len() { self.block(g); } }
        if self.p.pair_swaps && self.groups.len() > 1 { for _ in 0..self.groups.len() { self.pair_swap(); } }
        if self.gp.is_some() { for _ in 0..self.groups.len() { self.group_pair(); } }
        if self.win.is_some() { for _ in 0..self.win_reps { self.window_move(); } }
        for _ in 0..self.focus_reps { for k in 0..self.focus.len() { let i = self.focus[k]; self.site(i); } }
    }
    /// The router's global two-value flip (`Problem::collective`): `probbit_ir::Chain::global_flip` on this crate's state, with
    /// the same terms summed in the same order (unary, one-flipped-endpoint mates, flipped mate pairs) and the same single
    /// uniform draw, so it is bit-identical through `Problem::lower`. Symmetric involution + MH = detailed balance.
    fn global_flip(&mut self) {
        if self.flip.is_none() { self.flip = Some(Box::new(self.flip_plan())); }
        let p = self.p; let mut f = self.flip.take().unwrap();
        if !f.vars.is_empty() {
            for &(i, c0, c1, _) in &f.vars { let (a, b) = if self.x[i] == c0 { (c0, c1) } else { (c1, c0) }; f.dl[a] -= 1; f.dl[b] += 1; }
            if (0..p.a).all(|a| self.load[a] as i64 + f.dl[a] <= p.cap[a] as i64) {
                let mut d = 0.0;
                for &(i, c0, _, dh) in &f.vars { d += if self.x[i] == c0 { dh } else { -dh }; }
                for &(i, j) in &f.one { let (xi, xj) = (self.x[i], self.x[j]); let v = &f.vars[f.pos[i]]; let yi = xi ^ v.1 ^ v.2;
                    if xj == yi { d += p.lam; } else if xj == xi { d -= p.lam; } }
                for &(a, b, t) in &f.both { let (va, vb) = (&f.vars[a], &f.vars[b]); d += t[(self.x[va.0] != va.1) as usize * 2 + (self.x[vb.0] != vb.1) as usize]; }
                if d >= 0.0 || self.rng.f64() < d.exp() {
                    for &(i, c0, c1, _) in &f.vars { self.x[i] ^= c0 ^ c1; } self.moves[0] += 1;
                    for a in 0..p.a { self.load[a] = (self.load[a] as i64 + f.dl[a]) as usize; }
                }
            }
            for v in f.dl.iter_mut() { *v = 0; }
        }
        self.flip = Some(f);
    }
    /// The router's label swap (`Problem::collective`, > 2 workers): `probbit_ir::Chain::relabel` on this crate's state (the
    /// same two draws, the same changed set, the same sums in the same order), bit-identical through `Problem::lower`.
    /// Mate pairs that both change keep their equality, so only unary terms and one-changed-endpoint mates enter dlogw.
    fn relabel(&mut self) {
        let p = self.p; let na = p.a; let a = self.rng.below(na); let mut b = self.rng.below(na - 1); if b >= a { b += 1; }
        let (mut on, mut ch) = std::mem::take(&mut self.rl);
        for i in 0..p.t { let v = self.x[i]; if (v == a || v == b) && p.clamp[i].is_none() && p.allowed[i * na + a] && p.allowed[i * na + b] { on[i] = true; ch.push(i); } }
        if !ch.is_empty() {
            let from_a = ch.iter().filter(|&&i| self.x[i] == a).count() as i64; let from_b = ch.len() as i64 - from_a;
            if self.load[a] as i64 + from_b - from_a <= p.cap[a] as i64 && self.load[b] as i64 + from_a - from_b <= p.cap[b] as i64 {
                let mut d = 0.0;
                for &i in &ch { let (x, y) = (self.x[i], self.x[i] ^ a ^ b); d += p.h[i * na + y] - p.h[i * na + x]; }
                for &i in &ch { let (xi, yi) = (self.x[i], self.x[i] ^ a ^ b);
                    for &j in &self.mates[i] { if !on[j] { let xj = self.x[j]; if xj == yi { d += p.lam; } else if xj == xi { d -= p.lam; } } } }
                if d >= 0.0 || self.rng.f64() < d.exp() {
                    for &i in &ch { self.x[i] ^= a ^ b; } self.moves[1] += 1;
                    self.load[a] = (self.load[a] as i64 + from_b - from_a) as usize; self.load[b] = (self.load[b] as i64 + from_a - from_b) as usize; }
            }
            for &i in &ch { on[i] = false; } ch.clear();
        }
        self.rl = (on, ch);
    }
    /// The router's Wolff cluster move (`Problem::cluster`): `probbit_ir::Chain::cluster_move` on this crate's state (the same
    /// draws in the same order, the same sums), bit-identical through `Problem::lower`. Bonds = same-group mates when lam > 0.
    fn cluster_move(&mut self) {
        let p = self.p; let na = p.a; if na < 2 { return; }
        let s = self.rng.below(p.t); let old = self.x[s]; let mut v = self.rng.below(na - 1); if v >= old { v += 1; }
        let free = |i: usize, val: usize| p.clamp[i].is_none() && p.allowed[i * na + val];
        if !free(s, v) { return; }
        let (mut inc, mut cl) = std::mem::take(&mut self.rl); let w = p.lam;
        inc[s] = true; cl.push(s); let mut head = 0;
        while head < cl.len() { let i = cl[head]; head += 1;
            for &j in &self.mates[i] { if w > 0.0 && !inc[j] && self.x[j] == old && free(j, v) && self.rng.f64() < -(-w).exp_m1() { inc[j] = true; cl.push(j); } } }
        let nc = cl.len();
        if self.load[v] + nc <= p.cap[v] {
            let mut d = 0.0;
            for &i in &cl { d += p.h[i * na + v] - p.h[i * na + old];
                for &j in &self.mates[i] { if inc[j] { continue; } let xj = self.x[j];
                    let cancelled = w > 0.0 && ((xj == old && free(j, v)) || (xj == v && free(j, old)));
                    if !cancelled { if xj == v { d += w; } else if xj == old { d -= w; } } } }
            if d >= 0.0 || self.rng.f64() < d.exp() { for &i in &cl { self.x[i] = v; } self.load[old] -= nc; self.load[v] += nc; }
        }
        for &i in &cl { inc[i] = false; } cl.clear();
        self.rl = (inc, cl);
    }
    /// The router's three-cycle rotation (`Problem::cycles`): `probbit_ir::Chain::cycle3` on this crate's state (same three
    /// draws, same local sums in the same order), bit-identical through `Problem::lower`. Worker loads are unchanged.
    fn cycle3(&mut self) {
        let p = self.p; if p.t < 3 { return; }
        let (i, j, l) = (self.rng.below(p.t), self.rng.below(p.t), self.rng.below(p.t));
        if i == j || j == l || i == l { return; }
        let (a, b, c) = (self.x[i], self.x[j], self.x[l]);
        if a == b || b == c || a == c || !p.ok(i, b) || !p.ok(j, c) || !p.ok(l, a) { return; }
        let mates = &self.mates;
        let local = |x: &[usize]| { let mut e = 0.0; for &v in &[i, j, l] { e += p.h[v * p.a + x[v]];
            for &o in &mates[v] { if ((o != i && o != j && o != l) || o > v) && x[o] == x[v] { e += p.lam; } } } e };
        let e0 = local(&self.x); let mut y = std::mem::take(&mut self.x); y[i] = b; y[j] = c; y[l] = a; let e1 = local(&y);
        if !(e1 >= e0 || self.rng.f64() < (e1 - e0).exp()) { y[i] = a; y[j] = b; y[l] = c; }
        self.x = y;
    }
    fn flip_plan(&self) -> RouterFlip {
        let p = self.p; let na = p.a;
        let vars: Vec<(usize, usize, usize, f64)> = (0..p.t).filter(|&i| p.clamp[i].is_none() && self.cand_of[i].len() == 2)
            .map(|i| { let (c0, c1) = (self.cand_of[i][0], self.cand_of[i][1]); (i, c0, c1, p.h[i * na + c1] - p.h[i * na + c0]) }).collect();
        let mut pos = vec![usize::MAX; p.t]; for (q, v) in vars.iter().enumerate() { pos[v.0] = q; }
        let (mut one, mut both) = (vec![], vec![]);
        for (a, &(i, c0, c1, _)) in vars.iter().enumerate() { for &j in &self.mates[i] {
            if pos[j] == usize::MAX { one.push((i, j)); continue; }
            if j < i { continue; }
            let (d0, d1) = (vars[pos[j]].1, vars[pos[j]].2); let mut t = [0.0; 4];
            for (si, &(xi, yi)) in [(c0, c1), (c1, c0)].iter().enumerate() { for (sj, &(xj, yj)) in [(d0, d1), (d1, d0)].iter().enumerate() {
                t[si * 2 + sj] = (if yi == yj { p.lam } else { 0.0 }) - (if xi == xj { p.lam } else { 0.0 }); } }
            if t.iter().any(|&v| v != 0.0) { both.push((a, pos[j], t)); } } }
        RouterFlip { vars, pos, one, both, dl: vec![0; na] }
    }
    /// Reversible because we require g1 has no member on b and g2 none on a (so the reverse picks the same sets);
    /// proposal prob |S1|/|g1| * |S2|/|g2| is identical in both directions.
    fn pair_swap(&mut self) {
        let p = self.p; let ng = self.groups.len();
        let g1 = self.rng.below(ng); let g2 = self.rng.below(ng); if g1 == g2 { return; }
        let (m1, m2) = (&self.groups[g1], &self.groups[g2]);
        let a = self.x[m1[self.rng.below(m1.len())]]; let b = self.x[m2[self.rng.below(m2.len())]]; if a == b { return; }
        if m1.iter().any(|&i| self.x[i] == b) || m2.iter().any(|&i| self.x[i] == a) { return; }
        let s1: Vec<usize> = m1.iter().copied().filter(|&i| self.x[i] == a).collect();
        let s2: Vec<usize> = m2.iter().copied().filter(|&i| self.x[i] == b).collect();
        if self.load[b] + s1.len() - s2.len() > p.cap[b] || self.load[a] + s2.len() - s1.len() > p.cap[a] { return; }
        if s1.iter().any(|&i| !p.ok(i, b)) || s2.iter().any(|&i| !p.ok(i, a)) { return; }
        let e0 = self.logw();
        for &i in &s1 { self.x[i] = b; } for &i in &s2 { self.x[i] = a; }
        let e1 = self.logw();
        if e1 >= e0 || self.rng.f64() < (e1 - e0).exp() {
            self.load[a] = self.load[a] + s2.len() - s1.len(); self.load[b] = self.load[b] + s1.len() - s2.len();
        } else { for &i in &s1 { self.x[i] = a; } for &i in &s2 { self.x[i] = b; } }
    }
    /// Exact heat-bath of a whole group given everything else (valid Gibbs block update; preserves feasibility).
    fn block(&mut self, g: usize) {
        let mem = std::mem::take(&mut self.groups[g]); let p = self.p;
        for &i in &mem { self.load[self.x[i]] -= 1; }
        self.cand.clear(); let mut cur = vec![0usize; mem.len()];
        fn rec(p: &Problem, mem: &[usize], k: usize, lw: f64, cur: &mut Vec<usize>, load: &mut [usize], out: &mut Vec<(f64, Vec<usize>)>) {
            if k == mem.len() { out.push((lw, cur.clone())); return; }
            let i = mem[k];
            for a in 0..p.a { if !p.ok(i, a) || load[a] >= p.cap[a] { continue; }
                let mut d = p.h[i * p.a + a]; for q in 0..k { if cur[q] == a { d += p.lam; } }
                cur[k] = a; load[a] += 1; rec(p, mem, k + 1, lw + d, cur, load, out); load[a] -= 1; }
        }
        rec(p, &mem, 0, 0.0, &mut cur, &mut self.load, &mut self.cand);
        let mx = self.cand.iter().map(|c| c.0).fold(f64::NEG_INFINITY, f64::max);
        let tot: f64 = self.cand.iter().map(|c| (c.0 - mx).exp()).sum();
        let mut r = self.rng.f64() * tot; let mut pick = self.cand.len() - 1;
        for (k, c) in self.cand.iter().enumerate() { let w = (c.0 - mx).exp(); if r < w { pick = k; break; } r -= w; }
        let asg = self.cand[pick].1.clone();
        for (q, &i) in mem.iter().enumerate() { self.x[i] = asg[q]; self.load[asg[q]] += 1; }
        self.groups[g] = mem;
    }
}

/// The sample container is the IR's (same fields); re-exported so front-end and IR runs feed one gate.
pub use probbit_ir::Samples;

/// Run `chains` chains, each for `sweeps` sweeps (or until `budget_ms` wall-clock), optionally on threads.
pub fn sample(p: &Problem, chains: usize, sweeps: usize, budget_ms: Option<f64>, seed: u64, threads: bool, keep_plans: bool) -> Option<Samples> {
    sample_opts(p, chains, sweeps, budget_ms, seed, threads, keep_plans, false)
}
/// `sample` plus options: `group_pairs` enables the exact two-group joint heat-bath move (`GroupPairs`).
#[allow(clippy::too_many_arguments)]
pub fn sample_opts(p: &Problem, chains: usize, sweeps: usize, budget_ms: Option<f64>, seed: u64, threads: bool, keep_plans: bool, group_pairs: bool) -> Option<Samples> {
    let gpt = if group_pairs { GroupPairs::new(p) } else { None };
    let wt = window_tables(p);
    let run = |c: usize| chain_samples(p, c, sweeps, budget_ms.map(|b| probbit_core::rt::Instant::now() + std::time::Duration::from_secs_f64(b.max(0.0) / 1e3)), seed, keep_plans, &gpt, &wt, 1.0, 0);
    let parts: Vec<Samples> = if threads {
        std::thread::scope(|sc| { let hs: Vec<_> = (0..chains).map(|c| { let run = &run; sc.spawn(move || run(c)) }).collect(); hs.into_iter().map(|h| h.join().unwrap()).collect::<Option<Vec<_>>>() })?
    } else { (0..chains).map(run).collect::<Option<Vec<_>>>()? };
    Some(probbit_ir::merge(p.t * p.a, parts))
}
/// One router chain (stream `c`): a pure function of (problem, seed, c, sweeps) when `budget_ms` is None. `duty` < 1
/// (CPU limit, as `probbit_ir`): after every ~2 ms of sweeping the thread sleeps busy * (1/duty - 1); samples are unchanged.
#[allow(clippy::too_many_arguments)]
fn chain_samples(p: &Problem, c: usize, sweeps: usize, deadline: Option<probbit_core::rt::Instant>, seed: u64, keep_plans: bool, gpt: &Option<GroupPairs>, wt: &Option<GroupPairs>, duty: f64, max_rows: usize) -> Option<Samples> {
    // R19.9: a wall-clock run stops at `deadline` (set before the chain is built: the build and the start count)
    let mut ch = Chain::new(p, seed, c as u64)?; ch.gp = gpt.clone(); ch.set_window(wt); let na = p.a;
    let burn = sweeps / 10; let mut busy0 = probbit_core::rt::Instant::now(); let mut stride = 1usize;
    let mut s = Samples { chain_marg: vec![], traj: vec![vec![]], marg: vec![0.0; p.t * na], n: 0, plans: HashMap::new(), trace: vec![vec![]], viol: 0, best: (f64::NEG_INFINITY, vec![]), sweeps: 0, moves: vec![] };
    let mut k = 0usize;
    loop {
        if let Some(d) = deadline { if k % 8 == 0 && probbit_core::rt::Instant::now() >= d { break; } } else if k >= sweeps { break; }
        ch.sweep(); k += 1; probbit_ir::progress_tick(k);
        if duty < 1.0 { let b = busy0.elapsed(); if b.as_secs_f64() >= 0.002 { std::thread::sleep(b.mul_f64(1.0 / duty - 1.0)); busy0 = probbit_core::rt::Instant::now(); } }
        let burn_now = if deadline.is_some() { k <= 20 } else { k <= burn };
        if burn_now { continue; }
        for i in 0..p.t { s.marg[i * na + ch.x[i]] += 1.0; }
        s.viol += (p.violations(&ch.x) > 0) as usize;
        let lw = ch.logw(); probbit_ir::record_row(&mut s, &ch.x, lw, &mut stride, max_rows); s.n += 1;
        if lw > s.best.0 { s.best = (lw, ch.x.clone()); }
        if keep_plans { *s.plans.entry(ch.x.iter().map(|&v| v as u8).collect()).or_insert(0) += 1; }
    }
    // As in probbit_ir::run_chain: no recorded row left `best` empty and the polish indexed an empty plan
    // (`probbit decide --budget-ms 0.01` on the 300-task demo aborted); the chain's state is feasible
    if s.n == 0 { s.best = (ch.logw(), ch.x.clone()); }
    s.sweeps = k; s.moves = vec![ch.moves]; Some(s)
}
/// Router resource controls (as `probbit_ir::sample_on`): `threads` worker threads (>= 1) run `chains` chains; worker w
/// runs chains w, w + threads, ... and results are pooled in chain order, so with fixed `sweeps` the answer is bit-identical
/// for every thread count (and to `sample_opts`). A wall-clock budget is the deadline of the whole call: each chain gets
/// budget / ceil(chains / threads). `cpu_pct` in 1..=100: per-thread duty cycle; fixed `sweeps` => same answer, only slower.
/// R19.9 (finding 2): the r-th chain of a worker stops at call start + (r + 1) x that slice, so a chain that overran its
/// slice (its build and start, or the 8-sweep clock stride) is charged to the worker's next chains instead of each chain
/// timing its own slice: 100,000 chains on a 200 ms budget sampled for ~460 ms. Same chains, same output shape; late
/// chains get fewer sweeps (possibly none past the build).
#[allow(clippy::too_many_arguments)]
/// `max_rows` > 0 (memory limit): at most that many trajectory rows per chain (`probbit_ir::record_row`); 0 = unbounded.
pub fn sample_on(p: &Problem, chains: usize, threads: usize, sweeps: usize, budget_ms: Option<f64>, seed: u64, keep_plans: bool, group_pairs: bool, cpu_pct: u32, max_rows: usize) -> Option<Samples> {
    let gpt = if group_pairs { GroupPairs::new(p) } else { None }; let wt = window_tables(p);
    let seq = probbit_core::rt::sequential(); let t = if seq { 1 } else { threads.clamp(1, chains.max(1)) }; let rounds = chains.div_ceil(t).max(1); let b = budget_ms.map(|b| b / rounds as f64);
    let duty = cpu_pct.clamp(1, 100) as f64 / 100.0; let (gpt, wt) = (&gpt, &wt); let t_call = probbit_core::rt::Instant::now();
    let due = move |c: usize| b.map(|b| t_call + std::time::Duration::from_secs_f64(b.max(0.0) * (c / t + 1) as f64 / 1e3));
    let mut slots: Vec<Option<Samples>> = (0..chains).map(|_| None).collect();
    if seq { for (c, slot) in slots.iter_mut().enumerate() { *slot = chain_samples(p, c, sweeps, due(c), seed, keep_plans, gpt, wt, duty, max_rows); } } else {
    std::thread::scope(|sc| {
        let hs: Vec<_> = (0..t).map(|w| sc.spawn(move || (w..chains).step_by(t).map(|c| (c, chain_samples(p, c, sweeps, due(c), seed, keep_plans, gpt, wt, duty, max_rows))).collect::<Vec<_>>())).collect();
        for h in hs { for (c, s) in h.join().unwrap() { slots[c] = s; } }
    }); }
    Some(probbit_ir::merge(p.t * p.a, slots.into_iter().collect::<Option<Vec<_>>>()?))
}

/// Statistics and the certification gate live in the IR (`probbit_ir`); they work on any lowered program.
pub use probbit_ir::{split_rhat, mean_tv, max_tv, chain_disagreement, Gate, GateCfg, GATE, GATE_BS_POW, PARTIAL_RHAT, BATCH_RATIO_MAX, GATE_VERSION, GATE_ASSUMPTIONS};

// ---------------- the decision API ----------------
pub const RHAT_REFUSE: f64 = 1.05;
pub enum Verdict { Exact, DiagnosticsPassed { rhat: f64 }, Unmixed { rhat: f64 } }
pub struct Decision { pub verdict: Verdict, pub map: Vec<usize>, pub map_logw: f64, pub top: Vec<(f64, Vec<usize>)>, pub marg: Vec<f64>, pub ms: f64, pub samples: usize }

/// Router: exact if feasible set <= exact_limit, else 4-chain Gibbs with R-hat gate.
pub fn decide(p: &Problem, exact_limit: u64, sweeps: usize, seed: u64) -> Option<Decision> {
    let t0 = probbit_core::rt::Instant::now();
    if let Some(e) = exact(p, 5, exact_limit) {
        if e.n_feasible == 0 { return None; }
        let map = e.top[0].1.clone();
        return Some(Decision { verdict: Verdict::Exact, map_logw: p.logw(&map), map, top: e.top, marg: e.marg, ms: t0.elapsed().as_secs_f64() * 1e3, samples: 0 });
    }
    let s = sample(p, 4, sweeps, None, seed, true, true)?;
    let rhat = split_rhat(&s.trace);
    let mut plans: Vec<(u32, Vec<u8>)> = s.plans.into_iter().map(|(k, v)| (v, k)).collect();
    plans.sort_by(|a, b| b.0.cmp(&a.0));
    let top = plans.into_iter().take(5).map(|(c, k)| (c as f64 / s.n as f64, k.into_iter().map(|v| v as usize).collect())).collect();
    let verdict = if rhat < RHAT_REFUSE { Verdict::DiagnosticsPassed { rhat } } else { Verdict::Unmixed { rhat } };
    Some(Decision { verdict, map_logw: s.best.0, map: s.best.1, top, marg: s.marg, ms: t0.elapsed().as_secs_f64() * 1e3, samples: s.n })
}

// ---------------- Dispatch scenario generator ----------------
pub struct Dispatch { pub p: Problem, pub tickets: Vec<String>, pub agents: Vec<String> }
/// Stub judge: logits = skill match + agent seniority + noise (seeded); groups = same-customer tickets (affinity lam>0).
pub fn dispatch(t: usize, a: usize, cap: usize, group_size: usize, lam: f64, seed: u64) -> Dispatch {
    let mut r = Philox4x32::new(seed, 999);
    #[allow(clippy::approx_constant)] // 6.283185307, not TAU: the golden digests of the acceptance tests depend on these exact bits
    let mut g = || { let u = r.f64() + 1e-12; let v = r.f64(); (-2.0 * u.ln()).sqrt() * (6.283185307 * v).cos() };
    let skills = ["billing", "tech", "legal"];
    let agent_skill: Vec<usize> = (0..a).map(|k| k % 3).collect();
    let mut h = vec![0.0; t * a]; let mut allowed = vec![true; t * a];
    let mut tickets = vec![];
    for i in 0..t {
        let need = (i / group_size.max(1)) % 3;
        tickets.push(format!("T{:02}[{}]", i, skills[need]));
        for k in 0..a {
            h[i * a + k] = 1.0 * g() + if agent_skill[k] == need { 1.5 } else { 0.0 } + if k == 0 { 1.2 } else { 0.0 };
            // legal tickets may only go to legal-skilled agents (hard skill rule)
            if need == 2 && agent_skill[k] != 2 { allowed[i * a + k] = false; }
        }
    }
    let agents = (0..a).map(|k| format!("A{}({}{})", k, skills[agent_skill[k]], if k == 0 { ",star" } else { "" })).collect();
    let group = (0..t).map(|i| if group_size > 1 { i / group_size } else { usize::MAX }).collect();
    Dispatch { p: Problem { t, a, h, allowed, cap: vec![cap; a], group, lam, clamp: vec![None; t], block_moves: false, pair_swaps: false, collective: false, cluster: false, cycles: false }, tickets, agents }
}

/// Per-question independent softmax marginals, ignoring rules.
pub fn independent(p: &Problem) -> Vec<f64> {
    let mut m = vec![0.0; p.t * p.a];
    for i in 0..p.t { let mx = (0..p.a).map(|k| p.h[i * p.a + k]).fold(f64::NEG_INFINITY, f64::max);
        let z: f64 = (0..p.a).map(|k| (p.h[i * p.a + k] - mx).exp()).sum();
        for k in 0..p.a { m[i * p.a + k] = (p.h[i * p.a + k] - mx).exp() / z; } }
    m
}
pub fn argmax_plan(p: &Problem) -> Vec<usize> {
    (0..p.t).map(|i| (0..p.a).max_by(|&u, &v| p.h[i * p.a + u].partial_cmp(&p.h[i * p.a + v]).unwrap()).unwrap()).collect()
}

// ---------------- certification gate: runs on the IR through the lowering ----------------
/// The gate over this problem's marginals: `probbit_ir::gate_stats` on `self.lower()` (bit-identical to the earlier
/// Problem-specific gate; acceptance `ir_lowering_bit_identical` pins golden digests).
pub fn gate_stats(p: &Problem, s: &Samples) -> Gate { probbit_ir::gate_stats(&p.lower(), s) }
/// see `probbit_ir::rhat_tasks`
pub fn rhat_tasks(p: &Problem, s: &Samples) -> Vec<f64> { probbit_ir::rhat_tasks(&p.lower(), s) }
/// see `probbit_ir::frozen_saturated` (capacity constraint = agent)
pub fn frozen_saturated(p: &Problem, s: &Samples) -> usize { probbit_ir::frozen_saturated(&p.lower(), s) }
/// see `probbit_ir::gate_stats_with`
pub fn gate_stats_with(p: &Problem, s: &Samples, bs_pow: f64) -> Gate { probbit_ir::gate_stats_with(&p.lower(), s, bs_pow) }
/// Router with the certification gate: exact if feasible set <= exact_limit; else 4 chains (fixed sweeps or wall-clock budget)
/// and certify only if the per-marginal error bound passes.
/// With exact_limit > 0 a second exact tier runs before sampling: `exact_frontier` (thin sharing: exact odds + exact MAP
/// at any size; declines in <= ~10 ms on thick queues). exact_limit = 0 still forces the sampler.
pub fn decide_gated(p: &Problem, exact_limit: u64, sweeps: usize, budget_ms: Option<f64>, seed: u64, cfg: &GateCfg) -> Option<(Decision, Option<Gate>)> {
    let t0 = probbit_core::rt::Instant::now();
    if let Some(e) = exact(p, 5, exact_limit) {
        if e.n_feasible == 0 { return None; }
        let map = e.top[0].1.clone();
        return Some((Decision { verdict: Verdict::Exact, map_logw: p.logw(&map), map, top: e.top, marg: e.marg, ms: t0.elapsed().as_secs_f64() * 1e3, samples: 0 }, None));
    }
    if exact_limit > 0 { if let Some(f) = exact_frontier(p, FRONTIER_MAX_STATES) {
        return Some((Decision { verdict: Verdict::Exact, map_logw: f.map_logw, top: vec![((f.map_logw - f.logz).exp(), f.map.clone())], map: f.map, marg: f.marg, ms: t0.elapsed().as_secs_f64() * 1e3, samples: 0 }, None));
    } }
    let s = sample_on(p, 4, 4, sweeps, budget_ms, seed, false, auto_group_pairs(p), 100, 0)?;
    let g = gate_stats(p, &s);
    let verdict = if g.diagnostics_passed(cfg) { Verdict::DiagnosticsPassed { rhat: g.rhat } } else { Verdict::Unmixed { rhat: g.rhat } };
    Some((Decision { verdict, map_logw: s.best.0, map: s.best.1, top: vec![], marg: s.marg, ms: t0.elapsed().as_secs_f64() * 1e3, samples: s.n }, Some(g)))
}

// ---------------- replica exchange over the affinity coupling (lambda ladder) ----------------
impl<'a> Chain<'a> {
    /// S(x) = number of same-group pairs on the same agent (the sufficient statistic of lam).
    pub fn affinity_pairs(&self) -> f64 { let mut s = 0.0; for i in 0..self.p.t { for &j in &self.mates[i] { if j > i && self.x[j] == self.x[i] { s += 1.0; } } } s }
    /// Exchange full states with another chain of the same constraint set (feasibility preserved).
    pub fn exchange(&mut self, o: &mut Chain<'_>) { std::mem::swap(&mut self.x, &mut o.x); std::mem::swap(&mut self.load, &mut o.load); }
}
/// 4 independent chains, each a ladder of K replicas at lam_k = p.lam * ladder[k] (ladder ascending, last = 1.0).
/// Every sweep: all replicas sweep, then adjacent pairs attempt exchange with prob min(1, exp((l_k - l_j)(S_j - S_k))).
/// Only the ladder-top replica (the real problem) is recorded. Output is a normal `Samples` (gate-able).
pub fn sample_tempered(p: &Problem, ladder: &[f64], budget_ms: f64, seed: u64) -> Option<Samples> {
    let probs: Vec<Problem> = ladder.iter().map(|&f| { let mut q = p.clone(); q.lam = p.lam * f; q }).collect();
    let run = |c: usize| -> Option<Samples> {
        let mut reps: Vec<Chain> = probs.iter().enumerate().map(|(k, q)| Chain::new(q, seed, (c * 64 + k) as u64)).collect::<Option<Vec<_>>>()?;
        let mut rng = probbit_core::Philox4x32::new(seed ^ 0xA5A5, 1000 + c as u64);
        let na = p.a; let kt = reps.len() - 1; let t0 = probbit_core::rt::Instant::now();
        let mut s = Samples { chain_marg: vec![], traj: vec![vec![]], marg: vec![0.0; p.t * na], n: 0, plans: HashMap::new(), trace: vec![vec![]], viol: 0, best: (f64::NEG_INFINITY, vec![]), sweeps: 0, moves: vec![] };
        let mut k = 0usize;
        loop {
            if k % 4 == 0 && t0.elapsed().as_secs_f64() * 1e3 >= budget_ms { break; }
            for r in reps.iter_mut() { r.sweep(); }
            for j in 0..kt { let (a, b) = reps.split_at_mut(j + 1); let (ra, rb) = (&mut a[j], &mut b[0]);
                let d = (probs[j].lam - probs[j + 1].lam) * (rb.affinity_pairs() - ra.affinity_pairs());
                if d >= 0.0 || rng.f64() < d.exp() { ra.exchange(rb); } }
            k += 1; if k <= 20 { continue; }
            let top = &reps[kt]; for i in 0..p.t { s.marg[i * na + top.x[i]] += 1.0; }
            s.n += 1; s.traj[0].extend(top.x.iter().map(|&v| v as u16)); s.viol += (p.violations(&top.x) > 0) as usize;
            let lw = p.logw(&top.x); s.trace[0].push(lw); if lw > s.best.0 { s.best = (lw, top.x.clone()); }
        }
        if s.n == 0 { let top = &reps[kt]; s.best = (p.logw(&top.x), top.x.clone()); } // never an empty best plan
        s.sweeps = k; Some(s)
    };
    let parts: Vec<Samples> = std::thread::scope(|sc| { let hs: Vec<_> = (0..4).map(|c| { let run = &run; sc.spawn(move || run(c)) }).collect(); hs.into_iter().map(|h| h.join().unwrap()).collect::<Option<Vec<_>>>() })?;
    let mut out = Samples { chain_marg: vec![], traj: vec![], marg: vec![0.0; p.t * p.a], n: 0, plans: HashMap::new(), trace: vec![], viol: 0, best: (f64::NEG_INFINITY, vec![]), sweeps: 0, moves: vec![] };
    for s in parts { for (m, v) in out.marg.iter_mut().zip(&s.marg) { *m += v; } out.chain_marg.push(s.marg.iter().map(|v| v / (s.n as f64).max(1.0)).collect()); out.n += s.n; out.viol += s.viol; out.sweeps += s.sweeps; out.moves.extend(s.moves.iter().copied());
        if s.best.0 > out.best.0 { out.best = s.best; } out.trace.push(s.trace.into_iter().next().unwrap()); out.traj.push(s.traj.into_iter().next().unwrap()); }
    let n = out.n as f64; for m in out.marg.iter_mut() { *m /= n.max(1.0); }
    Some(out)
}

// ---------------- anytime certification (persistent chains, re-gated every slice) ----------------
/// If > 0, after each look the anytime loop gives every not-yet-released ticket this many extra site updates per
/// sweep (certification-aware scheduling). Default 0 (off) — see examples/focus.rs for the measurement.
pub static ANYTIME_FOCUS_REPS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
/// Outcome of an anytime run: the decision, the gate at the stopping look, how many looks were taken and when the
/// stop rule first passed (None = hit the deadline without passing).
pub struct Anytime { pub decision: Decision, pub gate: Gate, pub samples: Samples, pub looks: usize,
    /// wall-clock ms at the look whose gate passed (`diagnostics_passed`); None = no look passed (renamed from `certified_at_ms`, R19)
    pub passed_at_ms: Option<f64> }
/// z used at look k (1-based). `alpha_spend = false`: constant cfg.z. `true`: z_k = cfg.z * sqrt(1 + ln k) (a crude
/// law-of-iterated-log style inflation so repeated looks cannot ratchet a lucky early pass into a certificate).
pub fn look_z(cfg: &GateCfg, k: usize, alpha_spend: bool) -> f64 { if alpha_spend { cfg.z * (1.0 + (k as f64).ln()).sqrt() } else { cfg.z } }
/// Run 4 persistent chains in `slice_ms` slices until the whole-answer gate passes (at the look's z) or `deadline_ms`.
/// With `per_ticket_target = Some(f)` the stop rule is instead "fraction >= f of tickets released".
/// `slice_growth` > 1 makes slice k last slice_ms * growth^(k-1) (geometric looks: fewer gate evaluations and fewer chances to stop on noise).
#[allow(clippy::too_many_arguments)]
pub fn decide_anytime(p: &Problem, deadline_ms: f64, slice_ms: f64, slice_growth: f64, seed: u64, cfg: &GateCfg, alpha_spend: bool, per_ticket_target: Option<f64>) -> Option<Anytime> {
    let t0 = probbit_core::rt::Instant::now(); let na = p.a;
    let mut chains: Vec<Chain> = (0..4).map(|c| Chain::new(p, seed, c as u64)).collect::<Option<Vec<_>>>()?;
    if auto_group_pairs(p) { let gpt = GroupPairs::new(p); for ch in chains.iter_mut() { ch.gp = gpt.clone(); } }
    let wt = window_tables(p); for ch in chains.iter_mut() { ch.set_window(&wt); }
    struct Acc { marg: Vec<f64>, traj: Vec<u16>, trace: Vec<f64>, n: usize, k: usize, viol: usize, best: (f64, Vec<usize>) }
    let mut accs: Vec<Acc> = (0..4).map(|_| Acc { marg: vec![0.0; p.t * na], traj: vec![], trace: vec![], n: 0, k: 0, viol: 0, best: (f64::NEG_INFINITY, vec![]) }).collect();
    let mut looks = 0;
    loop {
        let now = t0.elapsed().as_secs_f64() * 1e3; let slice = (slice_ms * slice_growth.powi(looks as i32)).min(deadline_ms - now).max(0.0);
        std::thread::scope(|sc| { for (ch, ac) in chains.iter_mut().zip(accs.iter_mut()) { sc.spawn(move || {
            let ts = probbit_core::rt::Instant::now(); let mut j = 0usize;
            loop { if j % 8 == 0 && ts.elapsed().as_secs_f64() * 1e3 >= slice { break; }
                ch.sweep(); j += 1; ac.k += 1; if ac.k <= 20 { continue; }
                for i in 0..p.t { ac.marg[i * na + ch.x[i]] += 1.0; }
                ac.n += 1; ac.traj.extend(ch.x.iter().map(|&v| v as u16)); ac.viol += (p.violations(&ch.x) > 0) as usize;
                let lw = ch.logw(); ac.trace.push(lw); if lw > ac.best.0 { ac.best = (lw, ch.x.clone()); } }
                if ac.n == 0 { ac.best = (ch.logw(), ch.x.clone()); } }); } }); // never an empty best plan
        looks += 1;
        let mut s = Samples { chain_marg: vec![], traj: vec![], marg: vec![0.0; p.t * na], n: 0, plans: HashMap::new(), trace: vec![], viol: 0, best: (f64::NEG_INFINITY, vec![]), sweeps: 0, moves: vec![] };
        for ac in accs.iter_mut() { for (m, v) in s.marg.iter_mut().zip(&ac.marg) { *m += v; } s.chain_marg.push(ac.marg.iter().map(|v| v / (ac.n as f64).max(1.0)).collect());
            s.n += ac.n; s.viol += ac.viol; s.sweeps += ac.k; if ac.best.0 > s.best.0 { s.best = ac.best.clone(); }
            s.traj.push(std::mem::take(&mut ac.traj)); s.trace.push(std::mem::take(&mut ac.trace)); }
        let n = s.n as f64; for m in s.marg.iter_mut() { *m /= n.max(1.0); }
        let g = gate_stats(p, &s); let c = GateCfg { z: look_z(cfg, looks, alpha_spend), ..*cfg };
        let pass = match per_ticket_target { None => g.diagnostics_passed(&c),
            Some(f) => g.released_tasks(&c).iter().filter(|&&r| r).count() as f64 >= f * p.t as f64 };
        let ms = t0.elapsed().as_secs_f64() * 1e3;
        if pass || ms >= deadline_ms - 0.5 {
            let verdict = if g.diagnostics_passed(&c) { Verdict::DiagnosticsPassed { rhat: g.rhat } } else { Verdict::Unmixed { rhat: g.rhat } };
            let d = Decision { verdict, map_logw: s.best.0, map: s.best.1.clone(), top: vec![], marg: s.marg.clone(), ms, samples: s.n };
            return Some(Anytime { decision: d, gate: g, samples: s, looks, passed_at_ms: if pass { Some(ms) } else { None } });
        }
        for (ac, (tr, tc)) in accs.iter_mut().zip(s.traj.into_iter().zip(s.trace.into_iter())) { ac.traj = tr; ac.trace = tc; }
        let reps = ANYTIME_FOCUS_REPS.load(std::sync::atomic::Ordering::Relaxed);
        if reps > 0 { let rel = g.released_tasks(&c); let f: Vec<usize> = (0..p.t).filter(|&i| !rel[i]).collect();
            for ch in chains.iter_mut() { ch.focus = f.clone(); ch.focus_reps = reps; } }
    }
}

// ---------------- exact two-group joint heat-bath ("group-split block move") ----------------
/// For every group: all assignments of its members to allowed agents (caps ignored), bucketed by load vector over the
/// group's agent set, with static log-weights (h + lam * same-agent pairs inside the group; there are no cross-group
/// terms in log w). A move picks two groups whose agent sets intersect, frees their slots, and samples their JOINT
/// assignment exactly from the conditional given everything else: first a (bucket1, bucket2) pair among those whose
/// summed loads fit the free capacity, then an assignment inside each bucket. This lets a group leave a shared agent
/// at the same moment another group takes it (the barrier crossing single-group blocks and swaps cannot make).
/// Only built if every group has <= 4096 assignments.
#[derive(Clone)]
pub struct GroupPairs { gs: Vec<GTab>, pairs: Vec<(usize, usize)>, /// groups sharing >= 1 agent with group u
    nbr: Vec<Vec<usize>> }
#[derive(Clone)]
struct GTab { mem: Vec<usize>, ag: Vec<usize>, /// bucket: load vector over ag, log-sum weight, (lw, assignment) list
    buckets: Vec<(Vec<u8>, f64, Vec<(f64, Vec<u8>)>)>, /// exp(bucket log-sum - max over buckets) (linear DP weights)
    bw: Vec<f64> }
impl GroupPairs {
    pub fn new(p: &Problem) -> Option<GroupPairs> {
        let gs = group_tabs(p)?;
        let mut pairs = vec![]; let mut nbr = vec![vec![]; gs.len()];
        for u in 0..gs.len() { for v in u + 1..gs.len() { if gs[u].ag.iter().any(|a| gs[v].ag.contains(a)) { pairs.push((u, v)); nbr[u].push(v); nbr[v].push(u); } } }
        if pairs.is_empty() { return None; }
        Some(GroupPairs { gs, pairs, nbr })
    }
}
/// Per-group assignment tables (see `GroupPairs`).
/// None if some group has more than 4096 assignments, or more than 255 workers or members: assignments and bucket loads are
/// stored as u8, and a 300-worker group wrapped them (a task placed on a worker it does not allow, odds off). Skipping the
/// move keeps the chain exact, as `WINDOW_MAX_STATES` does for windows.
fn group_tabs(p: &Problem) -> Option<Vec<GTab>> {
        let mut gm: std::collections::BTreeMap<usize, Vec<usize>> = Default::default();
        for i in 0..p.t { if p.group[i] != usize::MAX { gm.entry(p.group[i]).or_default().push(i); } }
        let mems: Vec<Vec<usize>> = gm.into_values().collect();
        let mut gs = vec![];
        for mem in mems {
            let ag: Vec<usize> = (0..p.a).filter(|&a| mem.iter().any(|&i| p.ok(i, a))).collect();
            if ag.len() > 255 || mem.len() > 255 { return None; }
            let n: f64 = mem.iter().map(|&i| (0..p.a).filter(|&a| p.ok(i, a)).count() as f64).product(); if n > 4096.0 { return None; }
            let mut map: HashMap<Vec<u8>, Vec<(f64, Vec<u8>)>> = HashMap::new();
            let mut cur = vec![0u8; mem.len()];
            fn rec(p: &Problem, mem: &[usize], ag: &[usize], k: usize, lw: f64, cur: &mut Vec<u8>, map: &mut HashMap<Vec<u8>, Vec<(f64, Vec<u8>)>>) {
                if k == mem.len() { let mut lv = vec![0u8; ag.len()]; for &c in cur.iter() { lv[c as usize] += 1; } map.entry(lv).or_default().push((lw, cur.clone())); return; }
                let i = mem[k];
                for (ai, &a) in ag.iter().enumerate() { if !p.ok(i, a) { continue; }
                    let mut d = p.h[i * p.a + a]; for q in 0..k { if cur[q] as usize == ai { d += p.lam; } }
                    cur[k] = ai as u8; rec(p, mem, ag, k + 1, lw + d, cur, map); }
            }
            rec(p, &mem, &ag, 0, 0.0, &mut cur, &mut map);
            let mut buckets: Vec<(Vec<u8>, f64, Vec<(f64, Vec<u8>)>)> = map.into_iter().map(|(lv, v)| {
                let mx = v.iter().map(|x| x.0).fold(f64::NEG_INFINITY, f64::max); let lse = mx + v.iter().map(|x| (x.0 - mx).exp()).sum::<f64>().ln(); (lv, lse, v) }).collect();
            buckets.sort_by(|a, b| a.0.cmp(&b.0));
            let bm = buckets.iter().map(|b| b.1).fold(f64::NEG_INFINITY, f64::max); let bw = buckets.iter().map(|b| (b.1 - bm).exp()).collect();
            gs.push(GTab { mem, ag, buckets, bw });
        }
        Some(gs)
}
impl<'a> Chain<'a> {
    fn group_pair(&mut self) {
        let gp = self.gp.take().unwrap(); let p = self.p;
        let (u, v) = gp.pairs[self.rng.below(gp.pairs.len())]; let (g1, g2) = (&gp.gs[u], &gp.gs[v]);
        for &i in g1.mem.iter().chain(&g2.mem) { self.load[self.x[i]] -= 1; }
        // A cap of t or more never binds (at most t tasks): min(cap, t) keeps every comparison and fits an i32 (a cap of
        // 1e12 or 2^32 + 2, legal up to 2^53, truncated to a wrong, even negative, free capacity)
        let free: Vec<i32> = (0..p.a).map(|a| p.cap[a].min(p.t) as i32 - self.load[a] as i32).collect();
        let fits = |g: &GTab, lv: &[u8]| g.ag.iter().zip(lv).all(|(&a, &l)| l as i32 <= free[a]);
        let b1: Vec<usize> = (0..g1.buckets.len()).filter(|&k| fits(g1, &g1.buckets[k].0)).collect();
        let b2: Vec<usize> = (0..g2.buckets.len()).filter(|&k| fits(g2, &g2.buckets[k].0)).collect();
        // shared agents: index pairs (position in g1.ag, position in g2.ag)
        let sh: Vec<(usize, usize, usize)> = g1.ag.iter().enumerate().filter_map(|(q, &a)| g2.ag.iter().position(|&b| b == a).map(|r| (q, r, a))).collect();
        let mut cand: Vec<(usize, usize, f64)> = Vec::with_capacity(b1.len() * b2.len()); let mut mx = f64::NEG_INFINITY;
        for &k1 in &b1 { let l1 = &g1.buckets[k1].0; for &k2 in &b2 { let l2 = &g2.buckets[k2].0;
            if sh.iter().all(|&(q, r, a)| l1[q] as i32 + l2[r] as i32 <= free[a]) { let w = g1.buckets[k1].1 + g2.buckets[k2].1; if w > mx { mx = w; } cand.push((k1, k2, w)); } } }
        if cand.is_empty() { for &i in g1.mem.iter().chain(&g2.mem) { self.load[self.x[i]] += 1; } self.gp = Some(gp); return; } // unreachable: current state fits
        let tot: f64 = cand.iter().map(|c| (c.2 - mx).exp()).sum(); let mut r = self.rng.f64() * tot; let mut pick = cand.len() - 1;
        for (k, c) in cand.iter().enumerate() { let w = (c.2 - mx).exp(); if r < w { pick = k; break; } r -= w; }
        let (k1, k2, _) = cand[pick];
        for (g, k) in [(g1, k1), (g2, k2)] { let asg = &g.buckets[k].2; let m = asg.iter().map(|x| x.0).fold(f64::NEG_INFINITY, f64::max);
            let tt: f64 = asg.iter().map(|x| (x.0 - m).exp()).sum(); let mut r = self.rng.f64() * tt; let mut pk = asg.len() - 1;
            for (q, x) in asg.iter().enumerate() { let w = (x.0 - m).exp(); if r < w { pk = q; break; } r -= w; }
            for (q, &i) in g.mem.iter().enumerate() { let a = g.ag[asg[pk].1[q] as usize]; self.x[i] = a; self.load[a] += 1; } }
        self.gp = Some(gp);
    }
}

// ---------------- frontier-DP exact k-group block heat-bath ("window move") ----------------
/// Groups per window move (0 = off). A window = up to K groups grown by breadth-first search over the shared-agent graph
/// from a random group (neighbours in random order: the choice never looks at the state). The move frees every slot of the
/// window and draws the window's JOINT assignment exactly from pi(x_window | rest) by forward-filtering / backward-sampling
/// over the groups, where the DP state is the load vector on the "frontier" agents (used by a group already processed AND a
/// group still to come). Log w has no cross-group terms, so the conditional is a product of per-group bucket weights under
/// the joint capacity constraint: exact, feasibility-preserving, and pi-invariant (a Gibbs block update). K = 2 is the
/// two-group move; on chain-like agent sharing the frontier stays tiny, so K = all groups is an exact independent draw.
pub static WINDOW_K: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
/// Window moves per sweep (used only when WINDOW_K > 0).
pub static WINDOW_REPS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(1);
/// A window whose DP needs more states than this at some step, more than 16 frontier agents, or more than 255 members
/// is skipped. The test reads only the window and the loads of tasks outside it, which the move never changes, so skipping
/// keeps the kernel pi-invariant.
pub const WINDOW_MAX_STATES: usize = 1 << 13;
fn window_tables(p: &Problem) -> Option<GroupPairs> { if WINDOW_K.load(std::sync::atomic::Ordering::Relaxed) > 0 { GroupPairs::new(p) } else { None } }
impl<'a> Chain<'a> {
    fn set_window(&mut self, wt: &Option<GroupPairs>) {
        if wt.is_some() { self.win = wt.clone(); self.win_k = WINDOW_K.load(std::sync::atomic::Ordering::Relaxed).max(1); self.win_reps = WINDOW_REPS.load(std::sync::atomic::Ordering::Relaxed); }
    }
    fn window_move(&mut self) {
        let gp = self.win.take().unwrap(); let p = self.p; let ng = gp.gs.len();
        let mut sel = vec![self.rng.below(ng)]; let mut inw = vec![false; ng]; inw[sel[0]] = true; let mut head = 0;
        while sel.len() < self.win_k && head < sel.len() {
            let mut nb = gp.nbr[sel[head]].clone(); head += 1;
            for q in (1..nb.len()).rev() { let r = self.rng.below(q + 1); nb.swap(q, r); }
            for v in nb { if sel.len() >= self.win_k { break; } if !inw[v] { inw[v] = true; sel.push(v); } }
        }
        let k = sel.len(); let members: usize = sel.iter().map(|&g| gp.gs[g].mem.len()).sum();
        if members > 255 { self.win = Some(gp); return; }
        // processing order: greedy min-frontier (static: depends only on the window), so the DP state stays small
        let sel = frontier_order(p.a, &gp.gs, &sel);
        let mut first = vec![usize::MAX; p.a]; let mut last = vec![0usize; p.a];
        for (j, &g) in sel.iter().enumerate() { for &a in &gp.gs[g].ag { if first[a] == usize::MAX { first[a] = j; } last[a] = j; } }
        let fr = frontiers(&gp.gs, &sel, &last);
        if fr.iter().any(|f| f.len() > 16) { self.win = Some(gp); return; }
        for &g in &sel { for &i in &gp.gs[g].mem { self.load[self.x[i]] -= 1; } }
        let free: Vec<i32> = (0..p.a).map(|a| p.cap[a].min(p.t) as i32 - self.load[a] as i32).collect(); // as `group_pair`
        let lane = |f: &[usize], a: usize| f.iter().position(|&b| b == a);
        // per step: roles of the group's agents (lane in fr[j-1], lane in fr[j]), carried lanes, and buckets that fit on their own
        type Step = (Vec<(Option<usize>, Option<usize>)>, Vec<(usize, usize)>, Vec<usize>);
        let steps: Vec<Step> = (0..k).map(|j| {
            let g = &gp.gs[sel[j]]; let prev: &[usize] = if j == 0 { &[] } else { &fr[j - 1] };
            let roles = g.ag.iter().map(|&a| (lane(prev, a), lane(&fr[j], a))).collect();
            let carried = prev.iter().enumerate().filter(|(_, a)| !g.ag.contains(a)).filter_map(|(lp, &a)| lane(&fr[j], a).map(|ln| (lp, ln))).collect();
            let okb = (0..g.buckets.len()).filter(|&bi| g.ag.iter().zip(&g.buckets[bi].0).all(|(&a, &l)| l as i32 <= free[a])).collect();
            (roles, carried, okb) }).collect();
        let trans = |j: usize, s: u128, bi: usize| -> Option<u128> {
            let g = &gp.gs[sel[j]]; let (roles, carried, _) = &steps[j]; let lv = &g.buckets[bi].0; let mut ns: u128 = 0;
            for &(lp, ln) in carried { ns |= ((s >> (8 * lp)) & 0xff) << (8 * ln); }
            for (q, &a) in g.ag.iter().enumerate() { let u = lv[q] as u128 + roles[q].0.map_or(0, |lp| (s >> (8 * lp)) & 0xff);
                if u as i32 > free[a] { return None; } if let Some(ln) = roles[q].1 { ns |= u << (8 * ln); } }
            Some(ns)
        };
        // forward filtering; tr[j] = every valid transition (new key, index into alpha[j], bucket, weight), sorted by new key
        let mut alpha: Vec<Vec<(u128, f64)>> = Vec::with_capacity(k + 1); alpha.push(vec![(0, 1.0)]);
        let mut tr: Vec<Vec<(u128, u32, u16, f64)>> = Vec::with_capacity(k);
        for j in 0..k {
            let g = &gp.gs[sel[j]]; let mut t: Vec<(u128, u32, u16, f64)> = vec![];
            for (si, &(s, w)) in alpha[j].iter().enumerate() { for &bi in &steps[j].2 { if let Some(ns) = trans(j, s, bi) { t.push((ns, si as u32, bi as u16, w * g.bw[bi])); } } }
            t.sort_by_key(|e| e.0);
            let mut v: Vec<(u128, f64)> = vec![]; for e in &t { match v.last_mut() { Some(l) if l.0 == e.0 => l.1 += e.3, _ => v.push((e.0, e.3)) } }
            if v.is_empty() || v.len() > WINDOW_MAX_STATES { for &gg in &sel { for &i in &gp.gs[gg].mem { self.load[self.x[i]] += 1; } } self.win = Some(gp); return; }
            let mx = v.iter().map(|e| e.1).fold(0.0, f64::max); for e in v.iter_mut() { e.1 /= mx; } for e in t.iter_mut() { e.3 /= mx; }
            alpha.push(v); tr.push(t);
        }
        // backward sampling: the last frontier is empty, so alpha[k] holds the single key 0
        let mut target = alpha[k][0].0; let mut pick = vec![0usize; k];
        for j in (0..k).rev() {
            let t = &tr[j]; let lo = t.partition_point(|e| e.0 < target); let hi = t.partition_point(|e| e.0 <= target);
            let tot: f64 = t[lo..hi].iter().map(|e| e.3).sum(); let mut r = self.rng.f64() * tot; let mut pk = hi - 1;
            for q in lo..hi { if r < t[q].3 { pk = q; break; } r -= t[q].3; }
            pick[j] = t[pk].2 as usize; target = alpha[j][t[pk].1 as usize].0;
        }
        for (j, &gi) in sel.iter().enumerate() { let g = &gp.gs[gi]; let asg = &g.buckets[pick[j]].2;
            let m = asg.iter().map(|x| x.0).fold(f64::NEG_INFINITY, f64::max); let tt: f64 = asg.iter().map(|x| (x.0 - m).exp()).sum();
            let mut r = self.rng.f64() * tt; let mut pk = asg.len() - 1;
            for (q, x) in asg.iter().enumerate() { let w = (x.0 - m).exp(); if r < w { pk = q; break; } r -= w; }
            for (q, &i) in g.mem.iter().enumerate() { let a = g.ag[asg[pk].1[q] as usize]; self.x[i] = a; self.load[a] += 1; } }
        self.win = Some(gp);
    }
}

// ---------------- exact odds by frontier DP over all groups ("exact_frontier") ----------------
/// fr[j] = agents used at some step <= j and at some later step (sorted), built incrementally.
fn frontiers(gs: &[GTab], sel: &[usize], last: &[usize]) -> Vec<Vec<usize>> {
    let mut cur: Vec<usize> = vec![]; let mut out = Vec::with_capacity(sel.len());
    for (j, &g) in sel.iter().enumerate() { cur.retain(|&a| last[a] > j); for &a in &gs[g].ag { if last[a] > j && !cur.contains(&a) { cur.push(a); } }
        let mut f = cur.clone(); f.sort(); out.push(f); }
    out
}
/// Greedy min-frontier elimination order over the given group tables (static: depends only on the problem).
fn frontier_order(na: usize, gs: &[GTab], sel: &[usize]) -> Vec<usize> {
    let mut rem = vec![0usize; na]; for &g in sel { for &a in &gs[g].ag { rem[a] += 1; } }
    let mut open = vec![false; na]; let mut n_open = 0usize; let mut left: Vec<usize> = sel.to_vec(); let mut ord = Vec::with_capacity(sel.len());
    while !left.is_empty() {
        let mut best = (usize::MAX, 0usize);
        // frontier size after taking g = open now - agents g closes + agents g opens (O(|ag|) per candidate)
        for (q, &g) in left.iter().enumerate() { let ag = &gs[g].ag;
            let after = n_open - ag.iter().filter(|&&a| open[a] && rem[a] == 1).count() + ag.iter().filter(|&&a| !open[a] && rem[a] > 1).count();
            if after < best.0 { best = (after, q); } }
        let g = left.remove(best.1); for &a in &gs[g].ag { rem[a] -= 1; let o = rem[a] > 0; if o != open[a] { if o { n_open += 1; } else { n_open -= 1; } } open[a] = o; } ord.push(g);
    }
    ord
}
/// Exact result of `exact_frontier`: marginals, one exact MAP plan and its log w, log Z, and the largest DP layer.
pub use probbit_ir::FrontierExact;
/// DP layer cap for the `decide_gated` exact tier. Measured: every oracle family peaks at <= 266 states; thick
/// all-shared queues (6 workers, groups of 3) are solved exactly up to T = 30 and declined in 5-10 ms at T = 60-300.
pub const FRONTIER_MAX_STATES: usize = probbit_ir::FRONTIER_MAX_STATES;
/// EXACT marginals and an exact MAP plan without enumeration, for any problem whose group/agent sharing is thin.
/// Every group (ungrouped tasks = singleton groups) is eliminated in a greedy min-frontier order; the DP state is the load
/// vector on frontier agents. Returns None when a group has > 4096 assignments, the frontier exceeds 16 agents, a frontier
/// load exceeds 255, or a layer exceeds `max_states` (thick sharing: use the sampler), or if nothing is feasible.
/// Runs on the IR (`probbit_ir::exact_frontier` over `lower()`: components = groups, caps = agents); the earlier crate-local copy was
/// deleted after matching it on 7 cases (same DP states, marg <= 3.3e-16, logz <= 6.8e-13; golden values in acceptance)
/// at equal speed (lowering included: -19% to +3.3% wall time, examples/ir_frontier_speed).
pub fn exact_frontier(p: &Problem, max_states: usize) -> Option<FrontierExact> { probbit_ir::exact_frontier(&p.lower(), max_states) }
/// `exact_frontier` that declines once `hard` passes (`--exact-ms`); None = `exact_frontier`.
/// The lowering (O(group size^2) pairs) ran before the IR read the clock, so one 3,000-task group spent
/// ~115 ms past `--exact-ms 50`; a passed deadline now declines first (the IR declines on a passed deadline anyway: same answers).
/// The lowering itself is still not interruptible.
pub fn exact_frontier_until(p: &Problem, max_states: usize, hard: Option<probbit_core::rt::Instant>) -> Option<FrontierExact> {
    if hard.is_some_and(|h| probbit_core::rt::Instant::now() >= h) { return None; }
    probbit_ir::exact_frontier_until(&p.lower(), max_states, hard)
}

// ---------------- plan polish (the sampler's best-seen plan is NOT the optimum) ----------------
/// Anneal the same constraint-preserving moves over inverse temperatures beta = 2 -> 32 (geometric, one continuous schedule
/// per chain, 4 chains on threads, `ms` wall-clock), each chain warm-started from `start` if given (else a dispersed random
/// start), and return the best feasible plan by the ORIGINAL log w (never worse than `start`). Heuristic: no optimality
/// proof; an ILP / exact DP gives one.
/// The exact two-group move is used by `decide_gated` / `decide_anytime` iff lam >= 2 (measured at T=200: halves TV per ms
/// at lam 2 and 3x at lam 4; costs 2x per ms at lam 0.5, about even at 0.8) and the group tables are small enough to build.
pub const AUTO_GPAIR_LAM: f64 = 2.0;
pub fn auto_group_pairs(p: &Problem) -> bool { p.lam >= AUTO_GPAIR_LAM }
pub fn polish_plan(p: &Problem, start: Option<&[usize]>, ms: f64, seed: u64) -> Option<(f64, Vec<usize>)> { polish_plan_on(p, start, ms, 0, 4, seed) }
/// Fixed-work polish: `sweeps` sweeps per chain in total (split evenly over the betas) instead of `ms` of wall clock, so the
/// polished plan is a pure function of (problem, start, sweeps, seed) for any `--threads`. Backs `probbit decide --polish-sweeps`.
pub fn polish_plan_sweeps(p: &Problem, start: Option<&[usize]>, sweeps: usize, seed: u64) -> Option<(f64, Vec<usize>)> { polish_plan_on(p, start, 0.0, sweeps.max(1), 4, seed) }
/// The polish's 4 chains on `threads` workers (`--threads`); see `probbit_ir::anneal_on` for the ms / sweeps semantics.
pub fn polish_plan_on(p: &Problem, start: Option<&[usize]>, ms: f64, sweeps: usize, threads: usize, seed: u64) -> Option<(f64, Vec<usize>)> {
    if start.is_some_and(|x| x.len() != p.t || x.iter().any(|&a| a >= p.a) || p.violations(x) != 0) { return None; }
    let seq = probbit_core::rt::sequential(); let t = if seq { 1 } else { threads.clamp(1, 4) }; let ms = ms / 4usize.div_ceil(t) as f64;
    let betas = [2.0, 4.0, 8.0, 16.0, 32.0];
    let qs: Vec<Problem> = betas.iter().map(|&b| { let mut q = p.clone(); for v in q.h.iter_mut() { *v *= b; } q.lam *= b; q }).collect();
    let run = |c: usize| -> Option<(f64, Vec<usize>)> {
        let mut x: Vec<usize> = match start { Some(s) => s.to_vec(), None => Chain::new(p, seed, c as u64)?.x };
        let mut best = (p.logw(&x), x.clone()); let t0 = probbit_core::rt::Instant::now();
        for (k, q) in qs.iter().enumerate() {
            // Past the chain's whole wall-clock share, every remaining beta would run 0 sweeps (each stops at <= ms), so
            // skip building their chains (`Chain::new` = a random feasible start + 5 uniform sweeps: ~20 ms each on 100k grouped
            // tasks, profiled; the ~2 s overrun itself was `Problem::logw`, see there). Same plan: x and best are untouched by a 0-sweep beta; the one
            // difference is a later `Chain::new` that fails no longer discards the polish (the CLI then fell back to the sampler's plan).
            if sweeps == 0 && t0.elapsed().as_secs_f64() * 1e3 >= ms { break; }
            let mut ch = Chain::new(q, seed ^ 0x5eed, (c * 8 + k) as u64)?; ch.x = x.clone(); ch.load = vec![0; p.a]; for &a in &x { ch.load[a] += 1; }
            let stop = ms * (k + 1) as f64 / betas.len() as f64; let mut j = 0usize;
            let n = ((sweeps as u128 * (k + 1) as u128) / betas.len() as u128 - (sweeps as u128 * k as u128) / betas.len() as u128) as usize;
            while if sweeps > 0 { j < n } else { j % 4 != 0 || t0.elapsed().as_secs_f64() * 1e3 < stop } { ch.sweep(); j += 1; let lw = p.logw(&ch.x); if lw > best.0 { best = (lw, ch.x.clone()); } }
            x = ch.x.clone();
        }
        Some(best)
    };
    let mut slots: Vec<Option<(f64, Vec<usize>)>> = (0..4).map(|_| None).collect();
    if seq { for (c, slot) in slots.iter_mut().enumerate() { *slot = run(c); } } else {
    std::thread::scope(|sc| { let hs: Vec<_> = (0..t).map(|w| { let run = &run; sc.spawn(move || (w..4).step_by(t).map(|c| (c, run(c))).collect::<Vec<_>>()) }).collect();
        for h in hs { for (c, r) in h.join().unwrap() { slots[c] = r; } } }); }
    slots.into_iter().collect::<Option<Vec<_>>>()?.into_iter().max_by(|a, b| a.0.partial_cmp(&b.0).unwrap())
}
