//! `probbit` — a virtual probabilistic (p-bit) processor for joint decisions: exact where structure allows, sampled with
//! published diagnostics otherwise.
//!
//! JSON problem in on stdin, JSON decision out on stdout. Zero external crates.
//!
//!   probbit decide [--budget-ms N] [--seed N] [--exact-limit N] [--exact-ms N] [--frontier-states N] [--polish-ms N] [--polish-sweeps N] [--mode auto|exact|sample] [--sweeps N] [--collective on|off] [--cluster on|off] [--cycles on|off] [--chains N] [--threads N] [--cpu-limit PCT] [--mem-limit-mb N] [--priority low|normal] [--max-input-mb N] [--pretty]
//!   probbit demo   [--tasks N] [--seed N] [--hard] # emit an agent-routing problem as JSON; --hard = tight quotas + strong affinity
//!   probbit ir [--max-input-mb N]                # emit the problem in probbit-ir v0 (the hardware-facing text form)
//!   probbit run [--op decide|exact|sample] [--budget-ms N] [--deadline-ms N] [--seed N] [--exact-limit N] [--exact-ms N] [--frontier-states N] [--polish-ms N] [--polish-sweeps N] [--sweeps N] [--collective on|off] [--cluster on|off] [--cycles on|off] [--chains N] [--threads N] [--cpu-limit PCT] [--mem-limit-mb N] [--priority low|normal] [--max-input-mb N] [--pretty]  # a general probbit-ir JSON program
//!   probbit evaluate [run's flags] [--program]  # the decision-API adapter: a System One request + the judge's answers + rules -> the joint answer
//!   --progress [MS] (decide, run, evaluate): one JSONL telemetry line on stderr every MS ms (default 100) while the decision runs
//!   probbit stats [--sweeps N]                  # the processor's spec sheet: machine, build, effective controls + source, measured updates/s
//!   probbit persona init|turn|replay|explain|diff|lint|fuzz|prove|check|compile|describe PERSONA [flags]  # the individuality layer (docs/persona.md)
//!   probbit check PERSONA [--props FILE] [--strand FILE]... [--json]  # one CI gate: lint + prove + fuzz the rules, replay the strands
//!   probbit mcp                                 # a Model Context Protocol server on stdio (tools probbit_decide, probbit_run, probbit_stats, probbit_demo, probbit_evaluate, probbit_persona_init, probbit_persona_turn, probbit_persona_fuzz)
//!   probbit version
//!   probbit <command> --help | -h               # usage, every flag with its default, exit codes
//!   probbit --help | -h                          # the usage above (stdout, exit 0); at a terminal the hero screen
//!   --top, --summary (decide, run), --live (demo), --plain: see `probbit <command> --help`; visuals go to stderr (theme.rs)
mod explain;
mod fuzz;
mod labels;
mod live;
mod mcp;
mod monitor;
mod theme;
mod tui;
mod usage;
// the document layer and the strand writer: the probbit-persona library (the CLI's modules name them as `crate::json` etc.)
use probbit_persona::{evaluate, json, persona, router, run, sys};
use json::{num, obj, str as jstr, Json};
pub(crate) use router::{demo_doc, from_json, DEMO_TPL};
use probbit_decide::*;
use std::io::{Read, Write};
use usage::{Command, Flag, Sub};

const VERSION: &str = env!("CARGO_PKG_VERSION");


/// Every stderr line goes through here. `eprintln!` panicked when stderr was a pipe whose reader had gone (`probbit decide --bogus
/// 2>&1 >/dev/null | true`, or `--progress` into `head -c 1`: exit 134 with panic = abort, the decision lost to a log line). A
/// failed write to stderr is ignored: the command keeps its stdout and its exit code.
fn err_line(s: &str) { let _ = writeln!(std::io::stderr(), "{s}"); }
/// The error channel (R94, docs/CHEATSHEET.md "Errors"): a human line on stderr, always. A bad input document also gets its
/// `{"error"}` object on stdout when stdout is not a terminal (every wrapper reads it there: python/, mcp.rs, npm, the examples) or
/// with `--json`; at a terminal without `--json`, only the line. Usage errors (`fail`: an unknown command, subcommand or flag, a bad
/// flag value) stay off stdout unless `--json` is given, so a piped caller tells them from input errors by an empty stdout.
static JSON_FLAG: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
fn json_flag() -> bool { JSON_FLAG.load(std::sync::atomic::Ordering::Relaxed) }
/// An input error's object goes to stdout (see `JSON_FLAG`)
fn error_object(e: &Json) { if json_flag() || !theme::stdout_is_terminal() { emit(&json::write(e, false)); } }
fn fail(msg: &str) -> ! { fail_as("flag", msg, None) }
/// `fail` with the code of the `--json` object and, for an unknown command or subcommand, its `suggestion` (a name or null)
fn fail_as(code: &str, msg: &str, suggestion: Option<Option<&str>>) -> ! {
    tui::top_stop(); err_line(&format!("probbit: {msg}"));
    if json_flag() {
        let mut e = vec![("code", jstr(code)), ("path", jstr("")), ("message", jstr(msg))];
        if let Some(s) = suggestion { e.push(("suggestion", s.map_or(Json::Null, jstr))); }
        emit(&json::write(&obj(vec![("error", obj(e))]), false));
    }
    exit(2)
}
/// An unknown command (`sub` None) or subcommand of `cmd`: exit 2 with the closest name (`usage::suggest`) if there is one:
/// `probbit: unknown command "persna"; did you mean "persona"? see `probbit --help``
fn unknown(cmd: Option<&Command>, word: &str) -> ! {
    let (what, cands, see): (String, Vec<&str>, String) = match cmd {
        None => ("unknown command".into(), COMMANDS.iter().map(|c| c.name).collect(), "probbit --help".into()),
        Some(c) => (format!("unknown {} subcommand", c.name), c.subs.iter().map(|s| s.name).collect(), format!("probbit {} --help", c.name)) };
    let s = usage::suggest(word, &cands);
    fail_as("command", &match s { Some(s) => format!("{what} {word:?}; did you mean {s:?}? see `{see}`"), None => format!("{what} {word:?}; see `{see}`") }, Some(s))
}
/// Exit with `code`, releasing a strand's writer lock this process holds first (live.rs `Lock`; `std::process::exit` skips `Drop`)
fn exit(code: i32) -> ! { live::release_held(); std::process::exit(code) }
/// `--budget-ms inf` / `1e300` aborted (exit 134: the deadline Duration overflowed) and `NaN` never stopped sampling
fn budget_ms(args: &[String]) -> f64 {
    let b: f64 = arg(args, "--budget-ms", 200.0);
    if !(b.is_finite() && (0.0..=1e9).contains(&b)) { fail("--budget-ms must be a number of milliseconds from 0 to 1e9") } b
}
/// `--polish-ms inf` / `NaN` hung `probbit decide` and `probbit run` (the polish loops while elapsed < the budget) and `-7` ran
/// (echoed in telemetry); the same range as --budget-ms, else exit 2
/// R19.6 (P2.1) `--deadline-ms N` (`probbit run` only): a whole-call wall-clock target, see `run::run`.
fn deadline_ms(args: &[String]) -> Option<f64> {
    args.iter().any(|a| a == "--deadline-ms").then(|| { let x: f64 = arg(args, "--deadline-ms", 0.0);
        if !(x.is_finite() && x > 0.0 && x <= 1e9) { fail("--deadline-ms must be a number of milliseconds above 0, at most 1e9") } x })
}
fn polish_ms_arg(args: &[String]) -> f64 {
    let x: f64 = arg(args, "--polish-ms", 50.0);
    if !(x.is_finite() && (0.0..=1e9).contains(&x)) { fail("--polish-ms must be a number of milliseconds from 0 to 1e9") } x
}
/// `--exact-ms N` (opt-in; absent = no cap, the output unchanged): a wall-clock cap on the exact tiers that run BEFORE
/// the sampler (enumeration / MRV search and the frontier DP, together, from the start of the decision). Past it they decline
/// as at --exact-limit: `decide` / `--mode auto` go on to the sampler with the full --budget-ms, `--op exact` / `--mode exact`
/// decline. Without it a loose program's exact tier counted plans to --exact-limit first (1.66 s at 3,000 vars, 200 ms budget).
fn exact_ms(args: &[String]) -> Option<f64> {
    args.iter().any(|a| a == "--exact-ms").then(|| { let x: f64 = arg(args, "--exact-ms", 0.0);
        if !(x.is_finite() && (0.0..=1e9).contains(&x)) { fail("--exact-ms must be a number of milliseconds from 0 to 1e9") } x })
}
/// Every stdout document goes through here. `println!` panicked on a closed pipe (`probbit demo --tasks 3000 | head -c 20`:
/// "failed printing to stdout: Broken pipe", exit 134 with panic = abort); a reader that went away is not an error, so a broken
/// pipe is ignored and the command keeps its normal exit code.
fn emit_raw(s: &str) {
    let mut o = std::io::stdout().lock();
    if let Err(e) = o.write_all(s.as_bytes()).and_then(|_| o.flush()) {
        if e.kind() != std::io::ErrorKind::BrokenPipe { err_line(&format!("probbit: cannot write the output: {e}")); exit(2) } }
}
fn emit(s: &str) { emit_raw(&format!("{s}\n")) }

/// Bad input (JSON, schema, values): a human line on stderr and ONE structured error object on stdout (`error_object`), exit 2.
fn bad_input(what: &str, e: json::InErr) -> ! {
    err_line(&format!("probbit: {what}{}{}", if e.path.is_empty() { String::new() } else { format!("{}: ", e.path) }, e.msg));
    error_object(&e.to_json()); exit(2)
}
/// Emit a decision; returns its exit code. Every number in it must be finite (docs/probbit-ir-json.md "Numeric contract"). A
/// gate diagnostic that could not be estimated (R-hat / bound infinite: too few samples, chains stuck at different values) is
/// allowed only on a refusal (exit 3): it prints as null and is named in `gate.non_finite`. Any other non-finite number = no
/// answer: ONE structured `numeric` error on stdout, exit 3 (it used to print `null`, or a literal `inf`, with exit 0).
/// How a decision is printed (decide, run): `--pretty`; `--summary` = the compact document (`summary_doc`); with both and a
/// terminal on stderr (and no NO_COLOR / --plain / PROBBIT_THEME=plain), the summary box there too.
pub(crate) struct View { pretty: bool, summary: bool, cmd: &'static str, boxed: Option<theme::Theme> }
impl View {
    fn new(args: &[String], cmd: &'static str) -> View {
        let (pretty, summary) = (args.iter().any(|a| a == "--pretty"), args.iter().any(|a| a == "--summary"));
        View { pretty, summary, cmd, boxed: if pretty && summary { theme::stderr(args) } else { None } }
    }
}
/// `--summary`: the answer without its per-item tables (`plan`, `odds` / `marginals`, `released` / `escalated` lists,
/// `release_reason`, `top_plans`), every other field as is, plus `"summary": 1`, `counts` (items, released, escalated) and the
/// items an agent should look at first: `worst_released` (up to 5: on a sampled answer the largest error bars, on an exact one
/// the smallest gap between the top two odds) and `worst_escalated` (up to 5, largest error bars), each with its id, plan value
/// and that value's probability `p`, top two odds, release reason and error bar (`bar` = z x max(MCSE short, MCSE long), the number the gate compares with
/// `tv_tol`; null where it is not estimable). Same verdict and exit code as the full document.
fn summary_doc(doc: &Json, bars: &[f64], cmd: &str) -> Json {
    let Json::Obj(v) = doc else { return doc.clone() };
    let ids = |k: &str| doc.get(k).and_then(Json::as_arr).map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_string)).collect::<Vec<_>>());
    let (released, escalated) = (ids("released"), ids("escalated"));
    let odds = doc.get(if cmd == "decide" { "odds" } else { "marginals" }); let plan = doc.get("plan"); let reasons = doc.get("release_reason");
    let pos: std::collections::HashMap<&str, usize> = plan.or(odds).and_then(Json::as_obj).map_or_else(Default::default, |o| o.iter().enumerate().map(|(i, (k, _))| (k.as_str(), i)).collect());
    let bar = |id: &str| pos.get(id).and_then(|&i| bars.get(i)).copied();
    let margin = |id: &str| odds.and_then(|o| o.get(id)).and_then(Json::as_obj).map_or(1.0, |o| { let p: Vec<f64> = o.iter().filter_map(|(_, x)| x.as_f64()).collect();
        p.first().copied().unwrap_or(1.0) - p.get(1).copied().unwrap_or(0.0) });
    let key = |id: &str| if bars.is_empty() { margin(id) } else { bar(id).map_or(f64::INFINITY, |b| if b.is_nan() { f64::NEG_INFINITY } else { -b }) };
    let item = |id: &str| { let mut f = vec![("id", jstr(id))];
        if let Some(x) = plan.and_then(|p| p.get(id)) { f.push(("value", x.clone()));
            // the plan's own value and its probability: the joint plan need not hold each item's most likely value
            if let Some(q) = x.as_str().and_then(|w| odds.and_then(|o| o.get(id)).and_then(|o| o.get(w))) { f.push(("p", q.clone())); } }
        f.push(("odds", Json::Obj(odds.and_then(|o| o.get(id)).and_then(Json::as_obj).map_or(vec![], |o| o.iter().take(2).cloned().collect()))));
        if let Some(r) = reasons.and_then(|r| r.get(id)) { f.push(("reason", r.clone())); }
        if !bars.is_empty() { f.push(("bar", bar(id).filter(|b| b.is_finite()).map_or(Json::Null, num))); }
        obj(f) };
    let worst = |l: &[String]| { let mut l: Vec<&String> = l.iter().collect(); l.sort_by(|a, b| key(a).total_cmp(&key(b))); Json::Arr(l.into_iter().take(5).map(|id| item(id)).collect()) };
    let n = if cmd == "decide" { doc.get("tasks").cloned() } else { doc.get("program").and_then(|p| p.get("vars")).cloned() };
    let mut counts = vec![(if cmd == "decide" { "tasks" } else { "vars" }, n.unwrap_or(Json::Null))];
    if let (Some(r), Some(e)) = (&released, &escalated) { counts.push(("released", num(r.len() as f64))); counts.push(("escalated", num(e.len() as f64))); }
    // counts go right after the verdict (and its tier / reason)
    let mut out: Vec<(String, Json)> = vec![]; let mut after_verdict = false;
    for (k, x) in v {
        if after_verdict && k != "tier" && k != "reason" { out.push(("counts".into(), obj(std::mem::take(&mut counts)))); after_verdict = false; }
        match k.as_str() {
            "plan" | "released_plan" | "odds" | "marginals" | "release_reason" | "top_plans" | "escalated" => {}
            "released" => {
                if let Some(r) = &released { out.push(("worst_released".into(), worst(r))); }
                if let Some(e) = &escalated { out.push(("worst_escalated".into(), worst(e))); } }
            _ => { out.push((k.clone(), x.clone()));
                if k == "engine" { out.push(("summary".into(), num(1.0))); }
                if k == "verdict" { after_verdict = true; } } } }
    if after_verdict { out.push(("counts".into(), obj(counts))); }
    Json::Obj(out)
}
fn finish(mut doc: Json, code: i32, view: &View, bars: &[f64]) -> i32 {
    tui::top_stop();
    let mut bad = vec![]; json::non_finite(&doc, "", &mut bad);
    if !bad.is_empty() {
        if code == 3 && bad.iter().all(|p| p.starts_with("gate.")) {
            if let Json::Obj(v) = &mut doc { if let Some((_, Json::Obj(g))) = v.iter_mut().find(|(k, _)| k == "gate") {
                g.push(("non_finite".to_string(), Json::Arr(bad.iter().map(|p| jstr(&p["gate.".len()..])).collect()))); } }
        } else {
            let e = json::InErr { code: "numeric", path: bad[0].clone(), msg: format!("{} computed quantit{} not finite (first: {}); no answer is emitted", bad.len(), if bad.len() == 1 { "y is" } else { "ies are" }, bad[0]) };
            err_line(&format!("probbit: numeric failure: {}", e.msg)); error_object(&e.to_json()); return 3;
        }
    }
    if view.summary { doc = summary_doc(&doc, bars, view.cmd); }
    emit(&json::write(&doc, view.pretty));
    if let Some(th) = view.boxed { let _ = std::io::stderr().write_all(tui::summary_box(th, view.cmd, &doc).as_bytes()); }
    code
}
/// stdin -> a parsed document, or `bad_input`
fn read_doc(args: &[String]) -> Json { json::parse(&read_stdin(args)).unwrap_or_else(|e| bad_input(if e.code == "schema" { "" } else { "bad input: " }, e)) }


