//! `probbit evaluate`, the decision-API adapter: its invariants against the CLI. (a) Without rules it returns each question's argmax
//! of the given weights and the same probabilities (it adds nothing when it has nothing to add); (b) with rules the answer differs
//! from the argmax only inside rule-connected groups of questions where the argmax breaks a rule, and in each such group at least
//! one answer moves, with 0 violations; (c) a refusal keeps the `run` exit code and marks every answer unreleased; (d) bad input
//! is exit 2 with ONE error object located in the request; (e) `probbit evaluate --program | probbit run` is the same document.
use std::io::Write;
use std::process::{Command, Stdio};
#[allow(dead_code)]
#[path = "../../probbit-persona/src/json.rs"]
mod json;
use json::{num, obj, str as jstr, Json};

fn probbit(args: &[&str], stdin: &str) -> (i32, String, String) {
    let mut c = Command::new(env!("CARGO_BIN_EXE_probbit")).args(args).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().unwrap();
    let _ = c.stdin.take().unwrap().write_all(stdin.as_bytes());
    let o = c.wait_with_output().unwrap();
    (o.status.code().unwrap_or(-1), String::from_utf8_lossy(&o.stdout).into_owned(), String::from_utf8_lossy(&o.stderr).into_owned())
}
fn parse(s: &str) -> Json { json::parse(s).unwrap_or_else(|e| panic!("not one JSON document ({}): {s:.300}", e.msg)) }
/// The document without its wall-clock fields (the same normalization as the golden tests)
fn stable(j: &Json) -> Json {
    match j { Json::Obj(v) => Json::Obj(v.iter().filter(|(k, _)| !["ms", "sample_ms", "gate_ms", "site_updates_per_s", "process_cpu_ms", "peak_rss_mb", "nice", "phases"].contains(&k.as_str())).map(|(k, x)| (k.clone(), stable(x))).collect()),
        Json::Arr(v) => Json::Arr(v.iter().map(stable).collect()), x => x.clone() }
}
const EX: &str = include_str!("../../examples/evaluate/support-12.json");

/// splitmix64: a seeded stream for the random requests
struct Rng(u64);
impl Rng { fn next(&mut self) -> u64 { self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15); let mut z = self.0; z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9); z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB); z ^ (z >> 31) }
    fn f(&mut self) -> f64 { (self.next() >> 11) as f64 / (1u64 << 53) as f64 } fn n(&mut self, k: usize) -> usize { (self.next() % k as u64) as usize } }