/// A flag that is present must parse (exit 2 otherwise). It used to fall back to the default silently: `--sweeps 1e4`
/// ran in wall-clock mode (not the fixed-work, deterministic mode asked for) and `--seed x` ran seed 7.
fn arg<T: std::str::FromStr>(args: &[String], name: &str, default: T) -> T {
    match args.iter().position(|a| a == name) { None => default, Some(k) => match args.get(k + 1) {
        Some(v) => v.parse().unwrap_or_else(|_| fail(&format!("{name}: cannot read {v:?} (integer flags take plain digits, e.g. 10000 not 1e4)"))),
        None => fail(&format!("{name} needs a value")) } }
}
/// THE command table (R94): every command, subcommand and flag the parser accepts, with its help. `--help`, `probbit help`, the
/// completions and the man page render from it (usage.rs) and `check_flags` enforces it (`flags_of`, `sub_flags`), so what is
/// documented is what is accepted. Tests: the unit tests below and tests/cli_table.rs.
const F_SEARCH: &[Flag] = &[
    Flag { name: "--budget-ms", arg: "N", help: "sampler wall-clock budget, ms (default 200)" },
    Flag { name: "--seed", arg: "N", help: "random seed (default 7)" },
    Flag { name: "--exact-limit", arg: "N", help: "plans the exact enumeration may count before it declines (default 2000000)" },
    Flag { name: "--exact-ms", arg: "N", help: "wall-clock cap on the exact tiers that run before the sampler (opt-in; absent = no cap)" },
    Flag { name: "--frontier-states", arg: "N", help: "state cap of the frontier DP and the components tier (default 4096; 0 = off)" },
    Flag { name: "--polish-ms", arg: "N", help: "plan polish budget, ms (default 50; 0 = off)" },
    Flag { name: "--polish-sweeps", arg: "N", help: "fixed-work plan polish (deterministic)" },
    Flag { name: "--sweeps", arg: "N", help: "fixed work per chain instead of the wall-clock budget (with --polish-ms 0: a pure function of input + seed)" },
    Flag { name: "--collective", arg: "on|off", help: "global two-value flips + label swaps (default on)" },
    Flag { name: "--cluster", arg: "on|off", help: "Wolff cluster move (default on)" },
    Flag { name: "--cycles", arg: "on|off", help: "three-cycle rotations (default on)" },
];
const F_CONTROLS: &[Flag] = &[
    Flag { name: "--chains", arg: "N", help: "sampler chains, 1..=100000 (default 4)" },
    Flag { name: "--threads", arg: "N", help: "worker threads for the chains, the gate and the polish, 1..=1024 (default min(cores, 4))" },
    Flag { name: "--cpu-limit", arg: "PCT", help: "per-thread duty cycle 1..100 (default 100)" },
    Flag { name: "--mem-limit-mb", arg: "N", help: "cap on stored samples (default 1024; 0 = none)" },
    Flag { name: "--priority", arg: "low|normal", help: "OS scheduling priority (default normal)" },
];
const F_MAX_INPUT: Flag = Flag { name: "--max-input-mb", arg: "N", help: "largest stdin document read, MB (default 256; 0 = no limit)" };
const F_PRETTY: Flag = Flag { name: "--pretty", arg: "", help: "indented JSON" };
const F_PLAIN: Flag = Flag { name: "--plain", arg: "", help: "no colour, no animation (as NO_COLOR=1 or PROBBIT_THEME=plain); output otherwise unchanged" };
const F_PROGRESS: Flag = Flag { name: "--progress", arg: "[MS]", help: "one JSONL telemetry line on stderr every MS ms (default 100)" };
const F_SUMMARY: Flag = Flag { name: "--summary", arg: "", help: "compact answer: verdict, counts, gate, the 5 worst released and escalated items, telemetry (no per-item tables); with --pretty and a terminal on stderr, also a boxed summary there" };
const F_TOP: Flag = Flag { name: "--top", arg: "", help: "live monitor on stderr while it runs (tier, sweeps, updates/s, CPU, peak RSS, gate; 10 Hz), erased at exit; only with a terminal on stderr; stdout unchanged" };
const F_OP: Flag = Flag { name: "--op", arg: "decide|exact|sample", help: "decide = exact tiers, then the sampler; exact / sample = only those (default decide)" };
const F_DEADLINE: Flag = Flag { name: "--deadline-ms", arg: "N", help: "whole-call deadline (exact N/4, then sampler + polish); answer has `deadline` + `phases`" };
const F_VIEW: &[Flag] = &[F_PRETTY, F_PROGRESS, F_SUMMARY, F_TOP, F_PLAIN];
const EXIT_DECIDE: &str = "0 answer (exact | diagnostics_passed | partial), 1 infeasible (a proof), 2 bad input (one {\"error\"} object on stdout when stdout is not a terminal, or with --json) or bad flag (stderr), 3 refused / declined / non-finite result.";
const CONTROLS_NOTE: (&str, &str) = ("controls", "flag > PROBBIT_* environment > probbit.json > default.");
const F_P_SEED: Flag = Flag { name: "--seed", arg: "N", help: "the individual: an integer seed from 0 to 2^53 (default: the persona's seed)" };
const F_P_SCRIPT: Flag = Flag { name: "--script", arg: "JSON|FILE", help: "the turns: a JSON list of per-turn input objects, or {\"turns\": [...]}; inline or a file" };
const F_P_STATE: Flag = Flag { name: "--state", arg: "STATE", help: "the individual's stored state (`probbit persona init --out` writes one)" };
const F_P_INPUTS: Flag = Flag { name: "--inputs", arg: "JSON|FILE", help: "this turn's inputs: a JSON object, inline or a file (default {})" };
const F_P_TIMING: Flag = Flag { name: "--timing", arg: "", help: "add a separate, non-canonical `timing` object (compile, engine and decode ms)" };
const F_P_NO_INERTIA: Flag = Flag { name: "--no-inertia", arg: "", help: "moods carry nothing over from the previous turn (inertia 0)" };
const F_P_NEVER: Flag = Flag { name: "--never", arg: "RULE", help: "a rule in habit syntax, {when: {...}, then: {...}}: JSON or one line of the YAML subset" };
const F_P_PROPS: Flag = Flag { name: "--props", arg: "FILE", help: "rules from a file ({\"props\": [...]}, each with an optional id; YAML subset, or .json)" };
const F_P_SEEDS: Flag = Flag { name: "--seeds", arg: "LIST", help: "the individuals: 0-99 (default), 7, 1,4,9 or 0-9,20-29; seeds 0 to 2^53, at most 100000" };
const F_P_THREADS: Flag = Flag { name: "--threads", arg: "N", help: "search threads, 1 to 1024 (default: the logical CPUs)" };
const F_P_JSON: Flag = Flag { name: "--json", arg: "", help: "the JSON document instead of the readable report" };
const EXIT_PERSONA: &str = "0 done (every turn status, refusals and fallbacks included, is an answer; fuzz: no counterexample found; prove: every rule held or proved), 1 lint found an unresolved contradiction or (with rules) a counterexample, fuzz found a counterexample or prove left a rule unknown, 2 bad persona / state / inputs / script / rule (one {\"error\"} object on stdout, code \"persona\", when stdout is not a terminal or with --json) or bad flag (stderr), 3 lint (with rules): a rule the bound leaves unknown and the search cannot break (\"unproven\", not broken).";
const EXIT_LIVE: &str = "0 done (live: every event answered; verify: every line replays; control: the line appended), 1 verify: a line differs (its number and what) or, with --actions, a host action differs from its turn's released level, 2 a bad persona, state, event (one {\"error\"} object on stdout per bad event when stdout is not a terminal or with --json; the run goes on), control line or flag, 4 refused: the strand's writer lock is held by another writer, or the individual is paused or retired (one {\"error\"} object per refused event, as for 2; nothing is written), 5 verify: unsupported, a line has a field or line kind this engine does not know (a newer engine's strand: {\"ok\": false, \"line\", \"unsupported\": the field, \"engine\", \"strand_engine\"}; not verified, and not a divergence), 6 verify --chain: a previous segment file is missing ({\"ok\": false, \"missing\": its file name, \"segment_file\": the segment that links it}).";
pub(crate) static COMMANDS: &[Command] = &[
    Command { name: "persona", aliases: &[], group: "persona", summary: "the individuality layer: persona + state + inputs -> one stance",
        usage: &["persona SUB PERSONA [flags]   (SUB: init turn replay explain diff lint fuzz prove check compile describe)"],
        about: "The individuality layer (docs/persona.md): a persona file (YAML subset or JSON: traits with priors, moods with inertia, couplings, per-turn inputs, history, habits = hard rules) + an individual's state + this turn's inputs -> ONE probbit-ir program, run in process (`probbit run --op decide` at fixed work) -> the stance: a level per trait with exact odds, the habits in force and the ones that bound, a refusal when the engine cannot vouch, a why and a short stance line for any model's prompt.\nDocuments are canonical JSON (keys sorted); a turn is a pure function of (persona, state, inputs, version).",
        before: &[],
        flags: &[&[Flag { name: "--pretty", arg: "", help: "indent a single JSON document on stdout (init, turn, diff, lint, check, compile, describe, fuzz, prove); fuzz/prove --pretty implies --json. State files stay canonical. Replay stays JSONL; explain stays plain text." }]],
        subs: &[
            Sub { name: "init", args: "PERSONA [--seed N] [--out STATE]", help: "a new individual (genes from the seed, resting stance); stdout or --out",
                flags: &[F_P_SEED, Flag { name: "--out", arg: "STATE", help: "write the state to STATE instead of stdout" }] },
            Sub { name: "turn", args: "PERSONA --state STATE [--inputs JSON|FILE] [--out STATE2] [--timing] [--no-inertia]", help: "the stance on stdout; the new state replaces STATE (or goes to --out). With a needs block the inputs carry `needs` amounts ({\"needs\": {\"context\": {\"amount\": 0.6, \"src\": \"env:compactor\"}}}) and the stance adds `needs` (deficit, forecast, term per need) and `drives.window` (docs/persona.md §2.12); with a pace block the stance adds `pace` (level, rbar; §2.14) and with a surprise block `surprise` (each learned trait's gain above 1; §2.13)",
                flags: &[F_P_STATE, F_P_INPUTS, Flag { name: "--out", arg: "STATE2", help: "write the new state to STATE2 instead of replacing STATE" }, F_P_TIMING, F_P_NO_INERTIA] },
            Sub { name: "replay", args: "PERSONA [--seed N] --script JSON|FILE [--out TRACE] [--no-inertia] [--timing]", help: "init, then every turn of the script: one stance per line (JSONL)",
                flags: &[F_P_SEED, F_P_SCRIPT, Flag { name: "--out", arg: "TRACE", help: "write the JSONL trace to TRACE instead of stdout" }, F_P_NO_INERTIA, F_P_TIMING] },
            Sub { name: "explain", args: "PERSONA [--seed N] --script JSON|FILE --turn K", help: "turn K in words: every contribution, the odds, the twin",
                flags: &[F_P_SEED, F_P_SCRIPT, Flag { name: "--turn", arg: "K", help: "the turn to explain, counted from 0 (required)" }] },
            Sub { name: "diff", args: "PERSONA [--seed A] [--other PERSONA2] [--seed2 B] --script JSON|FILE", help: "distance between two individuals",
                flags: &[Flag { name: "--seed", arg: "A", help: "the first individual's seed (default: the persona's seed)" },
                    Flag { name: "--other", arg: "PERSONA2", help: "the second individual's persona (default: the same persona)" },
                    Flag { name: "--seed2", arg: "B", help: "the second individual's seed (default: its persona's seed)" }, F_P_SCRIPT] },
            Sub { name: "lint", args: "PERSONA [--never RULE | --props FILE] [--seeds 0-99] [--threads N] [--host MANIFEST [--strict]]",
                help: "contradicting habits (every conditional habit and pair, every prev level); warnings: planned levels that are not their trait's most likely one; with a needs block, `needs` notes: horizons whose forecast can pass K/2 before the deficit does, habits that exclude a bound goal; with a pace block, `pace`: the largest field shift pace can cause, with a warning over 1; with rules: prove each one, fuzz the unknown ones; exit 1 on a counterexample, exit 3 when a rule stays unknown and the search finds none (\"unproven\": not broken, not proved either)",
                flags: &[F_P_NEVER, F_P_PROPS, F_P_SEEDS, F_P_THREADS,
                    Flag { name: "--host", arg: "MANIFEST", help: "a host manifest ({\"host\": NAME, \"inputs\": [ids], \"senses\": [ids]}; examples/hosts/): `dead` lists each habit whose `when` needs an input or sense this host never supplies (a warning)" },
                    Flag { name: "--strict", arg: "", help: "with --host: a dead habit is an error (exit 1)" }] },
            Sub { name: "fuzz", args: "PERSONA (--never RULE | --props FILE) [--seeds 0-99] [--fuzz-seed N] [--scripts N] [--depth N] [--beam N] [--grid LIST] [--hours LIST] [--threads N] [--json] [--fixture-src env:synthetic|human:synthetic]",
                help: "search event scripts for the shortest one whose stance breaks a rule (habit syntax: {when: {...}, then: {...}}); shrunk (§5.6). --fixture-src explicitly authorizes synthetic-source fixtures for strict replay; never use them to relabel production events. The persona must allow that source.",
                flags: &[F_P_NEVER, F_P_PROPS, F_P_SEEDS,
                    Flag { name: "--fuzz-seed", arg: "N", help: "the search's own seed (default 0): the same inputs give the same report on any number of threads" },
                    Flag { name: "--scripts", arg: "N", help: "random scripts per individual before the beam, 0 to 1000000 (default 60)" },
                    Flag { name: "--depth", arg: "N", help: "events per script, 1 to 64 (default 8)" },
                    Flag { name: "--beam", arg: "N", help: "width of the odds-guided beam, 0 to 64 (default 4)" },
                    Flag { name: "--grid", arg: "LIST", help: "comma-separated fractions of a number input's max to try, 0 to 1 (default 0,0.5,1)" },
                    Flag { name: "--hours", arg: "LIST", help: "comma-separated idle hours to try between events (default 1,12,48)" },
                    F_P_THREADS, F_P_JSON,
                    Flag { name: "--fixture-src", arg: "env:synthetic|human:synthetic", help: "label exported counterexamples with that synthetic source so their replay commands run" }] },
            Sub { name: "prove", args: "PERSONA (--never RULE | --props FILE) [--seeds 0-99] [--threads N] [--json]",
                help: "per rule: held by construction (a habit implies it), proved for every event sequence (a sound bound), or unknown (the cell it fails; fuzz it) (§5.6)",
                flags: &[F_P_NEVER, F_P_PROPS, F_P_SEEDS, F_P_THREADS, F_P_JSON] },
            Sub { name: "check", args: "PERSONA", help: "valid? digest and sizes; with a bound block that declares max_epsilon, the certificate too (exit 1 when it fails)", flags: &[] },
            Sub { name: "bound", args: "PERSONA [--seeds LIST] [--json] [--audit STRAND]",
                help: "RFC 0004, read-only: per learned trait epsilon (no event sequence moves its odds at rest by more, in total variation: exact over the learned-weight box), the ceiling, the attaining level set and seed, speed, speed_floor, the residue r and every uncounted channel; exit 1 on an uncounted channel or an epsilon above max_epsilon. --audit STRAND replays a recorded life and reports per trait the room used (resting TV of the learned weights / epsilon), the most at any event and now; exit 1 on an exceedance",
                flags: &[F_P_SEEDS, F_P_JSON, Flag { name: "--audit", arg: "STRAND", help: "replay this strand and report the room each learned trait used" }] },
            Sub { name: "compile", args: "PERSONA --state STATE [--inputs JSON|FILE]", help: "the turn's probbit-ir program (its sha256 = engine.program)", flags: &[F_P_STATE, F_P_INPUTS] },
            Sub { name: "trajectory", args: "PERSONA (--state STATE [--inputs JSON|FILE] | --program FILE)", help: "RFC 0009: the turn's program (or a probbit-ir program) at K inverse temperatures beta0 -> 1, each step solved by the exact tier (engine.trajectory; default k 8, beta0 0.4): per step the marginals and their entropy",
                flags: &[F_P_STATE, F_P_INPUTS, Flag { name: "--program", arg: "FILE", help: "a probbit-ir program instead of the turn's" }] },
            Sub { name: "describe", args: "PERSONA", help: "traits, moods, inputs, habits, agenda, and the blocks it declares (learning, drives with windows and valence, senses, needs, surprise, pace with its closed form, the disposition genes)", flags: &[] },
        ],
        list_subs: false, shared: true,
        after: &[("scripts", "a script is a JSON list of per-turn input objects, or {\"turns\": [...]}. Resource controls as for run (PROBBIT_THREADS, ...)."), ("exit", EXIT_PERSONA)],
        examples: &["probbit persona init examples/persona/tutor.yaml --seed 2 --out pip.json",
            "probbit persona turn examples/persona/tutor.yaml --state pip.json --inputs '{\"loss\": true}'",
            "probbit persona lint examples/persona/tutor.yaml"],
        run: persona_cmd },
    Command { name: "live", aliases: &[], group: "live", summary: "a resident individual: JSONL events in, one stance per event out",
        usage: &["live PERSONA [--seed N | --state FILE] [--strand FILE [--label KEY=VALUE]...] [--events FILE [--watch]] [--clock real|fixed] [--checkpoint-every K] [--segment-every N]",
            "live PERSONA --demo week [--seed N] [--strand FILE [--label KEY=VALUE]...] [--plain]",
            "live verify STRAND [--from-checkpoint] [--chain] [--actions FILE] [--blocks FILE]",
            "live control STRAND pause|resume|retire|erase --by WHO --reason TEXT [--at TIME]",
            "live control STRAND reflect [--last N | --since-checkpoint]",
            "live control STRAND stir? [--idle-hours H]",
            "live control STRAND rest [--at TIME] [--state FILE]"],
        about: "A resident individual (docs/persona.md §5.7): JSONL events (one object of inputs per line) from --events FILE or stdin, one stance per event on stdout (canonical JSON). The clock stamps each event's elapsed_hours (quantised to 1e-6 h), so moods decay by their half-lives between events; feedback moves the learned deltas of a persona with a learning block (§2.8).\n--strand FILE logs the life: a header (persona document, initial state, engine version), then per event the inputs as used, the stance and state digests and the sha256 of the line before. `probbit live verify STRAND` replays it.",
        before: &[],
        flags: &[&[
            Flag { name: "--seed", arg: "N", help: "a new individual (default: the persona's seed)" },
            Flag { name: "--state", arg: "FILE", help: "a stored individual instead; rewritten after every event" },
            Flag { name: "--strand", arg: "FILE", help: "log the life to FILE: a new file gets the header; an existing strand is continued from --state (the state after its last line); a strand is never rewritten, just appended to" },
            Flag { name: "--events", arg: "FILE", help: "read events from FILE (default stdin)" },
            Flag { name: "--watch", arg: "", help: "follow --events FILE as lines are appended (tail -f); stops when the file is removed" },
            Flag { name: "--clock", arg: "real|fixed", help: "real (default): elapsed hours from a monotonic clock started with the run; fixed: each event carries its own elapsed_hours (default 0), so the run is a pure function of its events" },
            Flag { name: "--demo", arg: "week", help: "one individual's scripted week on the fixed clock (7 days, events hourly 09:00-15:00): praise for short answers moves the learned verbosity deltas to their cap, a quiet night relaxes the mood to its resting level, a campaign praising jokes raises humour while failure turns stay joke-free; a persona without a learning block gets the demo's (said in the opening line). Bars on stderr at a terminal, paced 1 s per hour (nights fast-forward in 2 s); otherwise one line per event on stdout, no waiting. With --seed, --strand" },
            F_PLAIN,
            Flag { name: "--checkpoint-every", arg: "K", help: "with --strand: a checkpoint line (the whole state) after every K-th event (default 1000; 0: none); `verify --from-checkpoint` and `monitor` start at the last one" },
            Flag { name: "--segment-every", arg: "N", help: "with --strand NAME.strand: seal the strand after every N-th event (a sealing checkpoint line) and go on in the next segment file NAME.0001.strand, NAME.0002.strand, ...: its header links the previous file (its name, the sealed line's sha256, the index) and its second line is the sealed line, so the chain continues across files; a run continues the newest segment (default 0: one file)" },
            Flag { name: "--label", arg: "KEY=VALUE", help: "with --strand, repeatable: a presentation label for the monitor (agent, host, lane, session, or a free key; at most 8 keys, values of 1 to 64 printable characters) merged into the strand's sidecar FILE.labels.json (FILE without its extension); the strand itself, its chain and its digests are not touched" },
        ]],
        subs: &[
            Sub { name: "verify", args: "STRAND [--from-checkpoint] [--chain] [--actions FILE] [--blocks FILE]", help: "replay a strand: the earliest line that differs (exit 1), or the earliest line with a field or line kind this engine does not know (`unsupported by this engine: FIELD`, exit 5: a newer engine's strand, not a divergence); from the header (every checkpoint checked), or from the last checkpoint; with --actions, also the earliest turn whose host action differs from the level its stance released",
                flags: &[Flag { name: "--from-checkpoint", arg: "", help: "start at the last checkpoint instead of the header" },
                    Flag { name: "--chain", arg: "", help: "a segment (section 5.7): verify it and walk back through the previous segment files to the first, each verified and its last line the sealed line the next one links; exit 1 at the first break (the segment file named), 6 when a previous segment file is missing" },
                    Flag { name: "--actions", arg: "FILE", help: "the host's action log, one {\"turn\": T, \"action\": {trait: level}} per line: compared with each turn's released levels (summary `actions`: checked, differs); exit 1 when one differs" },
                    Flag { name: "--blocks", arg: "FILE", help: "write each event's block (the text a host injects, RFC 0008 E: every trait and mood as `name level`, then the viability line) to FILE, one {\"block\", \"n\"} per line; replays from the header" }] },
            Sub { name: "control", args: "STRAND pause|resume|retire|erase --by WHO --reason TEXT [--at TIME] | STRAND seal | STRAND reflect [--last N | --since-checkpoint] | STRAND stir? [--idle-hours H] | STRAND rest [--at TIME] [--state FILE]",
                help: "pause, resume or retire an individual, or erase its learned weights (RFC 0004: every learned weight back to 0, the status unchanged); appends a control line under the lock: while paused or retired every event is refused (exit 4); no credit crosses a control line; retire is final; `verify` replays them and reports the status. `seal` (section 5.7) seals the newest segment of STRAND: refused unless its last line is an event line, it appends the sealing checkpoint and writes the next segment file (its header links the sealed line, then the sealed line itself), and prints that file in `file`",
                flags: &[Flag { name: "--by", arg: "WHO", help: "who decided it: human:ID or env:ID (required, except for seal)" },
                    Flag { name: "--reason", arg: "TEXT", help: "why (required, except for seal)" },
                    Flag { name: "--at", arg: "TIME", help: "when (default: now, UTC)" },
                    Flag { name: "--last", arg: "N", help: "reflect: the last N events (default: every event)" },
                    Flag { name: "--since-checkpoint", arg: "", help: "reflect: the events after the last checkpoint line" },
                    Flag { name: "--idle-hours", arg: "H", help: "stir? (RFC 0016): the hours since the strand's last event (default 0); the answer is {due, stir}, nothing is written" },
                    Flag { name: "--state", arg: "FILE", help: "rest (RFC 0015): the stored individual (the state after the strand's last line), rewritten after the pass" }] },
        ],
        list_subs: true, shared: false,
        after: &[("the safety kit", "(docs/persona.md §5.7) one writer per strand: `--strand` takes STRAND.lock (a second writer exits 4 and changes nothing; a lock whose process is gone is taken over). With `reward_from` in the persona (§2.10), a reward from `src: self`, without a src or from an undeclared one is refused like a bad event."),
            ("exit", EXIT_LIVE)],
        examples: &["probbit live examples/persona/tutor.yaml --seed 2 --demo week",
            "probbit live examples/persona/tutor.yaml --clock fixed --strand pip.strand < events.jsonl",
            "probbit live verify pip.strand"],
        run: live_cmd },
    Command { name: "check", aliases: &[], group: "persona", summary: "one CI gate: prove + fuzz a persona's rules, replay its strands (exit 0 / 1 / 3)",
        usage: &["check PERSONA [--props FILE] [--strand FILE]... [--seeds LIST] [--depth N] [--hours LIST] [--json]"],
        about: "The rule lock in one command, for CI: vet the rules a persona must keep before its first task and after every policy update, then replay the receipts. It runs `probbit persona lint`'s pipeline (contradicting habits; with --props, prove each rule over the individuals, then fuzz the ones the bound leaves unknown, the rules' and habits' thresholds on the grid) and `probbit live verify` on every --strand (each strand replays against the persona in its own header).\nOne line per rule (held = proved for every event sequence, held_by_construction = a habit implies it, broken seed N = the search found a counterexample, unproven = not proved and no counterexample found), one line per strand (ok N events, or FAIL and the first line that differs), then `check: held H · broken B · unproven U · strands S/T`.",
        before: &[],
        flags: &[&[
            F_P_PROPS,
            Flag { name: "--strand", arg: "FILE", help: "a strand to replay (`probbit live --strand` writes one); give it once per strand" },
            F_P_SEEDS,
            Flag { name: "--depth", arg: "N", help: "events per fuzzed script, 1 to 64 (default 8)" },
            Flag { name: "--hours", arg: "LIST", help: "comma-separated idle hours the fuzzer tries between events (default 1,12,48)" },
            Flag { name: "--json", arg: "", help: "one document: {persona, props, seeds, depth, hours, rules (the persona lint document), strands [{path, ok, events, final_state, error?, unsupported?, strand_engine?}], verdict {held, broken, unproven, conflicts, strands_ok, strands_total, exit}}" },
        ]],
        subs: &[], list_subs: false, shared: false,
        after: &[("exit", "0 every rule held and every strand replays; 1 a rule broken, an unresolved habit contradiction, a strand that fails or a strand this engine cannot verify (unsupported, `live verify` exit 5: its line says so) (1 wins over 3); 2 a bad persona, rule file or flag (one {\"error\"} object on stdout, code \"persona\", when stdout is not a terminal or with --json); 3 the only problem is an unproven rule.")],
        examples: &["probbit check examples/persona/dispatch-policy.yaml --props examples/persona/dispatch-props.json",
            "probbit check agent.yaml --props rules.yaml --strand day1.strand --strand day2.strand --json > check.json"],
        run: check_cmd },
    Command { name: "monitor", aliases: &[], group: "monitor", summary: "an individual's internal state, live: terminal bars or a page on 127.0.0.1",
        usage: &["monitor STRAND [--follow] [--once] [--plain] [--fps N] [--serve] [--open] [--port N]",
            "monitor STRAND [--label KEY=VALUE]... [STRAND [--label KEY=VALUE]...]... [--dir DIR] [--serve] [--open] [--port N]",
            "monitor --demo [week|drives] [--once] [--plain] [--serve] [--open] [--port N]"],
        about: "Watch an individual's internal state (docs/persona.md §5.8): replays the strand (`probbit live --strand` writes it) with the rules of `probbit live verify`, recomputing every stance document from the strand alone, and draws the latest event as horizontal bars: the stance (each trait's levels with their reported odds, the level taken highlighted, the phrase it says), the moods with a sparkline of the last 50 events, the senses (the event's inputs, the history features), the habits in force and the ones that bound, the learned deltas, the drives when the documents carry them, and the stance line.\nIt changes no file (--demo: its week goes through a temporary one, removed at once) and sends nothing anywhere.",
        before: &[("modes", "one frame on stdout, then exit (--once; the default without --follow) · the terminal, redrawn as the strand grows (--follow) · a page in the browser (--serve; --open also opens it).")],
        flags: &[&[
            Flag { name: "--follow", arg: "", help: "keep watching: lines appended to the strand are replayed and drawn within a second (a line counts once its newline is written); a truncated or rotated strand is replayed from the start, with a warning; Ctrl-C quits" },
            Flag { name: "--once", arg: "", help: "one frame on stdout, then exit (the default without --follow)" },
            Flag { name: "--plain", arg: "", help: "plain ASCII, no colour (NO_COLOR=1: no colour; PROBBIT_THEME=plain or TERM=dumb: as --plain)" },
            Flag { name: "--fps", arg: "N", help: "with --follow, at most N redraws per second (1-60, default 10)" },
            Flag { name: "--serve", arg: "", help: "the page instead of the terminal, following the strand (or looping the selected synthetic demo): its URL http://127.0.0.1:PORT/ is the line on stdout. GET / the page (one file; it fetches no fonts, styles or scripts), /events its server-sent events (the layout, the latest frame, then a frame per event, a heartbeat every 15 s; events that land within one 100 ms poll, or while a page falls behind, come as the latest frame), /doc/N event N's stance document, as `probbit live` printed it (the latest 10000 are kept), /strands the strands served (id, file name, labels, latest event and glyph, seconds since the file was written); with several strands (or --dir) /s/ID/events and /s/ID/doc/N are strand ID's, /events and /doc/N the first's, and /?strand=ID opens the page on ID. It binds 127.0.0.1 and no other address (there is no --host), answers requests addressed to 127.0.0.1 or localhost, and sends nothing anywhere; Ctrl-C quits" },
            Flag { name: "--open", arg: "", help: "serve the page (as --serve does) and open it in the default browser (BROWSER if set; else open on macOS, start on Windows, xdg-open elsewhere); a browser that does not start fails nothing" },
            Flag { name: "--port", arg: "N", help: "with --serve or --open, the port (default 0: a free one)" },
            Flag { name: "--demo", arg: "[week|drives]", help: "synthetic data, never a live individual (default week); see demo below" },
            Flag { name: "--label", arg: "KEY=VALUE", help: "after a STRAND, repeatable, with --serve or --open: a label for that strand's identity header (agent, host, lane, session, or a free key), over the ones in its sidecar STRAND.labels.json (STRAND without its extension); nothing is written" },
            Flag { name: "--dir", arg: "DIR", help: "with --serve or --open: every *.strand in DIR (files added while serving are picked up within a second), each labelled by DIR/NAME.labels.json; with two or more strands the page has a switcher, and /s/ID/events, /s/ID/doc/N serve strand ID (its file name without the extension)" },
        ]],
        subs: &[], list_subs: false, shared: false,
        after: &[("demo", "the tutor's week is the one `probbit live examples/persona/tutor.yaml --seed 2 --demo week` writes, replayed in memory: at a terminal or with --serve paced 1 s per hour, each night in 2 s (under a minute); otherwise, or with --once, its last frame. `--demo drives` plays a short goal-signal scenario with wanting, afterglow and prediction errors. The page labels each loop and its restart; Ctrl-C quits."),
            ("exit", "0 every line replays, 1 a line differs (the frame names it and shows the event before it), 2 a bad flag, a port that cannot be had, or a file that cannot be read or is not a strand.")],
        examples: &["probbit monitor --demo --open", "probbit monitor pip.strand --follow", "probbit monitor pip.strand --serve --port 8080"],
        run: monitor::cmd },
    Command { name: "explain", aliases: &[], group: "live", summary: "explain one logged event: inputs, stance, binding habits and replay result",
        usage: &["explain STRAND --event N [--json]"],
        about: "Read-only replay through event N (1-based strand event number): its logged inputs and flags, elapsed hours, the previous and recomputed stance with odds, binding habits and their constraints, previous hash -> this hash, and recomputes or differs. The first event has initial levels but no previous odds. Later lines are not verified; use live verify for the whole strand. Habits constrain the stance, not the model's words or actions.",
        before: &[],
        flags: &[&[
            Flag { name: "--event", arg: "N", help: "required: the 1-based event number in the strand, not its file line or persona turn" },
            Flag { name: "--json", arg: "", help: "structured explanation instead of plain English" },
        ]],
        subs: &[], list_subs: false, shared: false,
        after: &[("exit", "0 recomputes, 1 differs (including a broken prefix), 2 bad flag, missing file, out-of-range event or malformed input, 5 unsupported field or line kind.")],
        examples: &["probbit explain pip.strand --event 2", "probbit explain pip.strand --event 2 --json"],
        run: explain::cmd },
    Command { name: "reflect", aliases: &[], group: "live", summary: "RFC 0013 self-report: a canonical account of a strand's last N events, from its lines alone",
        usage: &["reflect STRAND [--last N | --since-checkpoint] [--json | --text]"],
        about: "The self-report (docs/persona.md §5.8): a canonical document computed from the strand's lines alone (no turn is run, nothing is solved, nothing is random): over the window, the inputs set, the expectation's surprises (each receipt's z recomputed from the logged receipts; the K = 5 largest |z| with the predicted and measured values), per trait the levels taken and the odds' drift, the moods' path with its turning points, the drives' peaks and the habits that bound (read from the checkpoint lines in the window: a window without one reports them as null), the credit settled, the learned weights' net movement and the control lines; every number at 6 decimals, `digest` = the sha256 of the canonical text, and `summary`: 6 to 12 sentences from fixed templates. `probbit live control STRAND reflect` appends it as a line (0.14.0 strands), which `live verify` recomputes.",
        before: &[],
        flags: &[&[
            Flag { name: "--last", arg: "N", help: "the last N events (default: every event)" },
            Flag { name: "--since-checkpoint", arg: "", help: "the events after the last checkpoint line (that checkpoint is the first sample)" },
            Flag { name: "--json", arg: "", help: "the canonical document (the default)" },
            Flag { name: "--text", arg: "", help: "the summary's sentences only, one per line" },
        ]],
        subs: &[], list_subs: false, shared: false,
        after: &[("exit", "0 the reflection printed, 2 a bad flag, a file that cannot be read, or a strand with no event line in the window (one {\"error\"} object on stdout).")],
        examples: &["probbit reflect pip.strand --last 50 --text", "probbit reflect pip.strand --since-checkpoint"],
        run: reflect_cmd },
    Command { name: "decide", aliases: &[], group: "decide/run", summary: "route tasks to workers: a router document in, a JSON decision out",
        usage: &["decide [flags] < router.json"],
        about: "Route tasks to workers: a router document (workers + caps, tasks + allowed + scores, affinity; README) on stdin, a JSON decision on stdout. Exact where structure allows, else sampled with the diagnostics gate.",
        before: &[], flags: &[F_SEARCH, F_CONTROLS, &[Flag { name: "--mode", arg: "auto|exact|sample", help: "which tiers may answer (default auto)" }, F_MAX_INPUT], F_VIEW], subs: &[], list_subs: false, shared: false,
        after: &[CONTROLS_NOTE, ("exit", EXIT_DECIDE)],
        examples: &["probbit demo --tasks 300 | probbit decide --summary --pretty", "probbit decide --sweeps 6400 --polish-ms 0 < router.json"],
        run: decide_cmd },
    Command { name: "run", aliases: &[], group: "decide/run", summary: "run a probbit-ir program (docs/probbit-ir-json.md), a JSON answer out",
        usage: &["run [flags] < program.json"],
        about: "Run a probbit-ir program (variables, values, scores, rules; docs/probbit-ir-json.md) on stdin; JSON answer on stdout.",
        before: &[], flags: &[F_SEARCH, F_CONTROLS, &[F_OP, F_DEADLINE, F_MAX_INPUT], F_VIEW], subs: &[], list_subs: false, shared: false,
        after: &[CONTROLS_NOTE, ("exit", EXIT_DECIDE)],
        examples: &["probbit run --pretty < examples/knapsack-20.json", "probbit run --deadline-ms 50 < program.json"],
        run: run_cmd },
    Command { name: "evaluate", aliases: &[], group: "decide/run", summary: "the decision-API adapter: request + judge's answers + rules -> joint answer",
        usage: &["evaluate [flags] < request.json"],
        about: "The decision-API adapter: a System One request (questions noul | choice | score) plus a \"probbit\" block (the judge's answers, rules over question ids) on stdin; the rule-abiding joint answer in the judge's response shape, odds per question and the gate's verdict on stdout. Every `probbit run` flag applies. docs/probbit-ir-json.md \"Decision API\".",
        before: &[], flags: &[F_SEARCH, F_CONTROLS, &[F_OP, F_DEADLINE, F_MAX_INPUT], F_VIEW,
            &[Flag { name: "--program", arg: "", help: "print the compiled probbit-ir program instead of running it (`probbit evaluate --program | probbit run` = the same answer)" }]],
        subs: &[], list_subs: false, shared: false, after: &[CONTROLS_NOTE, ("exit", EXIT_DECIDE)],
        examples: &["probbit evaluate --pretty < examples/evaluate/support-12.json"],
        run: evaluate_cmd },
    Command { name: "demo", aliases: &[], group: "decide/run", summary: "emit an agent-routing problem as JSON (--live: watch it get routed)",
        usage: &["demo [flags] > router.json"],
        about: "Emit a synthetic agent-routing document (JSON) for `probbit decide`.",
        before: &[], flags: &[&[Flag { name: "--tasks", arg: "N", help: "tasks to generate, 1..3333333 (default 12)" },
            Flag { name: "--seed", arg: "N", help: "random seed (default 7)" },
            Flag { name: "--hard", arg: "", help: "tight quotas + strong affinity" },
            Flag { name: "--live", arg: "", help: "the router story on stderr: exact tiers, then a live field of p-bits from the real chains, the gate, the verdict (default when stdout and stderr are both terminals; --tasks defaults to 300; stdout, if not a terminal, gets the JSON problem)" },
            F_PLAIN]],
        subs: &[], list_subs: false, shared: false, after: &[], examples: &["probbit demo --tasks 300 --hard > router.json", "probbit demo --live"],
        run: demo_cmd },
    Command { name: "ir", aliases: &[], group: "decide/run", summary: "a router document as probbit-ir v0 text (the hardware-facing form)",
        usage: &["ir [--max-input-mb N] [--plain] < router.json"],
        about: "Print a router document as probbit-ir v0 text (the hardware-facing form).",
        before: &[], flags: &[&[F_MAX_INPUT, F_PLAIN]], subs: &[], list_subs: false, shared: false, after: &[], examples: &["probbit demo | probbit ir"],
        run: ir_cmd },
    Command { name: "mcp", aliases: &[], group: "mcp", summary: "a Model Context Protocol server on stdio: the commands as agent tools",
        usage: &["mcp"],
        about: "A Model Context Protocol server on stdio (JSON-RPC 2.0, one message per line; logs on stderr). Tools probbit_decide, probbit_run, probbit_stats, probbit_demo, probbit_evaluate, probbit_persona_init, probbit_persona_turn, probbit_persona_fuzz, probbit_live_event, probbit_live_verify: the commands' own JSON in and out. Exits when stdin closes. docs/agents.md.",
        before: &[], flags: &[], subs: &[], list_subs: false, shared: false, after: &[], examples: &["claude mcp add probbit -- probbit mcp"],
        run: mcp_cmd },
    Command { name: "stats", aliases: &[], group: "other", summary: "the spec sheet: machine, build, controls + source, measured updates/s",
        usage: &["stats [flags]"],
        about: "The processor's spec sheet: machine, build, effective controls + their source, measured updates/s.",
        before: &[], flags: &[F_CONTROLS, &[Flag { name: "--sweeps", arg: "N", help: "sweeps of the throughput measurement (default 2000)" }, F_PRETTY, F_PLAIN]],
        subs: &[], list_subs: false, shared: false, after: &[CONTROLS_NOTE], examples: &["probbit stats --pretty"],
        run: stats_cmd },
    Command { name: "help", aliases: &[], group: "other", summary: "this screen, or a command's or subcommand's help (as COMMAND --help)",
        usage: &["help [COMMAND [SUBCOMMAND]]"],
        about: "Print the help of a command or subcommand, the same text as `probbit COMMAND [SUBCOMMAND] --help`; alone, the same as `probbit --help`.",
        before: &[], flags: &[], subs: &[], list_subs: false, shared: false, after: &[], examples: &["probbit help persona fuzz"],
        run: help_cmd },
    Command { name: "completions", aliases: &[], group: "other", summary: "a completion script for bash, zsh or fish: commands, subcommands, flags",
        usage: &["completions bash|zsh|fish"],
        about: "Print a static completion script for the shell: the commands, subcommands and flags (with their help where the shell shows it), generated from the same table as this help, so it lists exactly what the parser accepts. Regenerate it after an upgrade.",
        before: &[], flags: &[], subs: &[], list_subs: false, shared: false,
        after: &[("install", "bash: probbit completions bash > ~/.local/share/bash-completion/completions/probbit\nzsh: probbit completions zsh > ~/.zfunc/_probbit, with fpath=(~/.zfunc $fpath) before compinit in ~/.zshrc\nfish: probbit completions fish > ~/.config/fish/completions/probbit.fish"),
            ("exit", "0 the script, 2 no shell or an unknown one (the list of shells on stderr).")],
        examples: &["probbit completions zsh > ~/.zfunc/_probbit", "source <(probbit completions bash)"],
        run: completions_cmd },
    Command { name: "man", aliases: &[], group: "other", summary: "the manual page (roff), generated from this table: probbit man | man -l -",
        usage: &["man"],
        about: "Print the manual page in roff (man(7) macros): NAME, SYNOPSIS, DESCRIPTION, COMMANDS (every command, subcommand and flag of this help), EXIT STATUS, ENVIRONMENT, EXAMPLES, SEE ALSO. The release archives carry it as docs/probbit.1 and install.sh installs it to ~/.local/share/man/man1/, or to <prefix>/share/man/man1/ when PROBBIT_INSTALL_DIR is <prefix>/bin.",
        before: &[], flags: &[], subs: &[], list_subs: false, shared: false, after: &[],
        examples: &["probbit man | man -l -", "probbit man > ~/.local/share/man/man1/probbit.1"],
        run: man_cmd },
    Command { name: "version", aliases: &["--version", "-V"], group: "other", summary: "print the version (also --version, -V)",
        usage: &["version"], about: "Print the version.",
        before: &[], flags: &[], subs: &[], list_subs: false, shared: false, after: &[], examples: &[],
        run: version_cmd },
];
/// The command `name` names (a spelling in `aliases` too)
pub(crate) fn command(name: &str) -> Option<&'static Command> { COMMANDS.iter().find(|c| c.name == name || c.aliases.contains(&name)) }
/// (flags taking a value, switches) of a flag list; `check_flags` enforces exactly these
fn split_flags<'a>(fs: impl Iterator<Item = &'a Flag>) -> (Vec<&'static str>, Vec<&'static str>) {
    let (mut v, mut s) = (vec![], vec![]); for f in fs { if f.takes_value() { v.push(f.name) } else { s.push(f.name) } } (v, s)
}
/// A command's own flags (not its subcommands')
fn flags_of(cmd: &str) -> (Vec<&'static str>, Vec<&'static str>) { split_flags(command(cmd).map_or(&[][..], |c| c.flags).iter().flat_map(|g| g.iter())) }
/// A subcommand's flags, the command's shared ones included (persona's --pretty); None = no such subcommand
fn sub_flags(cmd: &str, sub: &str) -> Option<(Vec<&'static str>, Vec<&'static str>)> {
    let c = command(cmd)?; let s = c.sub(sub)?; Some(split_flags(c.sub_flags(s)))
}
/// `--collective on|off` (default on, R19 P1.1): the sampler's global two-value flip (`Problem::collective` / `Model::collective`).
fn collective_arg(args: &[String]) -> bool {
    match arg(args, "--collective", "on".to_string()).as_str() { "on" => true, "off" => false, _ => fail("--collective must be on or off") }
}
/// `--cluster on|off` (DEFAULT ON since R19.4, P1.4: the frozen-threshold holdout's rare-modes family gave 8/8 false whole
/// answers without it and 0/20 with it; R19 P1.1(d)): the sampler's Wolff cluster move (`Problem::cluster` / `Model::cluster`).
fn cluster_arg(args: &[String]) -> bool {
    match arg(args, "--cluster", "on".to_string()).as_str() { "on" => true, "off" => false, _ => fail("--cluster must be on or off") }
}
/// `--cycles on|off` (default on since R19.5, P1.4 calibration): the sampler's three-cycle rotations (`Problem::cycles` /
/// `Model::cycles`; attempted only with k > 2 values and caps).
fn cycles_arg(args: &[String]) -> bool {
    match arg(args, "--cycles", "on".to_string()).as_str() { "on" => true, "off" => false, _ => fail("--cycles must be on or off") }
}
/// Unknown arguments exit 2. They were ignored: a typo (`--budgetms 5`), or `"--budget-ms 50"` passed as ONE argument by
/// a shell that did not split it (zsh `$var`), ran the whole decision on defaults without a word. `values` take the next argument,
/// `switches` none; `--progress` takes an optional number.
/// A value flag given twice exits 2 (it used the first value and never read the second: `--budget-ms 50
/// --budget-ms nan` ran with exit 0). `--progress` too (its optional value was first-wins: `--progress 1000 --progress 10`
/// printed 0 lines in 300 ms, the reverse order 23).
fn check_flags(args: &[String], values: &[&str], switches: &[&str]) {
    let mut i = 1; let mut seen: Vec<&str> = Vec::new();
    while i < args.len() { let a = args[i].as_str();
        if values.contains(&a) { if args.get(i + 1).map_or(true, |v| v.starts_with("--")) { fail(&format!("{a} needs a value")) }
            if seen.contains(&a) { fail(&format!("{a} given twice")) } seen.push(a); i += 2; }
        else if a == "--progress" && switches.contains(&a) { if seen.contains(&a) { fail(&format!("{a} given twice")) } seen.push(a);
            i += if args.get(i + 1).is_some_and(|v| v.parse::<u64>().is_ok()) { 2 } else { 1 }; }
        else if switches.contains(&a) || a == "--json" { i += 1; } // --json: every command (the error channel, `JSON_FLAG`)
        else { fail(&format!("unknown argument {a:?} for `probbit {}` (see `probbit {} --help`)", args[0], args[0])) } }
}
/// Optional config file: `$PROBBIT_CONFIG`, else `./probbit.json` if present (keys chains, threads, cpu_limit, mem_limit_mb, priority).
fn config_path() -> Option<String> { std::env::var("PROBBIT_CONFIG").ok().or_else(|| std::path::Path::new("probbit.json").exists().then(|| "probbit.json".to_string())) }
const CONFIG_KEYS: [&str; 5] = ["chains", "threads", "cpu_limit", "mem_limit_mb", "priority"];
fn config() -> Option<json::Json> {
    let path = config_path()?;
    let src = std::fs::read_to_string(&path).unwrap_or_else(|e| fail(&format!("config {path}: {e}")));
    let c = json::parse(&src).unwrap_or_else(|e| fail(&format!("config {path}: {}", e.msg)));
    // An unknown key (`{"thread": 3}`) or a non-object was ignored silently: exit 2, as for the input documents' unknown fields
    match &c { Json::Obj(kv) => if let Some((k, _)) = kv.iter().find(|(k, _)| !CONFIG_KEYS.contains(&k.as_str())) { fail(&format!("config {path}: unknown key \"{k}\" (known: {})", CONFIG_KEYS.join(", "))) },
        _ => fail(&format!("config {path}: must be a JSON object")) }
    Some(c)
}
/// The config file in the telemetry of `decide` and `run` (absent without one): it can change the answer of an otherwise
/// identical command (`chains` is part of the determinism key), and nothing in the output said it was there.
pub(crate) fn config_echo() -> Vec<(&'static str, Json)> { config_path().map_or(vec![], |p| vec![("config", jstr(&p))]) }
/// A config-file value as text (numbers must be whole; anything else fails the caller's parse).
fn cfg_val(key: &str) -> Option<String> {
    let c = config()?; let v = c.get(key)?;
    Some(if let Some(x) = v.as_f64() { if x.fract() == 0.0 && x >= 0.0 { format!("{}", x as u64) } else { format!("{x}") } } else { v.as_str().map_or("?".into(), str::to_string) })
}
/// Processor-style control: CLI flag > `PROBBIT_*` environment variable > config file > default; must be in 1..=max.
fn ctl(args: &[String], name: &str, env: &str, default: usize, max: usize) -> usize { ctl_in(args, name, env, default, 1, max) }
fn ctl_min(args: &[String], name: &str, env: &str, default: usize, min: usize) -> usize { ctl_in(args, name, env, default, min, usize::MAX) }
fn ctl_in(args: &[String], name: &str, env: &str, default: usize, min: usize, max: usize) -> usize {
    let v = args.iter().position(|a| a == name).and_then(|k| args.get(k + 1)).map(|v| v.to_string()).or_else(|| std::env::var(env).ok())
        .or_else(|| cfg_val(&name[2..].replace('-', "_")));
    match v { None => default, Some(v) => match v.parse::<usize>() { Ok(n) if n >= min && n <= max => n,
        _ => fail(&if max == usize::MAX { format!("{name} / {env} must be an integer >= {min}") } else { format!("{name} / {env} must be an integer from {min} to {max}") }) } }
}
/// Upper bounds of --chains and --threads (from any source): `--chains 576460752303423488` aborted (`capacity overflow`, exit 134)
/// and `--chains 1000000000` exhausted memory. 100,000 chains is the largest count measured (BENCHMARKS §6); the sampler uses
/// at most `chains` threads, so 1,024 covers any machine.
const MAX_CHAINS: usize = 100_000;
const MAX_THREADS: usize = 1024;
/// The default memory cap (MB). Unbounded, sample memory grew ~190 MB per second of budget on the 300-task demo,
/// so a long `--budget-ms` could exhaust a small machine; 1024 MB never thins below ~8 s there (inferred from rows per chain).
const DEFAULT_MEM_LIMIT_MB: usize = 1024;
/// `--mem-limit-mb N` (+ PROBBIT_MEM_LIMIT_MB, config key mem_limit_mb; default now 1024, 0 = unbounded) caps the sample
/// buffers that grow with the budget (per-chain trajectory n x 2 bytes + trace 8 bytes per kept sweep) at N MB in total by thinning
/// (`probbit_ir::record_row`), never below 64 rows per chain. Fixed costs (program, JSON, marginals) come on top. -> (N, rows per chain)
fn mem_limit(args: &[String], chains: usize, n_vars: usize) -> (usize, usize) {
    let mb = ctl_min(args, "--mem-limit-mb", "PROBBIT_MEM_LIMIT_MB", DEFAULT_MEM_LIMIT_MB, 0);
    (mb, if mb == 0 { 0 } else { chains.checked_mul(2 * n_vars + 8).map_or(64, |d| (mb.saturating_mul(1_048_576) / d).max(64)) })
}
/// `--progress [MS]` monitor: a thread prints one JSONL line to stderr every MS ms (default 100) while the decision runs:
/// {"event":"progress","ms","sweeps","site_updates_per_s","process_cpu_ms","peak_rss_mb"} (sweeps = all chains so far, counted in
/// steps of 64; updates/s = since the previous line). Stdout is unchanged. Stopped (and joined) when dropped; exit paths skip it.
struct Monitor { tx: Option<std::sync::mpsc::Sender<()>>, h: Option<std::thread::JoinHandle<()>> }
impl Drop for Monitor { fn drop(&mut self) { drop(self.tx.take()); if let Some(h) = self.h.take() { let _ = h.join(); } } }
fn monitor(args: &[String], n_vars: usize) -> Option<Monitor> {
    // `--progress 0` used to become 100 silently (and `--progress 1.5` too; check_flags now rejects it as an unknown argument)
    let every: u64 = match args.get(args.iter().position(|a| a == "--progress")? + 1).and_then(|v| v.parse::<u64>().ok()) { Some(0) => fail("--progress MS must be >= 1"), Some(v) => v, None => 100 };
    use std::sync::atomic::Ordering::Relaxed;
    probbit_ir::PROGRESS_SWEEPS.store(0, Relaxed); probbit_ir::PROGRESS_ON.store(true, Relaxed);
    let (tx, rx) = std::sync::mpsc::channel::<()>();
    let h = std::thread::spawn(move || { let t0 = std::time::Instant::now(); let (mut lt, mut ls) = (0.0f64, 0u64);
        while let Err(std::sync::mpsc::RecvTimeoutError::Timeout) = rx.recv_timeout(std::time::Duration::from_millis(every)) {
            let (t, s) = (t0.elapsed().as_secs_f64(), probbit_ir::PROGRESS_SWEEPS.load(Relaxed)); let u = sys::usage();
            let line = obj(vec![("event", jstr("progress")), ("ms", num((t * 1e3).round())), ("sweeps", num(s as f64)), ("site_updates_per_s", num(if t > lt { ((s - ls) as f64 * n_vars as f64 / (t - lt)).round() } else { 0.0 })),
                ("process_cpu_ms", u.map_or(Json::Null, |u| num(u.0.round()))), ("peak_rss_mb", u.map_or(Json::Null, |u| num(u.1)))]);
            err_line(&json::write(&line, false)); (lt, ls) = (t, s); } });
    Some(Monitor { tx: Some(tx), h: Some(h) })
}
/// `--max-input-mb N` (decide, run, ir; default 256, 0 = no limit): the largest stdin document read. Parsing costs up to ~9x
/// the input in memory (a 200 MB numeric array peaked at 1.78 GB), so a larger input is a `limit` error before it is parsed.
const DEFAULT_MAX_INPUT_MB: usize = 256;
/// stdin -> text, or `bad_input`: ONE error object on stdout, exit 2. An unreadable stdin (`probbit decide < /`) and invalid UTF-8
/// went to `fail` (a stderr line, stdout empty), although both are document content, not flags.
fn read_stdin(args: &[String]) -> String {
    let mb: usize = arg(args, "--max-input-mb", DEFAULT_MAX_INPUT_MB); let cap = mb.saturating_mul(1_048_576); let mut b = Vec::new();
    if let Err(e) = std::io::stdin().lock().take(if mb == 0 { u64::MAX } else { (cap as u64).saturating_add(1) }).read_to_end(&mut b) {
        bad_input("", json::schema("", format!("cannot read stdin: {e}"))) }
    if mb > 0 && b.len() > cap { bad_input("bad input: ", json::limit("", format!("stdin holds more than --max-input-mb {mb} MB"))) }
    // One leading UTF-8 byte-order mark is skipped (RFC 8259 lets a parser ignore it): Windows PowerShell 5.1 adds one to every
    // pipe into a native program, so `probbit demo | probbit decide` exited 2 there (`unexpected character` at byte 0)
    if b.starts_with(&[0xEF, 0xBB, 0xBF]) { b.drain(..3); }
    String::from_utf8(b).unwrap_or_else(|e| bad_input("", json::schema("", format!("bad JSON: invalid UTF-8 at byte {}", e.utf8_error().valid_up_to()))))
}
/// --priority low|normal (flag > PROBBIT_PRIORITY > config > normal), applied before any chain thread starts.
fn priority(args: &[String]) {
    match args.iter().position(|a| a == "--priority").and_then(|k| args.get(k + 1)).cloned().or_else(|| std::env::var("PROBBIT_PRIORITY").ok()).or_else(|| cfg_val("priority")).as_deref() {
        None | Some("normal") => {}, Some("low") => { if sys::set_low().is_none() { fail("--priority low: setpriority failed"); } },
        Some(_) => fail("--priority / PROBBIT_PRIORITY must be low or normal") }
}
/// The processor controls shared by `probbit decide` and `probbit run`: (chains, threads, cpu_limit_pct).
/// R19.8: `--threads` also bounds the gate's per-chain passes (`probbit_ir::GATE_THREADS`; one thread per chain before).
fn controls(args: &[String]) -> (usize, usize, u32) {
    let r = (ctl(args, "--chains", "PROBBIT_CHAINS", 4, MAX_CHAINS), ctl(args, "--threads", "PROBBIT_THREADS", std::thread::available_parallelism().map_or(1, |n| n.get()).min(4), MAX_THREADS),
     { let c = ctl(args, "--cpu-limit", "PROBBIT_CPU_LIMIT", 100, usize::MAX); if c > 100 { fail("--cpu-limit / PROBBIT_CPU_LIMIT must be 1..=100"); } c as u32 });
    probbit_ir::GATE_THREADS.store(r.1, std::sync::atomic::Ordering::Relaxed); r
}

fn decide_cmd(args: &[String]) {
    let (v, w) = flags_of("decide"); check_flags(args, &v, &w);
    let budget: f64 = budget_ms(args); let seed: u64 = arg(args, "--seed", 7);
    let exact_limit: u64 = arg(args, "--exact-limit", 2_000_000); let polish_ms: f64 = polish_ms_arg(args);
    // Any other --mode value (`foo`, `EXACT`, or `--pretty` swallowed as the value) used to run the sampler silently (exit 0)
    let mode: String = arg(args, "--mode", "auto".to_string()); let view = View::new(args, "decide");
    if !["auto", "exact", "sample"].contains(&mode.as_str()) { fail("--mode must be auto, exact or sample") }
    // --polish-sweeps N = fixed-work polish (deterministic); parsed here, so a bad value exits 2 before sampling
    let polish_sweeps: usize = arg(args, "--polish-sweeps", 0);
    let fr_states: usize = arg(args, "--frontier-states", FRONTIER_MAX_STATES); let sweeps: usize = arg(args, "--sweeps", 0);
    let (chains, threads, cpu_pct) = controls(args); priority(args); let top = top_wanted(args);
    let j = read_doc(args);
    let mut n = from_json(&j).unwrap_or_else(|e| bad_input("bad problem: ", e)); n.p.collective = collective_arg(args); n.p.cluster = cluster_arg(args); n.p.cycles = cycles_arg(args); let p = &n.p;
    let _mon = monitor(args, p.t);
    if let Some(th) = top { tui::tier("exact tiers"); tui::top_start(th, tui::TopSpec { cmd: "decide", vars: p.t, chains, threads: threads.clamp(1, chains), budget_ms: budget, sweeps, deadline_ms: None }); }
    let o = router::Opts { budget, seed, exact_limit, polish_ms, polish_sweeps, fr_states, sweeps, mode, chains, threads, cpu_pct, xms: exact_ms(args) };
    let (doc, code, bars) = router::decide(&n, &o, &|| mem_limit(args, chains, p.t));
    let c = finish(doc, code, &view, &bars); if c != 0 { std::process::exit(c); }
}
/// `--top` asked for and drawable (a terminal on stderr, not plain); exit 2 with `--progress`, which writes the same stream.
fn top_wanted(args: &[String]) -> Option<theme::Theme> {
    if !args.iter().any(|a| a == "--top") { return None; }
    if args.iter().any(|a| a == "--progress") { fail("--top and --progress both write to stderr; use one"); }
    theme::stderr(args)
}

/// Where a control's value comes from (same precedence as `ctl`): "flag" > "env" > "file" > "default".
fn source(args: &[String], name: &str, env: &str) -> &'static str {
    if args.iter().any(|a| a == name) { "flag" } else if std::env::var(env).is_ok() { "env" } else if cfg_val(&name[2..].replace('-', "_")).is_some() { "file" } else { "default" }
}
/// The SIMD features this binary was built with (the ones the kernels can use), in `probbit stats` order.
pub(crate) fn target_features() -> Vec<&'static str> {
    [("sse4.2", cfg!(target_feature = "sse4.2")), ("avx", cfg!(target_feature = "avx")), ("avx2", cfg!(target_feature = "avx2")), ("fma", cfg!(target_feature = "fma")),
        ("bmi2", cfg!(target_feature = "bmi2")), ("avx512f", cfg!(target_feature = "avx512f")), ("neon", cfg!(target_feature = "neon")), ("dotprod", cfg!(target_feature = "dotprod"))]
        .iter().filter(|f| f.1).map(|f| f.0).collect()
}
/// The self-test program of `probbit stats` and the hero screen: an `n`-spin ring with a weak field (probbit-ir JSON).
pub(crate) fn ring(n: usize) -> String {
    format!("{{\"probbit_ir\": 1, \"values\": [\"-\", \"+\"], \"vars\": [{}], \"pairs\": [{}]}}",
        (0..n).map(|i| format!("{{\"id\": \"s{i}\", \"h\": {{\"+\": {}}}}}", 0.1 * ((i % 7) as f64 - 3.0))).collect::<Vec<_>>().join(","),
        (0..n).map(|i| format!("{{\"i\": \"s{i}\", \"j\": \"s{}\", \"table\": [[0.4, -0.4], [-0.4, 0.4]]}}", (i + 1) % n)).collect::<Vec<_>>().join(","))
}
/// Rows per chain under the default memory cap (`mem_limit` without flag, variable or config)
pub(crate) fn default_max_rows(chains: usize, n_vars: usize) -> usize { chains.checked_mul(2 * n_vars + 8).map_or(64, |d| (DEFAULT_MEM_LIMIT_MB.saturating_mul(1_048_576) / d).max(64)) }
/// `probbit stats` — the processor's spec sheet: machine, build target features, effective controls and where each came
/// from, and a measured self-test (400-spin ring, 4 chains x `--sweeps` fixed work, IR sampler) at 1 thread and at the effective
/// thread count, after one discarded warm-up pass. Nothing is applied (e.g. --priority low is reported, not set).
fn stats_cmd(args: &[String]) {
    let (v, w) = flags_of("stats"); check_flags(args, &v, &w);
    let (chains, threads, cpu) = controls(args); let sweeps: usize = arg(args, "--sweeps", 2000).max(1);
    let prio = args.iter().position(|a| a == "--priority").and_then(|k| args.get(k + 1)).cloned().or_else(|| std::env::var("PROBBIT_PRIORITY").ok()).or_else(|| cfg_val("priority")).unwrap_or_else(|| "normal".into());
    if !["low", "normal"].contains(&prio.as_str()) { fail("--priority / PROBBIT_PRIORITY must be low or normal") } // a bad value used to be echoed
    let cores = std::thread::available_parallelism().map_or(1, |n| n.get());
    let feats: Vec<Json> = target_features().into_iter().map(jstr).collect();
    let n = 400; let prog = run::from_json(&json::parse(&ring(n)).unwrap()).unwrap();
    let rate = |t: usize| { let t0 = std::time::Instant::now(); let s = probbit_ir::sample_on(&prog.m, chains, t, sweeps, None, 1, false, cpu as u32, 0).unwrap();
        (s.sweeps as f64 * n as f64 / t0.elapsed().as_secs_f64()).round() };
    rate(threads); let (r1, rt) = (rate(1), rate(threads)); // first pass discarded: page faults + clock ramp made 1 thread look slow (4.27x on 4 threads)
    let ctl_obj = |v: Json, name: &str, env: &str| obj(vec![("value", v), ("source", jstr(source(args, name, env)))]);
    let doc = obj(vec![("engine", jstr(&format!("probbit {VERSION}"))),
        ("machine", obj(vec![("os", jstr(std::env::consts::OS)), ("arch", jstr(std::env::consts::ARCH)), ("logical_cpus", num(cores as f64))])),
        ("build", obj(vec![("target_features", Json::Arr(feats)), ("debug_assertions", Json::Bool(cfg!(debug_assertions)))])),
        ("controls", obj(vec![("chains", ctl_obj(num(chains as f64), "--chains", "PROBBIT_CHAINS")), ("threads", ctl_obj(num(threads as f64), "--threads", "PROBBIT_THREADS")),
            ("cpu_limit_pct", ctl_obj(num(cpu as f64), "--cpu-limit", "PROBBIT_CPU_LIMIT")), ("priority", ctl_obj(jstr(&prio), "--priority", "PROBBIT_PRIORITY")),
            ("mem_limit_mb", ctl_obj(num(ctl_min(args, "--mem-limit-mb", "PROBBIT_MEM_LIMIT_MB", DEFAULT_MEM_LIMIT_MB, 0) as f64), "--mem-limit-mb", "PROBBIT_MEM_LIMIT_MB")),
            ("config_file", config_path().map_or(Json::Null, |f| jstr(&f)))])),
        ("self_test", obj(vec![("program", jstr("400-spin ring, IR sampler, fixed work")), ("chains", num(chains as f64)), ("sweeps_per_chain", num(sweeps as f64)),
            ("site_updates_per_s_1_thread", num(r1)), ("threads", num(threads.clamp(1, chains) as f64)), ("site_updates_per_s", num(rt)), ("speedup", num((rt / r1 * 100.0).round() / 100.0))])),
        ("process_cpu_ms", sys::usage().map_or(Json::Null, |u| num(u.0))), ("peak_rss_mb", sys::usage().map_or(Json::Null, |u| num(u.1)))]);
    emit(&json::write(&doc, args.iter().any(|a| a == "--pretty")));
}

/// The agent-routing demo as a JSON problem: agent tasks x workers under PII / prod-DB policy, quotas and same-workflow affinity.
/// Stdout: the document (`demo_doc`). With `--live`, or by default when stdout and stderr are both terminals, the router story
/// runs on stderr (`tui::live`; --tasks defaults to 300) and stdout gets the same document only if it is not a terminal.
fn demo_cmd(args: &[String]) {
    let (v, w) = flags_of("demo"); check_flags(args, &v, &w);
    let seed: u64 = arg(args, "--seed", 7); let hard = args.iter().any(|a| a == "--hard"); let live = args.iter().any(|a| a == "--live");
    let th = theme::stderr(args); let auto = !live && th.is_some() && theme::stdout_is_terminal();
    let tasks: usize = arg(args, "--tasks", if live || auto { 300 } else { 12 });
    let max_tasks = json::MAX_DENSE / 6; // the generated router has six workers
    if tasks == 0 || tasks > max_tasks { fail(&format!("--tasks must be 1 to {max_tasks} (the router's task-worker limit)")); }
    let doc = demo_doc(tasks, seed, hard);
    if !(live || auto) { emit(&json::write(&doc, true)); return; }
    let Some(th) = th else { err_line("probbit: --live draws on a colour terminal (stderr); no terminal, or NO_COLOR / --plain / PROBBIT_THEME=plain: printed the problem only");
        emit(&json::write(&doc, true)); return };
    let mut n = from_json(&doc).unwrap_or_else(|e| bad_input("bad problem: ", e)); n.p.collective = true; n.p.cluster = true; n.p.cycles = true; // decide's defaults
    tui::live(th, &tui::DemoRun { doc: &doc, p: &n.p, tasks: &n.tasks, workers: &n.workers, seed, hard });
    if !theme::stdout_is_terminal() { emit(&json::write(&doc, true)); }
}