/// A random question set: (id, type, options) and the judge's probabilities per option (each >= 0.01, summing to 1, rounded to
/// 4 decimals so the argmax is unique by construction and the request round-trips through the printer).
struct Q { id: String, ty: &'static str, opts: Vec<String>, p: Vec<f64> }
fn questions(r: &mut Rng, n: usize) -> Vec<Q> {
    (0..n).map(|i| { let ty = ["noul", "choice", "score"][r.n(3)];
        let opts: Vec<String> = match ty { "noul" => vec!["false".into(), "true".into()], "choice" => (0..2 + r.n(4)).map(|k| format!("o{}", k + i % 3)).collect(), _ => (0..2 + r.n(4)).map(|l| l.to_string()).collect() };
        let raw: Vec<f64> = loop { let w: Vec<f64> = (0..opts.len()).map(|_| 0.05 + r.f()).collect(); let s: f64 = w.iter().sum();
            let p: Vec<f64> = w.iter().map(|x| (x / s * 1e4).round() / 1e4).collect(); let mut q = p.clone(); q.sort_by(|a, b| b.total_cmp(a));
            if q[0] - q[1] > 2e-3 { break p; } };
        let mut p = raw; let s: f64 = p.iter().sum(); let last = p.len() - 1; p[last] = ((p[last] + 1.0 - s) * 1e4).round() / 1e4; // sum 1 to print precision
        if ty == "noul" { p[0] = ((1.0 - p[1]) * 1e4).round() / 1e4; }
        Q { id: format!("q{i}"), ty, opts, p } }).collect()
}
fn argmax(p: &[f64]) -> usize { (0..p.len()).fold(0, |b, k| if p[k] > p[b] { k } else { b }) }
/// The request: questions (criteria built from the options) + the judge's answers in the System One shape, + `rules` if any
fn request(qs: &[Q], rules: Option<Json>) -> String {
    let question = |q: &Q| match q.ty { "noul" => obj(vec![("type", jstr("noul")), ("instructions", jstr("yes or no?"))]),
        "choice" => obj(vec![("type", jstr("choice")), ("criteria", Json::Obj(q.opts.iter().map(|o| (o.clone(), Json::Null)).collect()))]),
        _ => obj(vec![("type", jstr("score")), ("criteria", Json::Arr(q.opts.iter().map(|o| jstr(&format!("level {o}"))).collect()))]) };
    let answer = |q: &Q| match q.ty { "noul" => obj(vec![("type", jstr("noul")), ("noul", num(q.p[1]))]),
        t => obj(vec![("type", jstr(t)), ("probabilities", Json::Obj(q.opts.iter().zip(&q.p).map(|(o, &p)| (o.clone(), num(p))).collect()))]) };
    let mut pb = vec![("judge", obj(vec![("model", jstr("judge-x")), ("answers", Json::Obj(qs.iter().map(|q| (q.id.clone(), answer(q))).collect()))]))];
    if let Some(r) = rules { pb.push(("rules", r)); }
    json::write(&obj(vec![("model", jstr("judge-x")), ("state", jstr("ignored")), ("questions", Json::Obj(qs.iter().map(|q| (q.id.clone(), question(q))).collect())), ("probbit", obj(pb))]), false)
}

#[test]
fn without_rules_the_answer_is_the_judge_argmax_with_the_same_probabilities() {
    let mut r = Rng(11); let mut worst = 0.0f64;
    for case in 0..150 {
        let k = 1 + r.n(12); let qs = questions(&mut r, k); let (c, out, err) = probbit(&["evaluate"], &request(&qs, None)); assert_eq!(c, 0, "case {case}: {err}");
        let d = parse(&out); assert_eq!(d.get("verdict").and_then(Json::as_str), Some("exact"), "case {case}");
        assert_eq!(d.get("model").and_then(Json::as_str), Some("judge-x")); assert_eq!(d.get("violations").and_then(Json::as_f64), Some(0.0));
        let ans = d.get("answers").unwrap();
        for q in &qs { let a = ans.get(&q.id).unwrap(); let x = a.get("probbit").unwrap(); let top = &q.opts[argmax(&q.p)];
            assert_eq!(x.get("value").and_then(Json::as_str), Some(top.as_str()), "case {case} {}: plan = the judge's argmax", q.id);
            assert_eq!(x.get("judge").and_then(Json::as_str), Some(top.as_str())); assert_eq!(x.get("changed"), Some(&Json::Bool(false))); assert_eq!(x.get("released"), Some(&Json::Bool(true)));
            let got: Vec<f64> = match q.ty { "noul" => { let y = a.get("noul").and_then(Json::as_f64).unwrap(); vec![1.0 - y, y] }
                _ => q.opts.iter().map(|o| a.get("probabilities").and_then(|p| p.get(o)).and_then(Json::as_f64).unwrap()).collect() };
            for (g, w) in got.iter().zip(&q.p) { worst = worst.max((g - w).abs()); assert!((g - w).abs() <= 2e-6, "case {case} {}: {got:?} vs {:?}", q.id, q.p); }
            if q.ty == "choice" { assert_eq!(a.get("choice").and_then(Json::as_str), Some(top.as_str())); }
            if q.ty == "score" { let e: f64 = q.p.iter().enumerate().map(|(l, p)| l as f64 * p).sum(); assert!((a.get("score").and_then(Json::as_f64).unwrap() - e).abs() <= 1e-5); } }
    }
    eprintln!("largest |probbit - judge| probability gap over 150 rule-free requests: {worst:e}");
}

/// Rules over random pairs of questions (implies, a forbidden pair table, all_different on two choice questions; half of the
/// implies / tables aimed at the judge's own answers so that they bite). With components = the questions linked by rules: a
/// component whose argmax satisfies its rules keeps the argmax; one where the argmax breaks a rule moves at least one answer;
/// nothing outside a broken component moves; violations 0 (checked here too); and in every broken component the answer is the
/// most likely assignment that satisfies its rules (brute force over the component, the same log-weights ln max(p, 1e-6)).
#[test]
fn with_rules_only_broken_components_move() {
    let mut r = Rng(29); let (mut informative, mut moved_cases, mut kept_cases) = (0, 0, 0);
    for case in 0..300 {
        let k = 3 + r.n(10); let qs = questions(&mut r, k); let n = qs.len(); let mut rules_i = vec![]; let mut rules_t = vec![]; let mut rules_a = vec![];
        let judge: Vec<usize> = qs.iter().map(|q| argmax(&q.p)).collect();
        let mut links: Vec<(usize, usize)> = vec![]; // (x, y) questions tied by one rule
        let mut checks: Vec<Check> = vec![];
        for _ in 0..1 + r.n(4) { let (x, y) = (r.n(n), r.n(n)); if x == y { continue; } let (ox, oy) = (qs[x].opts.len(), qs[y].opts.len()); let bite = r.n(2) == 0;
            match r.n(3) {
                0 => { let a = if bite { judge[x] } else { r.n(ox) }; let keep: Vec<usize> = (0..oy).filter(|&o| !(bite && o == judge[y]) && r.f() < 0.5).collect(); if keep.is_empty() { continue; }
                    rules_i.push(obj(vec![("if", obj(vec![("var", jstr(&qs[x].id)), ("value", jstr(&qs[x].opts[a]))])), ("then", obj(vec![("var", jstr(&qs[y].id)), ("in", Json::Arr(keep.iter().map(|&k| jstr(&qs[y].opts[k])).collect()))]))]));
                    checks.push(Box::new(move |v: &[usize]| v[x] != a || keep.contains(&v[y]))); }
                1 => { let (a, b) = if bite { (judge[x], judge[y]) } else { (r.n(ox), r.n(oy)) };
                    rules_t.push(obj(vec![("vars", Json::Arr(vec![jstr(&qs[x].id), jstr(&qs[y].id)])), ("forbid", Json::Arr(vec![Json::Arr(vec![jstr(&qs[x].opts[a]), jstr(&qs[y].opts[b])])]))]));
                    checks.push(Box::new(move |v: &[usize]| !(v[x] == a && v[y] == b))); }
                _ => { if qs[x].ty != "choice" || qs[y].ty != "choice" { continue; } rules_a.push(obj(vec![("vars", Json::Arr(vec![jstr(&qs[x].id), jstr(&qs[y].id)]))]));
                    let (ox_, oy_) = (qs[x].opts.clone(), qs[y].opts.clone()); checks.push(Box::new(move |v: &[usize]| ox_[v[x]] != oy_[v[y]])); } }
            links.push((x, y)); }
        if links.is_empty() { continue; }
        let rules: Vec<(&str, Json)> = [("implies", rules_i), ("tables", rules_t), ("all_different", rules_a)].into_iter().filter(|(_, r)| !r.is_empty()).map(|(k, r)| (k, Json::Arr(r))).collect();
        let (c, out, err) = probbit(&["evaluate"], &request(&qs, Some(obj(rules))));
        if c == 1 { continue; } // the random rules admit no answer (infeasible is a proof, exit 1)
        assert_eq!(c, 0, "case {case}: {err} {out:.300}"); let d = parse(&out); assert_eq!(d.get("verdict").and_then(Json::as_str), Some("exact"));
        assert_eq!(d.get("violations").and_then(Json::as_f64), Some(0.0), "case {case}"); informative += 1;
        let plan: Vec<usize> = qs.iter().map(|q| { let v = d.get("plan").and_then(|p| p.get(&q.id)).and_then(Json::as_str).unwrap(); q.opts.iter().position(|o| o == v).unwrap() }).collect();
        assert!(checks.iter().all(|f| f(&plan)), "case {case}: the plan breaks a rule");
        // components of the rule graph
        let mut comp: Vec<usize> = (0..n).collect(); fn root(c: &mut [usize], i: usize) -> usize { let mut i = i; while c[i] != i { c[i] = c[c[i]]; i = c[i]; } i }
        for &(x, y) in &links { let (a, b) = (root(&mut comp, x), root(&mut comp, y)); comp[a] = b; }
        let roots: Vec<usize> = (0..n).map(|i| root(&mut comp, i)).collect();
        let broken: std::collections::HashSet<usize> = links.iter().zip(&checks).filter(|(_, f)| !f(&judge)).map(|(&(x, _), _)| roots[x]).collect();
        for i in 0..n { if !broken.contains(&roots[i]) { assert_eq!(plan[i], judge[i], "case {case}: {} moved outside every broken component", qs[i].id); } }
        kept_cases += links.iter().map(|&(x, _)| roots[x]).collect::<std::collections::HashSet<_>>().difference(&broken).count();
        let lw = |i: usize, o: usize| (q_p(&qs[i], o).max(1e-6).ln() * 1e6).round() / 1e6; // the compiled log-weight
        for &b in &broken { let mem: Vec<usize> = (0..n).filter(|&i| roots[i] == b).collect();
            assert!(mem.iter().any(|&i| plan[i] != judge[i]), "case {case}: a broken component kept the argmax"); moved_cases += 1;
            // brute force over the component: the plan's part must be a most likely assignment that satisfies every rule
            let (mut best, mut v, mut idx) = (f64::NEG_INFINITY, plan.clone(), vec![0usize; mem.len()]);
            loop { for (t, &i) in mem.iter().enumerate() { v[i] = idx[t]; }
                if checks.iter().all(|f| f(&v)) { best = best.max(mem.iter().map(|&i| lw(i, v[i])).sum()); }
                let mut t = 0; while t < mem.len() { idx[t] += 1; if idx[t] < qs[mem[t]].opts.len() { break; } idx[t] = 0; t += 1; } if t == mem.len() { break; } }
            let got: f64 = mem.iter().map(|&i| lw(i, plan[i])).sum();
            assert!((got - best).abs() < 1e-9, "case {case}: component log-weight {got} but the best rule-abiding assignment has {best}"); }
        let changed: Vec<bool> = qs.iter().map(|q| d.get("answers").and_then(|a| a.get(&q.id)).and_then(|a| a.get("probbit")).and_then(|x| x.get("changed")) == Some(&Json::Bool(true))).collect();
        assert_eq!(changed, (0..n).map(|i| plan[i] != judge[i]).collect::<Vec<_>>(), "case {case}: `changed` marks exactly the moved answers");
    }
    eprintln!("rules: {informative} answered requests, {moved_cases} broken components (moved, = brute-force MAP), {kept_cases} rule components the argmax already satisfied (kept)");
    assert!(informative >= 200 && moved_cases >= 100 && kept_cases >= 50, "too few informative cases: {informative} answered, {moved_cases} broken, {kept_cases} kept");
}
fn q_p(q: &Q, o: usize) -> f64 { q.p[o] }
/// A rule as a predicate on an assignment (option index per question)
type Check = Box<dyn Fn(&[usize]) -> bool>;

/// A hard instance on a budget too small for the gate: `refused`, exit 3 as `probbit run`, every answer present and unreleased.
#[test]
fn a_refusal_keeps_the_exit_code_and_releases_nothing() {
    let mut r = Rng(5); let qs: Vec<Q> = questions(&mut r, 40).into_iter().map(|mut q| { if q.ty == "noul" { q.ty = "score"; q.opts = (0..3).map(|l| l.to_string()).collect(); q.p = vec![0.3, 0.3001, 0.3999]; } q }).collect();
    let ids: Vec<Json> = qs.iter().filter(|q| q.ty == "score").map(|q| jstr(&q.id)).collect();
    let req = request(&qs, Some(obj(vec![("caps", Json::Arr(vec![obj(vec![("value", jstr("2")), ("vars", Json::Arr(ids.clone())), ("limit", num(3.0))])]))])));
    let flags = ["--op", "sample", "--sweeps", "30", "--polish-ms", "0"];
    let (c, out, err) = probbit(&[&["evaluate"][..], &flags].concat(), &req); assert_eq!(c, 3, "{err} {out:.300}");
    let d = parse(&out); assert_eq!(d.get("verdict").and_then(Json::as_str), Some("refused"));
    assert_eq!(d.get("released").and_then(Json::as_arr).map(|a| a.len()), Some(0)); assert_eq!(d.get("escalated").and_then(Json::as_arr).map(|a| a.len()), Some(qs.len()));
    for q in &qs { let a = d.get("answers").and_then(|a| a.get(&q.id)).unwrap_or_else(|| panic!("no answer for {}", q.id));
        assert_eq!(a.get("probbit").and_then(|x| x.get("released")), Some(&Json::Bool(false))); }
    let (_, prog, _) = probbit(&["evaluate", "--program"], &req); let (c2, out2, _) = probbit(&[&["run"][..], &flags].concat(), &prog);
    assert_eq!(c2, 3); assert_eq!(parse(&out2).get("verdict"), d.get("verdict"));
    // no plan at all (the rules admit none): no `answers`, the verdict says why, exit 1 as `probbit run`
    let qs = questions(&mut Rng(3), 2);
    let bad: Vec<Json> = qs[0].opts.iter().map(|o| obj(vec![("vars", Json::Arr(vec![jstr(&qs[0].id)])), ("forbid", Json::Arr(vec![Json::Arr(vec![jstr(o)])]))])).collect();
    let (c, out, _) = probbit(&["evaluate"], &request(&qs, Some(obj(vec![("tables", Json::Arr(bad))])))); assert_eq!(c, 1);
    let d = parse(&out); assert_eq!(d.get("verdict").and_then(Json::as_str), Some("infeasible")); assert!(d.get("answers").is_none()); assert!(d.get("usage").is_some());
}

#[test]
fn bad_requests_are_one_error_object_located_in_the_request() {
    let q = r#""questions": {"a": {"type": "choice", "criteria": {"x": null, "y": null}}, "b": {"type": "noul"}}"#;
    let cases: [(String, &str, &str); 21] = [
        ("not json".into(), "schema", ""), (r#"{"state": "s"}"#.into(), "schema", "questions"), (format!(r#"{{{q}, "extra": 1}}"#), "schema", "extra"),
        (r#"{"questions": {}}"#.into(), "value", "questions"), (r#"{"questions": {"a": {"type": "boolean"}}}"#.into(), "value", "questions.a.type"),
        (r#"{"questions": {"a": {"type": "choice"}}}"#.into(), "schema", "questions.a.criteria"), (r#"{"questions": {"a": {"type": "score", "criteria": []}}}"#.into(), "value", "questions.a.criteria"),
        (r#"{"questions": {"a": {"type": "noul", "criteria": {"maybe": 1}}}}"#.into(), "schema", "questions.a.criteria.maybe"),
        (format!(r#"{{{q}, "probbit": {{"judge": {{"answers": {{"c": {{"type": "noul", "noul": 0.5}}}}}}}}}}"#), "value", "probbit.judge.answers.c"),
        (format!(r#"{{{q}, "probbit": {{"judge": {{"answers": {{"a": {{"type": "noul", "noul": 0.5}}}}}}}}}}"#), "value", "probbit.judge.answers.a.type"),
        (format!(r#"{{{q}, "probbit": {{"weights": {{"a": {{"z": 0.5}}}}}}}}"#), "value", "probbit.weights.a.z"),
        (format!(r#"{{{q}, "probbit": {{"weights": {{"b": 1.5}}}}}}"#), "value", "probbit.weights.b"),
        (format!(r#"{{{q}, "probbit": {{"weights": {{"b": 0.5}}, "logw": {{"b": {{"true": 1}}}}}}}}"#), "value", "probbit.logw.b"),
        (format!(r#"{{{q}, "probbit": {{"rules": {{"implies": [{{"if": {{"var": "a", "value": "x"}}, "then": {{"var": "zz", "in": ["true"]}}}}]}}}}}}"#), "value", "probbit.rules.implies[0].then.var"),
        (format!(r#"{{{q}, "probbit": {{"rules": {{"start": {{}}}}}}}}"#), "schema", "probbit.rules.start"), (format!(r#"{{{q}, "probbit": {{"floor": 0}}}}"#), "value", "probbit.floor"),
        // a rule naming an option its question does not have (another question's option is in the shared alphabet): an error, not a silent exclusion
        (format!(r#"{{{q}, "probbit": {{"rules": {{"implies": [{{"if": {{"var": "a", "value": "x"}}, "then": {{"var": "b", "in": ["y"]}}}}]}}}}}}"#), "value", "probbit.rules.implies[0].then.in[0]"),
        (format!(r#"{{{q}, "probbit": {{"rules": {{"implies": [{{"if": {{"var": "a", "value": "true"}}, "then": {{"var": "b", "in": ["true"]}}}}]}}}}}}"#), "value", "probbit.rules.implies[0].if.value"),
        (format!(r#"{{{q}, "probbit": {{"rules": {{"tables": [{{"vars": ["a", "b"], "forbid": [["x", "x"]]}}]}}}}}}"#), "value", "probbit.rules.tables[0].forbid[0][1]"),
        (format!(r#"{{{q}, "probbit": {{"rules": {{"caps": [{{"value": "x", "vars": ["b"], "limit": 1}}]}}}}}}"#), "value", "probbit.rules.caps[0].value"),
        (format!(r#"{{{q}, "probbit": {{"rules": {{"linear": [{{"terms": [["b", "y", 2]], "limit": 1}}]}}}}}}"#), "value", "probbit.rules.linear[0].terms[0][1]")];
    for (doc, code, path) in &cases {
        let (c, out, err) = probbit(&["evaluate"], doc); assert_eq!(c, 2, "{doc}: {out} {err}");
        assert_eq!(out.lines().count(), 1, "{doc}: one line"); let e = parse(&out); let e = e.get("error").unwrap_or_else(|| panic!("{doc}: {out}"));
        assert_eq!((e.get("code").and_then(Json::as_str), e.get("path").and_then(Json::as_str)), (Some(*code), Some(*path)), "{doc}: {out}");
        assert!(err.starts_with("probbit: ") && err.lines().count() == 1, "{doc}: {err}"); }
    let (c, out, err) = probbit(&["evaluate", "--nope"], EX); assert_eq!((c, out.as_str()), (2, ""), "flag errors stay on stderr: {err}");
}

/// `--program` prints the compiled program; `probbit run` on it gives the evaluate document minus `model`, `answers` and `usage`
/// (exact tiers, and the sampler at fixed work), with the same exit code.
#[test]
fn the_program_round_trips_through_run() {
    let mut no_rules: Json = parse(EX); if let Json::Obj(v) = &mut no_rules { for (k, x) in v.iter_mut() { if k == "probbit" { if let Json::Obj(p) = x { p.retain(|(k, _)| k != "rules"); } } } }
    let sampled = ["--op", "sample", "--sweeps", "2000", "--polish-ms", "0"]; let short = ["--op", "sample", "--sweeps", "400", "--polish-ms", "0"];
    // exact (exit 0), sampled at fixed work with and without the rules (0: diagnostics_passed), and a budget too small for the
    // gate (3: refused; 1,440 samples, bound 0.071 > 0.05): the same code and document from both commands
    let no_rules = json::write(&no_rules, false);
    for (doc, flags, want) in [(EX, &[][..], 0), (no_rules.as_str(), &[][..], 0), (no_rules.as_str(), &sampled[..], 0), (EX, &sampled[..], 0), (EX, &short[..], 3)] {
        let (c1, ev, e1) = probbit(&[&["evaluate"][..], flags].concat(), doc); let (c0, prog, _) = probbit(&["evaluate", "--program"], doc); assert_eq!(c0, 0);
        let p = parse(&prog); assert_eq!(p.get("probbit_ir").and_then(Json::as_f64), Some(1.0)); assert_eq!(p.get("vars").and_then(Json::as_arr).map(|v| v.len()), Some(12));
        let (c2, run, e2) = probbit(&[&["run"][..], flags].concat(), &prog); assert_eq!((c1, c2), (want, want), "{flags:?}: {e1} {e2}");
        let Json::Obj(mut v) = stable(&parse(&ev)) else { panic!() }; v.retain(|(k, _)| !["model", "answers", "usage"].contains(&k.as_str()));
        assert_eq!(Json::Obj(v), stable(&parse(&run)), "{flags:?}: evaluate's probbit fields differ from `probbit run` on its program"); }
}

/// The verified System One response shape (TypeSafe OpenAPI 0.2.0, Workers AI clef schema-output.json): `model` string, `answers`
/// keyed by question id with the answer types' required fields, `usage` with integer token counts; plus `probbit` per answer. The
/// 12-question example: the judge's own record breaks 5 rules; probbit moves 5 answers, exact, 0 violations.
#[test]
fn answers_have_the_vendor_shape() {
    let (c, out, err) = probbit(&["evaluate"], EX); assert_eq!(c, 0, "{err}"); let d = parse(&out); let req = parse(EX);
    assert_eq!(d.get("model").and_then(Json::as_str), Some("example-judge")); assert_eq!(d.get("usage"), req.get("probbit").and_then(|p| p.get("judge")).and_then(|j| j.get("usage")));
    let Some(Json::Obj(ans)) = d.get("answers") else { panic!("answers") }; let Some(Json::Obj(qs)) = req.get("questions") else { panic!() };
    assert_eq!(ans.iter().map(|(k, _)| k).collect::<Vec<_>>(), qs.iter().map(|(k, _)| k).collect::<Vec<_>>(), "one answer per question, in order");
    let unit = |x: Option<&Json>| { let p = x.and_then(Json::as_f64).unwrap(); assert!((0.0..=1.0).contains(&p), "{p}"); p };
    for (id, a) in ans { let q = &qs.iter().find(|(k, _)| k == id).unwrap().1; let ty = q.get("type").and_then(Json::as_str).unwrap(); assert_eq!(a.get("type").and_then(Json::as_str), Some(ty));
        match ty {
            "noul" => { unit(a.get("noul")); }
            "choice" => { let ks: Vec<&String> = q.get("criteria").and_then(Json::as_obj).unwrap().iter().map(|(k, _)| k).collect(); let ch = a.get("choice").and_then(Json::as_str).unwrap();
                assert!(ks.iter().any(|k| *k == ch)); unit(a.get("confidence")); let pr = a.get("probabilities").and_then(Json::as_obj).unwrap();
                assert_eq!(pr.iter().map(|(k, _)| k).collect::<Vec<_>>(), ks); let s: f64 = pr.iter().map(|(_, p)| unit(Some(p))).sum(); assert!((s - 1.0).abs() < 1e-5); }
            _ => { let l = q.get("criteria").and_then(Json::as_arr).unwrap().len(); let sc = a.get("score").and_then(Json::as_f64).unwrap(); assert!(sc >= 0.0 && sc <= (l - 1) as f64);
                unit(a.get("confidence")); let lg = a.get("legend").and_then(Json::as_obj).unwrap(); assert_eq!(lg.len(), l);
                let pr = a.get("probabilities").and_then(Json::as_obj).unwrap(); assert_eq!(pr.iter().map(|(k, _)| k.clone()).collect::<Vec<_>>(), (0..l).map(|x| x.to_string()).collect::<Vec<_>>()); } }
        let x = a.get("probbit").unwrap(); unit(x.get("p")); assert!(x.get("value").and_then(Json::as_str).is_some()); }
    let moved: Vec<&str> = ans.iter().filter(|(_, a)| a.get("probbit").and_then(|x| x.get("changed")) == Some(&Json::Bool(true))).map(|(k, _)| k.as_str()).collect();
    assert_eq!(moved, ["team", "severity", "refund_action", "escalate_to_human", "reply_channel"]);
    assert_eq!((d.get("verdict").and_then(Json::as_str), d.get("violations").and_then(Json::as_f64)), (Some("exact"), Some(0.0)));
    // --summary: the answers stay, the per-item tables go, counts by question
    let (c, out, _) = probbit(&["evaluate", "--summary"], EX); assert_eq!(c, 0); let s = parse(&out);
    assert_eq!(s.get("summary").and_then(Json::as_f64), Some(1.0)); assert_eq!(s.get("answers"), d.get("answers")); assert!(s.get("plan").is_none() && s.get("marginals").is_none());
    assert_eq!(s.get("counts").and_then(|c| c.get("vars")).and_then(Json::as_f64), Some(12.0));
}

/// The three ways to give weights compile to the same log-weights: the judge's answers, `weights` (probabilities as the judge
/// returns them) and `logw` (ln p, 6 decimals); p = 0 becomes the floor (1e-6, or `probbit.floor`).
#[test]
fn weights_judge_and_logw_are_the_same_program() {
    let q = r#""questions": {"a": {"type": "choice", "criteria": {"x": null, "y": null, "z": null}}, "b": {"type": "noul"}, "c": {"type": "score", "criteria": ["lo", "hi"]}}"#;
    let docs = [format!(r#"{{{q}, "probbit": {{"judge": {{"answers": {{"a": {{"type": "choice", "choice": "x", "probabilities": {{"x": 0.7, "y": 0.3, "z": 0}}}}, "b": {{"type": "noul", "noul": 0.2}}, "c": {{"type": "score", "score": 0.9, "probabilities": {{"0": 0.1, "1": 0.9}}}}}}}}}}}}"#),
        format!(r#"{{{q}, "probbit": {{"weights": {{"a": {{"x": 0.7, "y": 0.3}}, "b": 0.2, "c": {{"0": 0.1, "1": 0.9}}}}}}}}"#),
        format!(r#"{{{q}, "probbit": {{"logw": {{"a": {{"x": -0.356675, "y": -1.203973, "z": -13.815511}}, "b": {{"false": -0.223144, "true": -1.609438}}, "c": {{"0": -2.302585, "1": -0.105361}}}}}}}}"#)];
    let progs: Vec<String> = docs.iter().map(|d| { let (c, p, e) = probbit(&["evaluate", "--program"], d); assert_eq!(c, 0, "{e}"); p }).collect();
    assert_eq!(progs[0], progs[1]); assert_eq!(progs[0], progs[2]); assert!(progs[0].contains(r#""z":-13.815511"#), "{}", progs[0]);
    let (_, p, _) = probbit(&["evaluate", "--program"], &format!(r#"{{{q}, "probbit": {{"floor": 0.001, "weights": {{"a": {{"x": 1}}}}}}}}"#)); assert!(p.contains(r#""y":-6.907755"#), "{p}");
    // no weights at all: every option 0 (uniform), `judge` null
    let (c, out, _) = probbit(&["evaluate"], &format!("{{{q}}}")); assert_eq!(c, 0); let d = parse(&out);
    assert_eq!(d.get("answers").and_then(|a| a.get("b")).and_then(|b| b.get("noul")).and_then(Json::as_f64), Some(0.5));
    assert_eq!(d.get("answers").and_then(|a| a.get("a")).and_then(|b| b.get("probbit")).and_then(|x| x.get("judge")), Some(&Json::Null));
    assert_eq!(d.get("model").and_then(Json::as_str), Some("probbit"));
}

/// An exact tie in the judge's weights is no change: `judge` is the tied option the plan holds and `changed` is false; when a
/// rule removes every tied option, `judge` is the first of them and the move is a change.
#[test]
fn an_exact_tie_is_no_change() {
    let q = r#""questions": {"a": {"type": "noul"}, "b": {"type": "choice", "criteria": {"x": null, "y": null, "z": null}}}"#;
    let (c, out, _) = probbit(&["evaluate"], &format!(r#"{{{q}, "probbit": {{"weights": {{"a": 0.5, "b": {{"x": 0.4, "y": 0.4, "z": 0.2}}}}}}}}"#)); assert_eq!(c, 0); let d = parse(&out);
    for id in ["a", "b"] { let x = d.get("answers").and_then(|a| a.get(id)).and_then(|a| a.get("probbit")).unwrap();
        assert_eq!(x.get("judge"), x.get("value"), "{id}"); assert_eq!(x.get("changed"), Some(&Json::Bool(false)), "{id}"); }
    let (c, out, _) = probbit(&["evaluate"], &format!(r#"{{{q}, "probbit": {{"weights": {{"b": {{"x": 0.4, "y": 0.4, "z": 0.2}}}}, "rules": {{"tables": [{{"vars": ["b"], "forbid": [["x"], ["y"]]}}]}}}}}}"#));
    assert_eq!(c, 0); let x = parse(&out); let x = x.get("answers").and_then(|a| a.get("b")).and_then(|a| a.get("probbit")).unwrap();
    assert_eq!((x.get("value").and_then(Json::as_str), x.get("judge").and_then(Json::as_str), x.get("changed")), (Some("z"), Some("x"), Some(&Json::Bool(true))));
}