/// `probbit run` on a parsed program (also `probbit evaluate`'s compiled one): the flags, the instruction, `phases` appended.
/// -> (document, exit code, per-variable error bars for `--summary`)
fn run_program(args: &[String], mut p: run::Prog, cmd: &'static str, tp: std::time::Instant, parse_ms: f64, compile_ms: f64) -> (Json, i32, Vec<f64>) {
    p.m.collective = collective_arg(args); p.m.cluster = cluster_arg(args); p.m.cycles = cycles_arg(args);
    priority(args); let (chains, threads, cpu_pct) = controls(args); let top = top_wanted(args);
    let op: String = arg(args, "--op", "decide".to_string()); if !["decide", "exact", "sample"].contains(&op.as_str()) { fail("--op must be decide, exact or sample"); }
    let mon = monitor(args, p.m.n); let dl = deadline_ms(args);
    if dl.is_some() && arg::<usize>(args, "--sweeps", 0) > 0 { fail("--deadline-ms needs a wall-clock budget: drop --sweeps"); }
    if let Some(th) = top { tui::tier("exact tiers"); tui::top_start(th, tui::TopSpec { cmd, vars: p.m.n, chains, threads: threads.clamp(1, chains), budget_ms: budget_ms(args), sweeps: arg(args, "--sweeps", 0), deadline_ms: dl }); }
    let mut bars = vec![];
    let (mut doc, code) = run::run(&p, &op, budget_ms(args), arg(args, "--seed", 7), arg(args, "--exact-limit", 2_000_000), polish_ms_arg(args), arg(args, "--polish-sweeps", 0), arg(args, "--sweeps", 0), arg(args, "--frontier-states", probbit_ir::FRONTIER_MAX_STATES),
        chains, threads, cpu_pct, mem_limit(args, chains, p.m.n), exact_ms(args), dl.map(|d| (tp, d, args.iter().any(|a| a == "--budget-ms"))), &mut bars); drop(mon); run::phases(&mut doc, parse_ms, compile_ms, dl);
    (doc, code, bars)
}
/// `probbit evaluate`: a System One request + `probbit` block on stdin -> the compiled program (`evaluate::compile`) -> `probbit run`'s
/// instruction with every `run` flag -> the System-One-shaped answer plus the probbit document (`evaluate::respond`). `--program`
/// prints the compiled probbit-ir program instead (`probbit evaluate --program | probbit run` gives the same answer).
fn evaluate_cmd(args: &[String]) {
    let (v, w) = flags_of("evaluate"); check_flags(args, &v, &w); let tp = std::time::Instant::now(); let j = read_doc(args);
    let tc = std::time::Instant::now(); let parse_ms = (tc - tp).as_secs_f64() * 1e3;
    let r = evaluate::compile(&j).unwrap_or_else(|e| bad_input("bad request: ", e));
    if args.iter().any(|a| a == "--program") { emit(&json::write(&r.program, args.iter().any(|a| a == "--pretty"))); return; }
    let p = run::from_json(&r.program).unwrap_or_else(|e| bad_input("bad request: ", evaluate::locate(e))); let compile_ms = tc.elapsed().as_secs_f64() * 1e3;
    let (doc, code, bars) = run_program(args, p, "evaluate", tp, parse_ms, compile_ms);
    let c = finish(evaluate::respond(&r, doc), code, &View::new(args, "evaluate"), &bars); if c != 0 { std::process::exit(c); }
}

/// The persona engine of this process: `persona::run_program` with the resource controls `probbit run` would use (flag > env >
/// config > default; resolved once), and --priority low applied if asked for. They never change an answer at fixed work.
pub(crate) fn persona_engine() -> impl Fn(&Json, &persona::Flags) -> Json {
    let a: Vec<String> = ["persona", "--chains", "4"].iter().map(|s| s.to_string()).collect(); // chains come from the persona (the prototype passes them as a flag)
    priority(&a); let (_, threads, cpu) = controls(&a); let mem = ctl_min(&a, "--mem-limit-mb", "PROBBIT_MEM_LIMIT_MB", DEFAULT_MEM_LIMIT_MB, 0);
    move |prog: &Json, f: &persona::Flags| persona::run_program(prog, f, threads, cpu, mem)
}
/// `--inputs` / `--script` / `--state`: a file, else the argument itself as JSON
fn json_arg(s: &str, what: &str) -> Json {
    let t = if std::path::Path::new(s).is_file() { std::fs::read_to_string(s).unwrap_or_else(|e| bad_input("persona: ", persona::perr(what, format!("cannot read {s}: {e}")))) } else { s.to_string() };
    json::parse(t.strip_prefix('\u{feff}').unwrap_or(&t)).unwrap_or_else(|e| bad_input("persona: ", persona::perr(what, format!("not JSON: {}", e.msg))))
}
/// Human-readable JSON without changing the canonical values, sorted keys, digests or stored state bytes.
fn persona_text(doc: &Json, pretty: bool) -> String {
    let canonical = persona::canon(doc);
    if pretty { json::write(&json::parse(&canonical).expect("canonical persona JSON"), true) } else { canonical }
}
/// Write a document: to `path` (write, then rename: a killed turn never leaves a half-written state), or to stdout
fn put(path: Option<&str>, text: &str) {
    let Some(path) = path else { emit(text); return };
    let tmp = format!("{path}.tmp-{}", std::process::id());
    if let Err(e) = std::fs::write(&tmp, format!("{text}\n")).and_then(|_| std::fs::rename(&tmp, path)) { let _ = std::fs::remove_file(&tmp); fail(&format!("persona: cannot write {path}: {e}")) }
}
/// A --seed: an integer 0..=2^53
fn seed_arg(args: &[String], name: &str) -> Option<u64> {
    args.iter().any(|a| a == name).then(|| { let s: u64 = arg(args, name, 0); if s > 1 << 53 { fail(&format!("{name} must be an integer from 0 to 2^53")) } s })
}
/// A list flag: comma-separated numbers from `lo` to `hi`
fn list_arg(args: &[String], name: &str, default: &[f64], lo: f64, hi: f64) -> Vec<f64> {
    let Some(k) = args.iter().position(|a| a == name) else { return default.to_vec() };
    let v = args.get(k + 1).map_or("", String::as_str);
    let xs: Option<Vec<f64>> = v.split(',').map(|x| x.trim().parse::<f64>().ok().filter(|x| x.is_finite() && *x >= lo && *x <= hi)).collect();
    match xs { Some(x) if !x.is_empty() => x, _ => fail(&format!("{name}: comma-separated numbers from {lo} to {hi}, e.g. {}", default.iter().map(|x| x.to_string()).collect::<Vec<_>>().join(","))) }
}
/// `--seeds 0-99` / `3` / `1,4,9` / `0-9,20-29`: the individuals, in the order given, each once (default 0-99)
fn seeds_arg(args: &[String]) -> Vec<u64> {
    let Some(k) = args.iter().position(|a| a == "--seeds") else { return (0..100).collect() };
    let v = args.get(k + 1).map_or("", String::as_str); let bad = || -> ! { fail(&format!("--seeds: cannot read {v:?} (e.g. 0-99, 7, or 1,4,9; seeds 0 to 2^53, at most 100000)")) };
    fuzz::seeds(v).unwrap_or_else(|| bad())
}
/// `probbit persona fuzz PERSONA (--never RULE | --props FILE) [flags]` (docs/persona.md §5.6): exit 0 nothing found, 1 a
/// counterexample, 2 bad input
/// `--never RULE` / `--props FILE` -> the rules (fuzz, prove, lint); a bad rule is an error object, exit 2
fn rules_arg(args: &[String], p: &persona::Persona) -> Vec<persona::Prop> {
    let opt = |f: &str| -> Option<String> { args.iter().position(|x| x == f).and_then(|k| args.get(k + 1)).cloned() };
    let mut props = vec![];
    // a rule on the command line: JSON, or one line of the YAML subset (a flow mapping, read as the value of a key)
    if let Some(t) = opt("--never") { let d = json::parse(&t).or_else(|_| persona::parse_doc(&format!("never: {t}"), false).map(|d| d.get("never").cloned().unwrap_or(Json::Null)))
            .unwrap_or_else(|e| bad_input("persona: ", persona::perr("--never", e)));
        props = persona::props(p, &d, "--never", "never").unwrap_or_else(|e| bad_input("persona: ", e)); }
    if let Some(f) = opt("--props") {
        let t = std::fs::read_to_string(&f).unwrap_or_else(|e| bad_input("persona: ", persona::perr("--props", format!("cannot read {f}: {e}"))));
        let d = persona::parse_doc(&t, f.ends_with(".json")).unwrap_or_else(|e| bad_input("persona: ", persona::perr("--props", e)));
        for q in persona::props(p, &d, "--props", "prop1").unwrap_or_else(|e| bad_input("persona: ", e)) {
            if props.iter().any(|x: &persona::Prop| x.id == q.id) { bad_input("persona: ", persona::perr("--props", format!("property id {} is given twice", q.id))) }
            props.push(q); } }
    props
}
fn fuzz_cmd(args: &[String], path: &str, p: &persona::Persona, eng: fuzz::SyncEngine) {
    let props = rules_arg(args, p);
    let pretty = args.iter().any(|a| a == "--pretty");
    let sub = args[1].as_str();
    if props.is_empty() { fail(&format!("persona {sub}: give a rule: --never RULE (habit syntax) or --props FILE")) }
    let bounded = |f: &str, d: usize, lo: usize, hi: usize| -> usize { let x: usize = arg(args, f, d); if x < lo || x > hi { fail(&format!("{f} must be {lo} to {hi}")) } x };
    let cores = std::thread::available_parallelism().map_or(1, |n| n.get());
    if sub == "prove" { let (seeds, threads) = (seeds_arg(args), bounded("--threads", cores, 1, 1024));
        let t0 = std::time::Instant::now(); let out = fuzz::prove(p, &props, &seeds, threads, eng); let secs = t0.elapsed().as_secs_f64();
        if pretty || args.iter().any(|a| a == "--json") { emit(&persona_text(&fuzz::prove_doc(p, &props, &seeds, &out), pretty)) } else { emit(&fuzz::prove_human(p, path, &props, &seeds, &out)) }
        err_line(&format!("prove: {secs:.2} s"));
        if out.iter().any(|v| matches!(v, fuzz::Verdict::Unknown { .. })) { std::process::exit(1) }
        return; }
    let s = fuzz::Search { seeds: seeds_arg(args), fuzz_seed: arg(args, "--fuzz-seed", 0u64), scripts: bounded("--scripts", 60, 0, 1_000_000), depth: bounded("--depth", 8, 1, 64),
        beam: bounded("--beam", 4, 0, 64), grid: fuzz::grid(p, &props, &list_arg(args, "--grid", &[0.0, 0.5, 1.0], 0.0, 1.0)), hours: list_arg(args, "--hours", &[1.0, 12.0, 48.0], 0.0, 1e6), threads: bounded("--threads", cores, 1, 1024) };
    let t0 = std::time::Instant::now(); let (mut res, turns) = fuzz::fuzz(p, &props, &s, eng); let secs = t0.elapsed().as_secs_f64();
    if let Some(i) = args.iter().position(|a| a == "--fixture-src") {
        fuzz::authorize_fixtures(p, &s.seeds, &mut res, &args[i + 1], eng).unwrap_or_else(|e| bad_input("persona fuzz: ", e));
    }
    if pretty || args.iter().any(|a| a == "--json") { emit(&persona_text(&fuzz::doc(p, path, &props, &s, &res, turns), pretty)) } else { emit(&fuzz::human(p, path, &props, &s, &res, turns)) }
    err_line(&format!("fuzz: {turns} turns in {secs:.2} s ({:.0} turns/s, {} search thread{})", turns as f64 / secs.max(1e-9), s.threads.min(s.seeds.len().max(1)), if s.threads.min(s.seeds.len().max(1)) == 1 { "" } else { "s" }));
    if res.iter().any(|per| per.iter().any(Option::is_some)) { std::process::exit(1) }
}
/// `probbit live PERSONA [--seed N | --state FILE] [--strand FILE] [--events FILE [--watch]] [--clock real|fixed] [--checkpoint-every K]`;
/// `probbit live PERSONA --demo week`; `probbit live verify STRAND [--from-checkpoint] [--actions FILE]`; `probbit live control STRAND pause|resume|retire|erase --by WHO --reason TEXT`
fn live_cmd(args: &[String]) {
    // §5.8: `--label k=v` (repeatable) goes to the strand's sidecar, outside the strand and both digests
    let (args, labels) = labels::take(args, "live"); let args = &args[..];
    if !labels.is_empty() && matches!(args.get(1).map(String::as_str), Some("verify" | "control")) { fail("live: --label labels a strand being written: probbit live PERSONA --strand FILE --label KEY=VALUE") }
    if !labels.is_empty() && !args.iter().any(|a| a == "--strand") { fail("live: --label goes with --strand FILE (the labels are written next to the strand, in FILE's .labels.json)") }
    let engine = persona_engine(); let eng: persona::Engine = &engine;
    let opt = |f: &str| -> Option<String> { args.iter().position(|x| x == f).and_then(|k| args.get(k + 1)).cloned() };
    // an error object on stdout, a line on stderr, then the exit: 4 for a lock held by another writer or a paused / retired individual
    let refuse = |what: &str, e: json::InErr| -> ! { err_line(&format!("probbit: {what}{}: {}", e.path, e.msg)); error_object(&e.to_json());
        exit(if ["locked", "paused", "retired"].contains(&e.code) { 4 } else { 2 }) };
    if args.get(1).map(String::as_str) == Some("verify") {
        let mut a = vec!["live verify".to_string()]; a.extend(args.iter().skip(3).cloned()); let (v, w) = sub_flags("live", "verify").unwrap(); check_flags(&a, &v, &w);
        let Some(path) = args.get(2).filter(|a| !a.starts_with("--")) else { fail("live verify: the strand file: probbit live verify STRAND") };
        let text = std::fs::read_to_string(path).unwrap_or_else(|e| fail(&format!("live verify: cannot read {path}: {e}")));
        // a strand that does not verify: the earliest line that differs (exit 1), or the earliest line with a field or line kind
        // this engine does not know (exit 5: unsupported, the strand of a newer engine; not verified, and not a divergence)
        let failed = |line: usize, why: &str| -> ! {
            let doc = live::failure(&text, line, why);
            if live::unsupported_field(why).is_some() {
                let by = doc.get("strand_engine").and_then(Json::as_str).map_or_else(|| "the header names no engine".to_string(), |e| format!("written by {e}"));
                err_line(&format!("probbit: live verify: line {line}: {why} ({by}; this engine is {}): not verified, and not a divergence", live::engine()));
                emit(&persona::canon(&doc)); exit(5) }
            emit(&persona::canon(&doc)); exit(1) };
        // --actions (docs/persona.md §5.7): the host's action log against the released levels, replayed from the header
        if let Some(af) = opt("--actions") {
            if args.iter().any(|a| a == "--from-checkpoint") { fail("live verify: --actions replays from the header: drop --from-checkpoint") }
            let at = std::fs::read_to_string(&af).unwrap_or_else(|e| fail(&format!("live verify: cannot read {af}: {e}")));
            let acts = live::read_actions(&at).unwrap_or_else(|e| bad_input("live verify: ", e));
            let turns: Vec<u64> = acts.iter().map(|a| a.turn).collect();
            match live::verify_keep(&text, eng, &turns) {
                Ok((mut doc, kept, p)) => { let (checked, d) = live::first_difference(&p, &acts, &kept).unwrap_or_else(|e| bad_input("live verify: ", e)); let differs = d.is_some();
                    if let Json::Obj(v) = &mut doc { v.push(("actions".into(), obj(vec![("checked", num(checked as f64)), ("differs", d.unwrap_or(Json::Null))]))); }
                    emit(&persona::canon(&doc)); if differs { std::process::exit(1) } }
                Err((line, why)) => failed(line, &why) }
            return;
        }
        // --blocks (RFC 0008 E): the block every replayed stance renders, one line per event, for persona without a block key too
        if let Some(bf) = opt("--blocks") {
            if args.iter().any(|a| a == "--from-checkpoint") { fail("live verify: --blocks replays from the header: drop --from-checkpoint") }
            match live::verify_blocks(&text, eng) {
                Ok((doc, blocks)) => { let body: String = blocks.iter().map(|(n, b)| format!("{}\n", persona::canon(&obj(vec![("n", num(*n as f64)), ("block", Json::Str(b.clone()))])))).collect();
                    std::fs::write(&bf, body).unwrap_or_else(|e| fail(&format!("live verify: cannot write {bf}: {e}"))); emit(&persona::canon(&doc)) }
                Err((line, why)) => failed(line, &why) }
            return;
        }
        // --chain (§5.7, segments): this segment, then each previous one back to the first, linked by the sealed line
        if args.iter().any(|a| a == "--chain") {
            if args.iter().any(|a| a == "--from-checkpoint") { fail("live verify: --chain replays every segment from its start: drop --from-checkpoint") }
            let dir = std::path::Path::new(path).parent().map(std::path::Path::to_path_buf).unwrap_or_default();
            let name = |p: &str| std::path::Path::new(p).file_name().map_or_else(|| p.to_string(), |f| f.to_string_lossy().into_owned());
            let (mut cur, mut t, mut segs, mut newest) = (name(path), text.clone(), 0u64, None::<Json>);
            let broke = |file: &str, d: Json, code: i32| -> ! { let mut d = d; if let Json::Obj(v) = &mut d { v.push(("segment_file".into(), Json::Str(file.to_string()))); }
                emit(&persona::canon(&d)); exit(code) };
            loop {
                match live::verify(&t, eng) {
                    Ok(d) => { if newest.is_none() { newest = Some(d); } segs += 1; }
                    Err((line, why)) => { err_line(&format!("probbit: live verify --chain: {cur}: line {line}: {why}"));
                        broke(&cur, live::failure(&t, line, &why), if live::unsupported_field(&why).is_some() { 5 } else { 1 }) } }
                let h = json::parse(live::lines(&t).first().copied().unwrap_or("")).unwrap_or(Json::Null);
                let Some(sg) = h.get("segment").cloned() else { break };
                let (prev, sealed, index) = (sg.get("previous").and_then(Json::as_str).unwrap_or("").to_string(), sg.get("sealed").and_then(Json::as_str).unwrap_or("").to_string(), sg.get("index").and_then(Json::as_f64).unwrap_or(0.0));
                let pt = match std::fs::read_to_string(dir.join(&prev)) { Ok(x) => x,
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => { err_line(&format!("probbit: live verify --chain: {cur} links {prev}, which does not exist: the chain before it is not verified"));
                        broke(&cur, obj(vec![("ok", Json::Bool(false)), ("missing", Json::Str(prev.clone()))]), 6) }
                    Err(e) => fail(&format!("live verify --chain: cannot read {prev}: {e}")) };
                let pls = live::lines(&pt); let last = pls.last().copied().unwrap_or("");
                let pidx = json::parse(pls.first().copied().unwrap_or("")).ok().and_then(|h| h.get("segment").and_then(|s| s.get("index")).and_then(Json::as_f64)).unwrap_or(0.0);
                let why = if persona::digest_of(last) != sealed { Some(format!("its last line is not the sealed line {cur} links")) }
                    else if pidx + 1.0 != index { Some(format!("its segment index is not {} (the one before {cur}'s)", index - 1.0)) } else { None };
                if let Some(why) = why { err_line(&format!("probbit: live verify --chain: {prev}: line {}: {why}", pls.len()));
                    broke(&prev, obj(vec![("ok", Json::Bool(false)), ("line", num(pls.len() as f64)), ("diverges", Json::Str(why))]), 1) }
                cur = prev; t = pt;
            }
            let mut d = newest.unwrap_or(Json::Null); if let Json::Obj(v) = &mut d { v.push(("segments".into(), num(segs as f64))); }
            emit(&persona::canon(&d)); return;
        }
        let res = if args.iter().any(|a| a == "--from-checkpoint") { live::verify_from_checkpoint(&text, eng) } else { live::verify(&text, eng) };
        match res {
            Ok(doc) => emit(&persona::canon(&doc)),
            Err((line, why)) => failed(line, &why) }
        return;
    }
    if args.get(1).map(String::as_str) == Some("control") {
        let mut a = vec!["live control".to_string()]; a.extend(args.iter().skip(4).cloned()); let (v, w) = sub_flags("live", "control").unwrap(); check_flags(&a, &v, &w);
        let usage = "live control: probbit live control STRAND pause|resume|retire|erase --by human:ID --reason TEXT (or STRAND seal, STRAND rest)";
        let (Some(path), Some(what)) = (args.get(2).filter(|a| !a.starts_with("--")), args.get(3).filter(|a| !a.starts_with("--"))) else { fail(usage) };
        // reflect (RFC 0013): the reflection of the lines so far, appended as one line (0.14.0 strands)
        if what == "reflect" {
            if let Some(f) = ["--by", "--reason", "--at"].iter().find(|f| args.iter().any(|a| a == *f)) { fail(&format!("live control STRAND reflect: {f} does not apply (a reflection is computed, not decided by someone)")) }
            match live::reflect_cmd(path, reflect_window(args, "live control")) { Ok(doc) => emit(&persona::canon(&doc)), Err(e) => refuse("live control: ", e) }
            return;
        }
        if args.iter().any(|a| a == "--last" || a == "--since-checkpoint") { fail("live control: --last and --since-checkpoint go with reflect") }
        // stir? (RFC 0016): is a self-started idle event due, exactly from the replayed strand and the idle hours; writes nothing
        if what == "stir?" {
            if let Some(f) = ["--by", "--reason", "--at"].iter().find(|f| args.iter().any(|a| a == *f)) { fail(&format!("live control STRAND stir?: {f} does not apply (a query writes nothing)")) }
            let idle = match opt("--idle-hours") { None => 0.0, Some(x) => x.parse::<f64>().ok().filter(|h| h.is_finite() && *h >= 0.0).unwrap_or_else(|| fail("live control STRAND stir?: --idle-hours H, a number >= 0")) };
            match live::stir_cmd(path, idle, eng) { Ok(doc) => emit(&persona::canon(&doc)), Err(e) => refuse("live control: ", e) }
            return;
        }
        if args.iter().any(|a| a == "--idle-hours") { fail("live control: --idle-hours goes with stir?") }
        if what != "rest" && args.iter().any(|a| a == "--state") { fail("live control: --state goes with rest") }
        // rest (RFC 0015): the rest pass on the replayed strand, one consolidation line (0.14.0 strands with expectation.rest)
        if what == "rest" {
            if let Some(f) = ["--by", "--reason"].iter().find(|f| args.iter().any(|a| a == *f)) { fail(&format!("live control STRAND rest: {f} does not apply (a rest pass is a host's timing, not a control by someone)")) }
            match live::rest_cmd(path, &opt("--at").unwrap_or_else(live::utc_now), opt("--state").as_deref(), eng) { Ok(doc) => emit(&persona::canon(&doc)), Err(e) => refuse("live control: ", e) }
            return;
        }
        // seal (§5.7): no control line, so no --by, --reason or --at; the sealing checkpoint and the next segment file
        if what == "seal" {
            if let Some(f) = ["--by", "--reason", "--at"].iter().find(|f| args.iter().any(|a| a == *f)) { fail(&format!("live control STRAND seal: {f} does not apply (a seal writes a checkpoint line, not a control line)")) }
            match live::seal_cmd(path, eng) { Ok(doc) => emit(&persona::canon(&doc)), Err(e) => refuse("live control: ", e) }
            return;
        }
        let (Some(by), Some(reason)) = (opt("--by"), opt("--reason")) else { fail(&format!("{usage} (--by and --reason are required)")) };
        match live::control_cmd(path, what, &by, &reason, &opt("--at").unwrap_or_else(live::utc_now)) { Ok(doc) => emit(&persona::canon(&doc)), Err(e) => refuse("live control: ", e) }
        return;
    }
    let Some(path) = args.get(1).filter(|a| !a.starts_with("--")) else { fail("live: the persona file goes before the flags: probbit live PERSONA [flags] (or probbit live verify STRAND, probbit live control STRAND ...)") };
    // `probbit live verfy x.strand`: no persona file of that name, and a subcommand close to it
    if !std::path::Path::new(path).exists() { let c = command("live").unwrap(); if usage::suggest(path, &c.subs.iter().map(|s| s.name).collect::<Vec<_>>()).is_some() { unknown(Some(c), path) } }
    let mut a = vec!["live".to_string()]; a.extend(args.iter().skip(2).cloned()); let (v, w) = flags_of("live"); check_flags(&a, &v, &w);
    let (p, doc) = persona::load(path).unwrap_or_else(|e| bad_input("live: ", e));
    if let Some(d) = opt("--demo") {
        if d != "week" { fail(&format!("live: --demo week, not {d:?}")) }
        if let Some(f) = ["--state", "--events", "--clock", "--watch", "--checkpoint-every"].iter().find(|f| args.iter().any(|a| a == *f)) { fail(&format!("live --demo week: {f} does not apply (the demo is its own events on the fixed clock)")) }
        // the bars on stderr at a colour terminal; the plain lines on stdout unless it is that terminal too
        let th = theme::stderr(args); let lines = th.is_none() || !theme::stdout_is_terminal();
        // the week's strand is written under the writer lock too
        let lock = opt("--strand").map(|f| live::Lock::take(&f).unwrap_or_else(|e| refuse("live: ", e)));
        if let Some(f) = opt("--strand").filter(|_| !labels.is_empty()) { labels::merge_into(&f, &labels).unwrap_or_else(|e| fail(&format!("live: {e}"))); }
        live::week(&doc, seed_arg(args, "--seed"), opt("--strand").as_deref(), th, &mut |l: &str| if lines { emit(l) }, eng).unwrap_or_else(|e| bad_input("live: ", e));
        drop(lock); return;
    }
    let every: u64 = arg(args, "--checkpoint-every", live::CHECKPOINT_EVERY);
    let seg_every: u64 = arg(args, "--segment-every", 0);
    if seg_every > 0 && opt("--strand").is_none() { fail("live: --segment-every goes with --strand NAME.strand (the segments are NAME.0001.strand, ...)") }
    let state_file = opt("--state"); if state_file.is_some() && opt("--seed").is_some() { fail("live: give --seed N (a new individual) or --state FILE (a stored one), not both") }
    let st = match &state_file {
        Some(f) => { let t = std::fs::read_to_string(f).unwrap_or_else(|e| bad_input("live: ", persona::perr("state", format!("cannot read {f}: {e} (make one with `probbit persona init`)"))));
            let j = json::parse(t.strip_prefix('\u{feff}').unwrap_or(&t)).unwrap_or_else(|e| bad_input("live: ", persona::perr("state", format!("not JSON: {}", e.msg))));
            persona::State::read(&p, &j).unwrap_or_else(|e| bad_input("live: ", e)) }
        None => persona::init(&p, seed_arg(args, "--seed"), true, eng) };
    let clock = match opt("--clock").as_deref() { None | Some("real") => live::Clock::Real(std::time::Instant::now()), Some("fixed") => live::Clock::Fixed, Some(c) => fail(&format!("live: --clock real|fixed, not {c:?}")) };
    let watch = args.iter().any(|a| a == "--watch"); if watch && opt("--events").is_none() { fail("live: --watch follows an --events FILE") }
    // --strand: one writer per strand (the lock, held to the exit, from before the strand is read); a new file gets the header;
    // an existing strand is continued from --state (the state after its last line)
    let strand = opt("--strand");
    let lock = strand.as_ref().map(|f| live::Lock::take(f).unwrap_or_else(|e| refuse("live: ", e)));
    if let Some(f) = &strand { if std::path::Path::new(f).exists() && state_file.is_none() { fail(&format!("live: {f} exists: continue it with --state FILE (the state after its last line), or log to a new file")) } }
    // the run continues the newest segment of the strand (NAME.strand, then NAME.0001.strand, ...), sealed by --segment-every
    // or by `live control STRAND seal`: a sealed file is never appended to
    let mut seg = 0u64; if let Some(f) = &strand { while std::path::Path::new(&live::segment_name(f, seg + 1)).exists() { seg += 1; } }
    let cur = std::cell::RefCell::new(strand.as_ref().map(|f| live::segment_name(f, seg)));
    // the strand's engine (its header's; a new strand: this one's): the next segment's header names it
    let strand_engine = cur.borrow().as_ref().and_then(|f| std::fs::read_to_string(f).ok()).and_then(|t| json::parse(live::lines(&t).first().copied().unwrap_or("")).ok())
        .and_then(|h| h.get("engine").and_then(Json::as_str).map(str::to_string)).unwrap_or_else(live::engine);
    if let Some(f) = strand.as_ref().filter(|_| !labels.is_empty()) { labels::merge_into(f, &labels).unwrap_or_else(|e| fail(&format!("live: {e}"))); }
    let (mut lv, header) = match cur.borrow().as_ref() { Some(f) => live::open(f, p, &doc, st, clock).unwrap_or_else(|e| bad_input("live: ", e)), None => (live::Live::start(p, &doc, st, clock, &live::engine()).0, None) };
    if seg_every > 0 && lv.expect.as_ref().is_some_and(|m| !m.is_canonical()) { fail("live: --segment-every: before engine 0.14.0 a checkpoint does not carry the expectation's memory, so a segment of a persona with an expectation block could not start from its sealed line") }
    // every append first checks the lock is still this writer's: a lock removed or taken over under it stops the run unwritten
    let log = |ls: &[&str]| if let (Some(f), Some(c)) = (&strand, cur.borrow().as_ref()) {
        if !lock.as_ref().is_some_and(live::Lock::held) { refuse("live: ", json::InErr { code: "locked", path: "strand".into(), msg: format!("the writer lock {f}.lock is no longer this run's (removed or taken over): stopped before writing") }) }
        live::append(c, ls).unwrap_or_else(|e| fail(&format!("live: {}", e.msg))) };
    if let Some(h) = &header { log(&[h]); }
    let (mut bad, mut refused, mut warned, mut warned_credit, mut warned_needs) = (0, 0, false, false, false);
    // feature `turn_trace`: with PROBBIT_TURN_TRACE set, each event's phases on stderr (the parse and the append are timed here)
    #[cfg(feature = "turn_trace")]
    let trace_on = std::env::var_os("PROBBIT_TURN_TRACE").is_some();
    let mut one = |lv: &mut live::Live, i: usize, t: &str| {
        #[cfg(feature = "turn_trace")]
        let (t_parse, parse_us) = (std::time::Instant::now(), std::cell::Cell::new(0u64));
        let res = json::parse(t).map_err(|e| persona::perr("event", format!("not JSON: {}", e.msg))).and_then(|ev| {
            #[cfg(feature = "turn_trace")]
            parse_us.set(t_parse.elapsed().as_micros() as u64);
            // readings sent to a persona without a senses block are an undeclared input (listed in `ignored`): say so once
            if !warned && !lv.p.has_senses() && ev.get("senses").is_some() { warned = true;
                err_line(&format!("probbit: live: events[{i}]: this persona has no senses block: `senses` is an undeclared input (listed in ignored), no reading is judged")); }
            // so are `await`, `for` and `cancel` without a credit block (docs/persona.md §2.8): say so once
            if !warned_credit && !lv.p.has_credit() && ["await", "for", "cancel"].iter().any(|k| ev.get(k).is_some()) { warned_credit = true;
                err_line(&format!("probbit: live: events[{i}]: this persona has no credit block: `await`, `for` and `cancel` are undeclared inputs (listed in ignored), feedback pays the previous stance")); }
            // and `needs` without a needs block (docs/persona.md §2.12): say so once
            if !warned_needs && !lv.p.has_needs() && ev.get("needs").is_some() { warned_needs = true;
                err_line(&format!("probbit: live: events[{i}]: this persona has no needs block: `needs` is an undeclared input (listed in ignored), no amount is paid")); }
            lv.event(&ev, eng) });
        match res {
            Ok((stance, line)) => { let seal = strand.is_some() && seg_every > 0 && lv.n % seg_every == 0;
                let cp = (strand.is_some() && (seal || (every > 0 && lv.n % every == 0))).then(|| lv.checkpoint(&stance));
                #[cfg(feature = "turn_trace")]
                let t_append = std::time::Instant::now();
                match &cp { Some(c) => log(&[&line, c]), None => log(&[&line]) }
                // sealed: the next segment file, its header linking this one, then the sealed line itself; the run goes on there
                if let (true, Some(c), Some(base)) = (seal, &cp, &strand) {
                    let prev = cur.borrow().clone().unwrap_or_default(); seg += 1; let next = live::segment_name(base, seg);
                    let pname = std::path::Path::new(&prev).file_name().map_or_else(|| prev.clone(), |f| f.to_string_lossy().into_owned());
                    let h = live::segment_header(lv, &doc, &strand_engine, seg, &pname, &persona::digest_of(c));
                    if std::path::Path::new(&next).exists() { fail(&format!("live: {next} exists: the strand's newest segment is not the one this run continued")) }
                    *cur.borrow_mut() = Some(next); log(&[&h, c]); }
                #[cfg(feature = "turn_trace")]
                if trace_on { let tr = probbit_persona::trace::last(); let us = |t: std::time::Instant| Json::Num(t.elapsed().as_micros() as f64);
                    let mut j = tr.to_json(); if let Json::Obj(v) = &mut j { v.push(("n".into(), Json::Num(lv.n as f64))); v.push(("parse_us".into(), Json::Num(parse_us.get() as f64))); v.push(("append_us".into(), us(t_append))); }
                    err_line(&format!("probbit: live: trace {}", persona::canon(&j))); }
                if let Some(f) = &state_file { put(Some(f), &persona::canon(&lv.st.to_json(&lv.p))); } emit(&persona::canon(&stance)); }
            Err(e) => { if ["paused", "retired"].contains(&e.code) { refused += 1 } else { bad += 1 }
                let e = json::InErr { path: format!("events[{i}].{}", e.path), ..e }; err_line(&format!("probbit: live: {}: {}", e.path, e.msg)); error_object(&e.to_json()); } } };
    match (opt("--events"), watch) {
        // --watch: follow the file as lines are appended (a line counts once its newline is written); stops when the file is removed
        (Some(f), true) => { let mut r = std::io::BufReader::new(std::fs::File::open(&f).unwrap_or_else(|e| fail(&format!("live: cannot read {f}: {e}"))));
            let (mut pending, mut i) = (String::new(), 0usize);
            loop { let n = std::io::BufRead::read_line(&mut r, &mut pending).unwrap_or_else(|e| fail(&format!("live: cannot read the events: {e}")));
                if n == 0 { if !std::path::Path::new(&f).exists() { break; } std::thread::sleep(std::time::Duration::from_millis(50)); continue; }
                if !pending.ends_with('\n') { continue; }
                let l = std::mem::take(&mut pending); if !l.trim().is_empty() { one(&mut lv, i, l.trim()); } i += 1; } }
        (f, _) => { let input: Box<dyn std::io::BufRead> = match f { Some(f) => Box::new(std::io::BufReader::new(std::fs::File::open(&f).unwrap_or_else(|e| fail(&format!("live: cannot read {f}: {e}"))))),
                None => Box::new(std::io::BufReader::new(std::io::stdin())) };
            for (i, l) in std::io::BufRead::lines(input).enumerate() {
                let l = l.unwrap_or_else(|e| fail(&format!("live: cannot read the events: {e}"))); let t = l.trim(); if !t.is_empty() { one(&mut lv, i, t); } } } }
    err_line(&format!("live: {} events, strand head {}, final state {}{}", lv.n, lv.head(), lv.st.digest, if lv.status == live::Status::Active { String::new() } else { format!(", {} ({refused} events refused)", lv.status.name()) }));
    drop(lock);
    if refused > 0 { exit(4) }
    if bad > 0 { exit(2) }
}
/// What `persona lint` reports, and `probbit check` reuses: the lint document, its unresolved contradictions, the rules the search
/// broke and the unproven ones (the document's `unproven` entries)
struct Lint { doc: Json, unresolved: usize, broken: usize, unproven: Vec<Json> }
/// `persona lint`'s pipeline: the contradictions and plan warnings; with rules (--never / --props, §5.6) prove each one, then fuzz
/// the ones the bound leaves unknown (`depth` events per script, idle `hours`; the grid gets the rules' and habits' thresholds)
fn lint_report(args: &[String], path: &str, p: &persona::Persona, depth: usize, hours: Vec<f64>) -> Lint {
    let engine = persona_engine(); let eng: persona::Engine = &engine;
    let found = persona::lint(p, eng); let unresolved = found.iter().filter(|f| !f.get("resolution").and_then(Json::as_str).is_some_and(|r| r.starts_with("yield"))).count();
    let seeds = seeds_arg(args); let warnings = persona::plan_warnings(p, &seeds, eng);
    let mut doc = vec![("persona", jstr(&p.name)), ("conflicts", Json::Arr(found)), ("unresolved", num(unresolved as f64)), ("warnings", Json::Arr(warnings))];
    let (mut broken, mut unproven) = (0, vec![]);
    // drives (§2.9): habits that outrank a goal floor, floors whose lift can reach 50
    if let Some(mut n) = persona::floor_notes(p) { n.extend(persona::floor_reach(p, &seeds)); doc.push(("floors", Json::Arr(n))); }
    // needs (RFC 0002): horizons whose forecast can pass K/2 before the deficit does, habits that exclude a bound goal
    if let Some(n) = persona::needs_notes(p) { doc.push(("needs", Json::Arr(n))); }
    // pace (RFC 0002 Design C): the largest field shift pace can cause, with a warning over 1
    if let Some(n) = persona::pace_notes(p) { doc.push(("pace", n)); }
    // with rules (§5.6): prove each one, then fuzz the ones the bound leaves unknown
    if args.iter().any(|a| a == "--never" || a == "--props") {
        let props = rules_arg(args, p); let threads = arg(args, "--threads", std::thread::available_parallelism().map_or(1, |n| n.get())).clamp(1, 1024);
        let verdicts = fuzz::prove(p, &props, &seeds, threads, &engine);
        let unknown: Vec<persona::Prop> = props.iter().zip(&verdicts).filter(|(_, v)| matches!(v, fuzz::Verdict::Unknown { .. })).map(|(q, _)| q.clone()).collect();
        let s = fuzz::Search { seeds: seeds.clone(), fuzz_seed: 0, scripts: 60, depth, beam: 4, grid: fuzz::grid(p, &unknown, &[0.0, 0.5, 1.0]), hours, threads };
        let (res, turns) = if unknown.is_empty() { (vec![], 0) } else { fuzz::fuzz(p, &unknown, &s, &engine) };
        let fd = fuzz::doc(p, path, &unknown, &s, &res, turns); let fp = fd.get("properties").and_then(Json::as_arr).unwrap_or(&[]).to_vec();
        broken = fp.iter().filter(|x| x.get("verdict").and_then(Json::as_str) == Some("counterexample")).count();
        let mut entries = fuzz::prove_doc(p, &props, &seeds, &verdicts).get("properties").and_then(Json::as_arr).unwrap_or(&[]).to_vec();
        for e in entries.iter_mut() { let id = e.get("id").cloned();
            if let (Json::Obj(v), Some(f)) = (e, fp.iter().find(|x| x.get("id").cloned() == id)) { v.push(("fuzz".into(), f.clone())); } }
        // unproven: the bound left it unknown and the search found no counterexample. Not broken, and not proved either.
        for (q, v) in props.iter().zip(&verdicts) {
            let fuzz::Verdict::Unknown { failed, .. } = v else { continue };
            if fp.iter().any(|x| x.get("id").and_then(Json::as_str) == Some(q.id.as_str()) && x.get("verdict").and_then(Json::as_str) == Some("counterexample")) { continue; }
            let searched = obj(vec![("individuals", num(s.seeds.len() as f64)), ("scripts", num(s.scripts as f64)), ("depth", num(s.depth as f64)), ("beam", num(s.beam as f64)),
                ("grid", Json::Arr(s.grid.iter().map(|x| num(*x)).collect())), ("hours", Json::Arr(s.hours.iter().map(|x| num(*x)).collect()))]);
            unproven.push(obj(vec![("id", jstr(&q.id)), ("rule", q.rule.clone()), ("seeds_unknown", Json::Arr(failed.iter().map(|f| num(f.0 as f64)).collect())), ("searched", searched)])); }
        doc.push(("props", Json::Arr(entries))); doc.push(("broken", num(broken as f64))); doc.push(("unproven", Json::Arr(unproven.clone()))); }
    doc.push(("ok", Json::Bool(unresolved == 0 && broken == 0 && unproven.is_empty())));
    Lint { doc: obj(doc), unresolved, broken, unproven }
}
/// `probbit check PERSONA [--props FILE] [--strand FILE]... [--seeds LIST] [--depth N] [--hours LIST] [--json]`: one gate for CI.
/// `persona lint`'s pipeline on the rules (`lint_report`), then `live verify` on every strand; exit 1 when a rule is broken, a
/// contradiction is unresolved or a strand fails, else 3 when a rule is unproven, else 0
/// The window of `--last N` / `--since-checkpoint` (RFC 0013)
fn reflect_window(args: &[String], what: &str) -> probbit_persona::reflect::Window {
    use probbit_persona::reflect::Window;
    let last = args.iter().position(|x| x == "--last").map(|k| args.get(k + 1).and_then(|v| v.parse::<u64>().ok()).filter(|n| *n >= 1)
        .unwrap_or_else(|| fail(&format!("{what}: --last N: a whole number >= 1"))));
    match (last, args.iter().any(|a| a == "--since-checkpoint")) { (Some(_), true) => fail(&format!("{what}: --last and --since-checkpoint: one window")),
        (Some(n), false) => Window::Last(n), (None, true) => Window::SinceCheckpoint, (None, false) => Window::All }
}
/// `probbit reflect STRAND [--last N | --since-checkpoint] [--json | --text]` (RFC 0013)
fn reflect_cmd(args: &[String]) {
    let Some(path) = args.get(1).filter(|a| !a.starts_with("--")) else { fail("reflect: the strand file: probbit reflect STRAND [--last N | --since-checkpoint]") };
    let mut a = vec!["reflect".to_string()]; a.extend(args.iter().skip(2).cloned()); let (v, w) = flags_of("reflect"); check_flags(&a, &v, &w);
    if args.iter().any(|a| a == "--json") && args.iter().any(|a| a == "--text") { fail("reflect: --json or --text") }
    let text = std::fs::read_to_string(path).unwrap_or_else(|e| fail(&format!("reflect: cannot read {path}: {e}")));
    let r = probbit_persona::reflect::reflect_text(&text, reflect_window(args, "reflect")).unwrap_or_else(|e| bad_input("reflect: ", e));
    if args.iter().any(|a| a == "--text") { emit(&r.summary().join("\n")) } else { emit(&r.text()) }
}
fn check_cmd(args: &[String]) {
    // --strand may be given more than once: take its values out before the flags are checked
    let (mut rest, mut strands) = (vec![], vec![]); let mut i = 0;
    while i < args.len() {
        if args[i] == "--strand" { match args.get(i + 1).filter(|v| !v.starts_with("--")) { Some(v) => strands.push(v.clone()), None => fail("--strand needs a value") } i += 2; }
        else { rest.push(args[i].clone()); i += 1; } }
    let Some(path) = rest.get(1).filter(|a| !a.starts_with("--")).cloned() else { fail("check: the persona file comes first: probbit check PERSONA [--props FILE] [--strand FILE]...") };
    let mut a = vec!["check".to_string()]; a.extend(rest[2..].iter().cloned()); let (v, w) = flags_of("check"); check_flags(&a, &v, &w);
    let depth: usize = arg(&a, "--depth", 8); if !(1..=64).contains(&depth) { fail("--depth must be 1 to 64") }
    let hours = list_arg(&a, "--hours", &[1.0, 12.0, 48.0], 0.0, 1e6);
    let (p, _) = persona::load(&path).unwrap_or_else(|e| bad_input("check: ", e));
    let seeds = seeds_arg(&a);
    let Lint { doc: lint, unresolved, broken, unproven } = lint_report(&a, &path, &p, depth, hours.clone());
    let engine = persona_engine(); let eng: persona::Engine = &engine;
    let strand_docs: Vec<Json> = strands.iter().map(|f| {
        // a strand with a field or line kind this engine does not know (`live verify` exit 5) fails the gate as unsupported: its
        // entry adds `unsupported` (the field) and `strand_engine` (the header's engine), and its line never says FAIL
        let (res, unsupported) = match std::fs::read_to_string(f) {
            Err(e) => (Err(format!("cannot read: {e}")), None),
            Ok(t) => match live::verify(&t, eng) { Ok(d) => (Ok(d), None),
                Err((line, why)) => { let u = live::unsupported_field(&why).map(|u| (jstr(u), live::strand_engine(&t))); (Err(format!("line {line}: {why}")), u) } } };
        let mut e = vec![("path", jstr(f))];
        match res {
            Ok(d) => { e.push(("ok", Json::Bool(true))); e.push(("events", d.get("events").cloned().unwrap_or(Json::Null))); e.push(("final_state", d.get("final_state").cloned().unwrap_or(Json::Null))); }
            Err(why) => { e.push(("ok", Json::Bool(false))); e.push(("events", Json::Null)); e.push(("final_state", Json::Null)); e.push(("error", jstr(&why))); } }
        if let Some((u, by)) = unsupported { e.push(("unsupported", u)); e.push(("strand_engine", by)); }
        obj(e) }).collect();
    let strands_ok = strand_docs.iter().filter(|s| s.get("ok") == Some(&Json::Bool(true))).count();
    let rules: &[Json] = lint.get("props").and_then(Json::as_arr).unwrap_or(&[]);
    let held = rules.iter().filter(|r| matches!(r.get("verdict").and_then(Json::as_str), Some("held_by_construction") | Some("proved"))).count();
    let code = if broken > 0 || unresolved > 0 || strands_ok < strand_docs.len() { 1 } else if !unproven.is_empty() { 3 } else { 0 };
    if a.iter().any(|x| x == "--json") {
        let verdict = obj(vec![("held", num(held as f64)), ("broken", num(broken as f64)), ("unproven", num(unproven.len() as f64)), ("conflicts", num(unresolved as f64)),
            ("strands_ok", num(strands_ok as f64)), ("strands_total", num(strand_docs.len() as f64)), ("exit", num(code as f64))]);
        let props = a.iter().position(|x| x == "--props").and_then(|k| a.get(k + 1)).map_or(Json::Null, |f| jstr(f));
        emit(&persona::canon(&obj(vec![("persona", jstr(&path)), ("props", props), ("seeds", jstr(&fuzz::ranges(&seeds))), ("depth", num(depth as f64)),
            ("hours", Json::Arr(hours.iter().map(|x| num(*x)).collect())), ("rules", lint), ("strands", Json::Arr(strand_docs)), ("verdict", verdict)])));
        exit(code)
    }
    let seed_list = |j: Option<&Json>| fuzz::ranges(&j.and_then(Json::as_arr).unwrap_or(&[]).iter().filter_map(|x| x.as_f64().map(|x| x as u64)).collect::<Vec<_>>());
    let width = rules.iter().filter_map(|r| r.get("id").and_then(Json::as_str)).map(|s| s.chars().count()).chain(strands.iter().map(|s| s.chars().count())).max().unwrap_or(0);
    let mut o = vec![format!("probbit check  {path}  ({} {}, seeds {}, depth {depth})", p.name, p.version, fuzz::ranges(&seeds))];
    if unresolved > 0 { o.push(format!("conflicts  {unresolved} unresolved habit contradiction{} (probbit persona lint {path})", if unresolved == 1 { "" } else { "s" })); }
    if rules.is_empty() { o.push("rules   none given (--props FILE)".into()); }
    for r in rules {
        let id = r.get("id").and_then(Json::as_str).unwrap_or("");
        let what = match (r.get("verdict").and_then(Json::as_str), r.get("fuzz").and_then(|f| f.get("verdict")).and_then(Json::as_str)) {
            (Some("held_by_construction"), _) => "held_by_construction".to_string(),
            (Some("proved"), _) => "held".to_string(),
            (_, Some("counterexample")) => { let s = r.get("fuzz").and_then(|f| f.get("failing_seeds")).and_then(Json::as_arr).map_or(0, |a| a.len());
                format!("broken seed{} {}", if s == 1 { "" } else { "s" }, seed_list(r.get("fuzz").and_then(|f| f.get("failing_seeds")))) }
            _ => format!("unproven  seeds {} not proved, no counterexample found", seed_list(unproven.iter().find(|u| u.get("id").and_then(Json::as_str) == Some(id)).and_then(|u| u.get("seeds_unknown")))) };
        o.push(format!("rule    {id:<width$}  {what}")); }
    for s in &strand_docs {
        let f = s.get("path").and_then(Json::as_str).unwrap_or("");
        let error = s.get("error").and_then(Json::as_str).unwrap_or("");
        let what = if s.get("ok") == Some(&Json::Bool(true)) { format!("ok {} events", s.get("events").and_then(Json::as_f64).unwrap_or(0.0)) }
            else if s.get("unsupported").is_some() { format!("{error} ({})", s.get("strand_engine").and_then(Json::as_str).map_or_else(|| "the header names no engine".to_string(), |e| format!("written by {e}"))) }
            else { format!("FAIL {error}") };
        o.push(format!("strand  {f:<width$}  {what}")); }
    o.push(format!("check: held {held} · broken {broken} · unproven {} · strands {strands_ok}/{}", unproven.len(), strand_docs.len()));
    emit(&o.join("\n"));
    exit(code)
}
/// `probbit persona <sub> PERSONA [flags]` (docs/persona.md)
fn persona_cmd(args: &[String]) {
    let sub = args.get(1).map_or("", String::as_str);
    let Some((vals, switches)) = sub_flags("persona", sub) else {
        if sub.is_empty() || sub.starts_with('-') { fail("persona: a subcommand: init, turn, replay, explain, diff, lint, fuzz, prove, check, compile or describe (probbit persona --help)") }
        unknown(command("persona"), sub) };
    let pretty = args.iter().any(|a| a == "--pretty");
    if pretty && sub == "replay" { fail("persona replay emits one canonical JSON object per line (JSONL); --pretty is for single-document commands such as persona turn") }
    if pretty && sub == "explain" { fail("persona explain already emits readable text; --pretty is for JSON commands") }
    let Some(path) = args.get(2).filter(|a| !a.starts_with("--")) else { fail(&format!("persona {sub}: the persona file comes first: probbit persona {sub} PERSONA [flags]")) };
    let mut a = vec![format!("persona {sub}")]; a.extend(args[3..].iter().cloned()); check_flags(&a, &vals, &switches);
    let need = |f: &str| -> String { args.iter().position(|x| x == f).and_then(|k| args.get(k + 1)).cloned().unwrap_or_else(|| fail(&format!("persona {sub}: {f} is required"))) };
    let opt = |f: &str| -> Option<String> { args.iter().position(|x| x == f).and_then(|k| args.get(k + 1)).cloned() };
    let (p, pdoc) = persona::load(path).unwrap_or_else(|e| bad_input("persona: ", e));
    let engine = persona_engine(); let eng: persona::Engine = &engine;
    let bound = || persona::bound::of_doc(&pdoc, &p).unwrap_or_else(|e| bad_input("persona: ", e));
    let no_inertia = args.iter().any(|x| x == "--no-inertia");
    let state = |f: &str| -> persona::State { let s = need(f);
        let t = std::fs::read_to_string(&s).unwrap_or_else(|e| bad_input("persona: ", persona::perr("state", format!("cannot read {s}: {e} (make one with `probbit persona init`)"))));
        let j = json::parse(t.strip_prefix('\u{feff}').unwrap_or(&t)).unwrap_or_else(|e| bad_input("persona: ", persona::perr("state", format!("not JSON: {}", e.msg))));
        persona::State::read(&p, &j).unwrap_or_else(|e| bad_input("persona: ", e)) };
    let inputs = || opt("--inputs").map_or(Json::Obj(vec![]), |s| json_arg(&s, "inputs"));
    let turns = || persona::script(&json_arg(&need("--script"), "script")).unwrap_or_else(|e| bad_input("persona: ", e));
    match sub {
        "init" => { let st = persona::init(&p, seed_arg(args, "--seed"), true, eng);
            if let Some(path) = opt("--out") { put(Some(&path), &persona::canon(&st.to_json(&p))); err_line(&format!("wrote {path} ({} {}, seed {})", p.name, p.version, st.seed)); }
            else { emit(&persona_text(&st.to_json(&p), pretty)); } }
        "turn" => { let st = state("--state"); let (out, ns) = persona::turn(&p, &st, &inputs(), no_inertia, eng, args.iter().any(|x| x == "--timing")).unwrap_or_else(|e| bad_input("persona: ", e));
            put(Some(&opt("--out").unwrap_or_else(|| need("--state"))), &persona::canon(&ns.to_json(&p))); emit(&persona_text(&out, pretty)); }
        "replay" => { let (trace, st) = persona::replay(&p, seed_arg(args, "--seed"), &turns(), no_inertia, eng, args.iter().any(|x| x == "--timing")).unwrap_or_else(|e| bad_input("persona: ", e));
            let text: String = trace.iter().map(|t| persona::canon(t) + "\n").collect();
            match opt("--out") { Some(o) => if let Err(e) = std::fs::write(&o, &text) { fail(&format!("persona: cannot write {o}: {e}")) }, None => emit_raw(&text) }
            err_line(&format!("replay: {} turns, trace sha256 {}, final state {}", trace.len(), persona::hex(&persona::sha256(text.as_bytes())), st.digest)); }
        "explain" => { let ts = turns(); let k: usize = arg(args, "--turn", usize::MAX); if k == usize::MAX { fail("persona explain: --turn K is required") }
            if k >= ts.len() { bad_input("persona: ", persona::perr("--turn", format!("the script has {} turns (0..{})", ts.len(), ts.len().saturating_sub(1)))) }
            let (_, st) = persona::replay(&p, seed_arg(args, "--seed"), &ts[..k], false, eng, false).unwrap_or_else(|e| bad_input("persona: ", e));
            emit(&persona::explain(&p, &st, &ts[k], eng).unwrap_or_else(|e| bad_input("persona: ", e))); }
        "diff" => { let q = opt("--other").map_or_else(|| p.clone(), |o| persona::load(&o).unwrap_or_else(|e| bad_input("persona: ", e)).0);
            let (sa, sb) = (seed_arg(args, "--seed").unwrap_or(p.seed), seed_arg(args, "--seed2").unwrap_or(q.seed));
            emit(&persona_text(&persona::diff(&p, sa, &q, sb, &turns(), eng).unwrap_or_else(|e| bad_input("persona: ", e)), pretty)); }
        "trajectory" => { let (k, b0) = p.traj.as_ref().map_or((8, 0.4), |t| (t.k, t.beta0));
            let prog = match opt("--program") { Some(f) => json_arg(&f, "program"), None => persona::program_json(&p, &state("--state"), &inputs()).unwrap_or_else(|e| bad_input("persona: ", e)) };
            match persona::trajectory_program(&prog, k, b0, 2_000_000) { Some(t) => emit(&persona_text(&t, pretty)), None => bad_input("persona: ", persona::perr("trajectory", "a step is not solved by the exact tier (too large, or infeasible)")) } }
        "compile" => { let st = state("--state"); let text = persona::program(&p, &st, &inputs()).unwrap_or_else(|e| bad_input("persona: ", e));
            if pretty { emit(&json::write(&json::parse(&text).expect("compiled program JSON"), true)); } else { emit(&text); } }
        "check" => { let mut doc = persona::check(&p, eng).unwrap_or_else(|e| bad_input("persona: ", e)); let mut ok = true;
            // RFC 0004: a declared max_epsilon makes the certificate part of the check
            if let Some(b) = bound().filter(|b| b.max_epsilon.is_some()) { let (cert, k) = persona::bound::publish(&p, &b, None, eng).unwrap_or_else(|e| bad_input("persona: ", e));
                ok = k; if let Json::Obj(v) = &mut doc { v.push(("bound".into(), cert)); v.iter_mut().filter(|(k, _)| k == "ok").for_each(|(_, x)| *x = Json::Bool(ok)); } }
            emit(&persona_text(&doc, pretty)); if !ok { std::process::exit(1) } }
        "bound" => { let b = bound().unwrap_or(persona::bound::Bound { rest_hours: None, max_epsilon: None, seeds: None, count: vec![] });
            let json_out = args.iter().any(|x| x == "--json") || pretty;
            if let Some(s) = opt("--audit") {
                let text = std::fs::read_to_string(&s).unwrap_or_else(|e| bad_input("persona: ", persona::perr("--audit", format!("cannot read {s}: {e}"))));
                match live::bound_audit(&text, &b, eng) {
                    Ok((doc, ok)) => { emit(&persona_text(&doc, pretty)); if !ok { std::process::exit(1) } }
                    Err((line, why)) => bad_input("persona: ", persona::perr("--audit", format!("line {line}: {why}"))) }
                return; }
            let seeds = opt("--seeds").map(|v| persona::bound::seeds_text(&v).unwrap_or_else(|| bad_input("persona: ", persona::perr("--seeds", "a range a-b or a list a,b,c of whole numbers in 0..2147483647 (a <= b, at most 10000 seeds)"))));
            let (doc, ok) = persona::bound::publish(&p, &b, seeds, eng).unwrap_or_else(|e| bad_input("persona: ", e));
            if json_out { emit(&persona_text(&doc, pretty)); } else { emit(&persona::bound::text(&doc)); }
            if !ok { std::process::exit(1) } }
        "describe" => emit(&persona_text(&persona::describe(&p), pretty)),
        "fuzz" | "prove" => fuzz_cmd(args, path, &p, &engine),
        _ => { let Lint { mut doc, unresolved, broken, unproven } = lint_report(args, path, &p, 8, vec![1.0, 12.0, 48.0]);
            // the dead-rule lint: habits this host can never bring into force (a warning; an error with --strict)
            let mut dead = 0;
            if let Some(h) = opt("--host") { let j = json_arg(&h, "host"); let (name, ins, sns) = persona::hostlint::manifest(&j).unwrap_or_else(|e| bad_input("persona: ", e));
                let d = persona::hostlint::dead(&p, &ins, &sns, eng).unwrap_or_else(|e| bad_input("persona: ", e)); dead = d.len();
                for x in &d { err_line(&format!("lint: {} {} on host {name} ({})", x.get("habit").and_then(Json::as_str).unwrap_or(""), "dead", x.get("needs").and_then(Json::as_str).unwrap_or(""))); }
                if let Json::Obj(v) = &mut doc { v.push(("host".into(), Json::Str(name))); v.push(("dead".into(), Json::Arr(d))); } }
            else if args.iter().any(|x| x == "--strict") { fail("persona lint: --strict needs --host MANIFEST") }
            emit(&persona_text(&doc, pretty));
            if dead > 0 && args.iter().any(|x| x == "--strict") { std::process::exit(1) }
            if !unproven.is_empty() {
                let ids: Vec<&str> = unproven.iter().filter_map(|u| u.get("id").and_then(Json::as_str)).collect();
                err_line(&format!("lint: {} rule{} unproven, not broken: {} (the bound left {} unknown and the search found no counterexample; `probbit persona prove` names the cell)",
                    ids.len(), if ids.len() == 1 { "" } else { "s" }, ids.join(", "), if ids.len() == 1 { "it" } else { "them" })); }
            if unresolved > 0 || broken > 0 { std::process::exit(1) }
            if !unproven.is_empty() { std::process::exit(3) } }
    }
}

fn ir_cmd(args: &[String]) {
    let (v, w) = flags_of("ir"); check_flags(args, &v, &w); let j = read_doc(args);
    let n = from_json(&j).unwrap_or_else(|e| bad_input("bad problem: ", e)); emit_raw(&probbit_decide::ir::to_ir(&n.p));
}
fn run_cmd(args: &[String]) {
    let (v, w) = flags_of("run"); check_flags(args, &v, &w); let tp = std::time::Instant::now(); let j = read_doc(args);
    let tc = std::time::Instant::now(); let parse_ms = (tc - tp).as_secs_f64() * 1e3;
    let p = run::from_json(&j).unwrap_or_else(|e| bad_input("bad program: ", e)); let compile_ms = tc.elapsed().as_secs_f64() * 1e3;
    let (doc, code, bars) = run_program(args, p, "run", tp, parse_ms, compile_ms);
    let c = finish(doc, code, &View::new(args, "run"), &bars); if c != 0 { std::process::exit(c); }
}
fn mcp_cmd(args: &[String]) { check_flags(args, &[], &[]); mcp::serve() }
fn version_cmd(args: &[String]) { check_flags(args, &[], &[]); emit(&format!("probbit {VERSION}")) }
/// `probbit --help` piped (and `probbit help`): the grouped usage; at a terminal the hero screen
fn top_help() -> String { usage::top_help(COMMANDS, VERSION) }
/// `probbit completions bash|zsh|fish` (usage::completions)
fn completions_cmd(args: &[String]) {
    let shells = usage::SHELLS.join(", ");
    let Some(sh) = args.get(1).filter(|a| !a.starts_with('-')) else { fail(&format!("completions: a shell: {shells} (probbit completions zsh)")) };
    let mut a = vec!["completions".to_string()]; a.extend(args[2..].iter().cloned()); check_flags(&a, &[], &[]);
    match usage::completions(sh, COMMANDS) { Some(s) => emit_raw(&s), None => fail(&format!("completions: unknown shell {sh:?}; one of {shells}")) }
}
/// `probbit man` (usage::man; docs/probbit.1 is its output)
fn man_cmd(args: &[String]) { check_flags(args, &[], &[]); emit_raw(&usage::man(COMMANDS, VERSION)) }
/// `probbit help [COMMAND [SUBCOMMAND]]`: the same text as `probbit COMMAND [SUBCOMMAND] --help`; alone, as `probbit --help`
/// (the hero screen at a terminal, else the usage). Exit 0; an unknown command or subcommand exits 2.
fn help_cmd(args: &[String]) {
    let words: Vec<&str> = args[1..].iter().map(String::as_str).filter(|a| *a != "--plain").collect();
    match words[..] {
        [] => if theme::stdout_is_terminal() { emit_raw(&tui::hero(theme::stdout(args))) } else { emit_raw(&top_help()) },
        [name, ..] if words.len() <= 2 => {
            let Some(c) = command(name) else { unknown(None, name) };
            match words.get(1) {
                None => emit_raw(&usage::command_help(c)),
                Some(s) => match c.sub(s) { Some(s) => emit_raw(&usage::sub_help(c, s)),
                    None => unknown(Some(c), s) } }
        }
        _ => fail("help: at most a command and a subcommand: probbit help [COMMAND [SUBCOMMAND]]"),
    }
}

fn main() {
    // the library's hooks: the `--top` monitor and the config echo in telemetry
    probbit_persona::hooks::set_monitor(tui::tier, tui::gate_seen); probbit_persona::hooks::set_config_echo(config_echo);
    // an argument that is not valid Unicode (a file name in a legacy encoding on Linux, an unpaired surrogate on Windows) is
    // refused by name; std::env::args would panic on it
    let args: Vec<String> = std::env::args_os().skip(1).enumerate().map(|(i, a)| a.into_string().unwrap_or_else(|a|
        fail(&format!("argument {} is not valid UTF-8 ({}); rename the file or pass a UTF-8 path", i + 1, a.to_string_lossy())))).collect();
    JSON_FLAG.store(args.iter().any(|a| a == "--json"), std::sync::atomic::Ordering::Relaxed);
    // `probbit CMD [SUB] ... --help`: the command's help, or the subcommand's when the word after the command is one of its subcommands
    if args.len() >= 2 && args[1..].iter().any(|a| a == "--help" || a == "-h") { if let Some(c) = command(&args[0]) {
        emit_raw(&match args.get(1).and_then(|s| c.sub(s)) { Some(s) => usage::sub_help(c, s), None => usage::command_help(c) }); return; } }
    // At a terminal, `probbit` (no command) and `probbit --help` print the hero screen (exit 0); piped, `--help` prints the usage and a
    // bare `probbit` still exits 2 with the usage on stderr, as before
    let bare = args.iter().all(|a| a == "--plain") || args.iter().all(|a| a == "--help" || a == "-h" || a == "--plain");
    if bare && theme::stdout_is_terminal() { emit_raw(&tui::hero(theme::stdout(&args))); return; }
    match args.first().map(|s| s.as_str()) {
        // `probbit --help` / `-h` asked for the usage: stdout, exit 0 (it went to stderr with exit 2, as for a missing command)
        Some("--help") | Some("-h") => { check_flags(&args, &[], &["--plain"]); emit_raw(&top_help()); }
        Some(name) => match command(name) { Some(c) => (c.run)(&args), None => unknown(None, name) },
        None => { let _ = std::io::stderr().write_all(top_help().as_bytes()); std::process::exit(2) }
    }
}

#[cfg(test)]
mod table_tests {
    use super::*;
    /// The flags of a synopsis: every `--word` in it
    fn synopsis_flags(s: &str) -> Vec<&str> {
        let mut v: Vec<&str> = s.split(|c: char| c == ' ' || c == '[' || c == ']' || c == '(' || c == ')').filter(|w| w.starts_with("--")).collect();
        v.sort_unstable(); v.dedup(); v
    }
    #[test]
    fn names_are_unique_and_grouped() {
        let mut seen: Vec<&str> = vec![];
        for c in COMMANDS {
            for n in std::iter::once(&c.name).chain(c.aliases) { assert!(!seen.contains(n), "{n} twice"); seen.push(n); }
            assert!(usage::GROUPS.contains(&c.group), "{}: group {}", c.name, c.group);
            assert!(!c.summary.is_empty() && !c.about.is_empty() && !c.usage.is_empty(), "{}", c.name);
            assert!(c.usage.iter().all(|u| u.split(' ').next() == Some(c.name)), "{}: a usage line names another command", c.name);
            let mut subs: Vec<&str> = c.subs.iter().map(|s| s.name).collect(); subs.sort_unstable(); let n = subs.len(); subs.dedup(); assert_eq!(n, subs.len(), "{}", c.name);
            let mut fl: Vec<&str> = c.flags().map(|f| f.name).collect(); fl.sort_unstable(); let n = fl.len(); fl.dedup(); assert_eq!(n, fl.len(), "{}: a flag twice", c.name);
            for s in c.subs { let mut fl: Vec<&str> = c.sub_flags(s).map(|f| f.name).collect(); fl.sort_unstable(); let n = fl.len(); fl.dedup(); assert_eq!(n, fl.len(), "{} {}", c.name, s.name); }
        }
    }
    /// A subcommand's synopsis names exactly its flags; a command's usage lines name only flags it (or a subcommand) accepts,
    /// and every flag of a command without subcommands is in its usage or its usage says [flags]
    #[test]
    fn synopses_match_flags() {
        for c in COMMANDS {
            for s in c.subs {
                let mut want: Vec<&str> = s.flags.iter().map(|f| f.name).collect(); want.sort_unstable();
                assert_eq!(synopsis_flags(s.args), want, "{} {}: synopsis {:?}", c.name, s.name, s.args);
            }
            let known: Vec<&str> = c.flags().chain(c.subs.iter().flat_map(|s| s.flags.iter())).map(|f| f.name).collect();
            for u in c.usage { for f in synopsis_flags(u) { assert!(known.contains(&f), "{}: usage names {f}, which the parser rejects", c.name); } }
            for f in c.flags() {
                assert!(f.name.starts_with("--") && !f.help.is_empty(), "{}: {}", c.name, f.name);
                if c.subs.is_empty() && !c.usage.iter().any(|u| u.contains("[flags]")) { assert!(c.usage.iter().any(|u| u.contains(f.name)), "{}: {} not in the usage", c.name, f.name); }
            }
        }
    }
    /// The parser's flag lists are the table's: `flags_of` / `sub_flags` split them by arity
    #[test]
    fn parser_flags_are_the_tables() {
        let (v, w) = flags_of("decide");
        assert!(v.contains(&"--budget-ms") && v.contains(&"--mode") && !v.contains(&"--progress") && w.contains(&"--progress") && w.contains(&"--top"));
        let (v, w) = sub_flags("persona", "fuzz").unwrap(); assert!(v.contains(&"--fixture-src") && w.contains(&"--json") && w.contains(&"--pretty"));
        let (v, w) = flags_of("monitor"); assert!(v.contains(&"--port") && w.contains(&"--demo"), "--demo takes an optional value: parsed as a switch");
        assert!(sub_flags("persona", "lnt").is_none());
        assert_eq!(sub_flags("live", "verify").unwrap(), (vec!["--actions", "--blocks"], vec!["--from-checkpoint", "--chain"]), "live's own flags are not verify's");
        for c in COMMANDS { let (v, w) = flags_of(c.name); assert_eq!(v.len() + w.len(), c.flags().count(), "{}", c.name); }
    }
    #[test]
    fn did_you_mean() {
        assert_eq!([usage::distance("persna", "persona"), usage::distance("rnu", "run"), usage::distance("lnt", "lint"), usage::distance("", "ir"), usage::distance("abc", "abc")], [1, 1, 1, 2, 0]);
        let names: Vec<&str> = COMMANDS.iter().map(|c| c.name).collect();
        for (typo, want) in [("persna", Some("persona")), ("dcide", Some("decide")), ("rnu", Some("run")), ("pers", Some("persona")), ("monitr", Some("monitor")),
            ("evalute", Some("evaluate")), ("verison", Some("version")), ("x", None), ("frobnicate", None), ("ab", None), ("helpme", Some("help")), ("statistics", None)] {
            assert_eq!(usage::suggest(typo, &names), want, "{typo}");
        }
    }
    /// The dispatcher accepts exactly the table: `main` dispatches through `command()`, and its own match names no command
    #[test]
    fn dispatcher_is_the_table() {
        let src = include_str!("main.rs").replace("\r\n", "\n"); // a Windows checkout has CRLF
        let body = &src[src.find("\nfn main() {").unwrap()..]; let body = &body[..body.find("\n}\n").unwrap()];
        let lits: Vec<&str> = body.match_indices("Some(\"").map(|(i, _)| { let r = &body[i + 6..]; &r[..r.find('"').unwrap()] }).collect();
        assert!(lits.iter().all(|l| ["--help", "-h"].contains(l)), "main dispatches {lits:?} outside the table");
        for (old, n) in [("decide", 0), ("demo", 0), ("ir", 0), ("run", 0), ("evaluate", 0), ("stats", 0), ("persona", 0), ("live", 0), ("check", 0), ("monitor", 0), ("mcp", 0), ("version", 0), ("--version", 0), ("-V", 0)] {
            assert!(command(old).is_some(), "{old} (accepted before the table) is not in it {n}");
        }
    }
}
