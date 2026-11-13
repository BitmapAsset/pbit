//! `probbit monitor STRAND` (docs/persona.md §5.8): watch an individual's internal state as live horizontal bars. The monitor
//! replays the strand with `live::Replay`, the rules `probbit live verify` uses, so it recomputes every stance document from
//! the strand alone (no other log, any strand, any machine) and says whether the replay checks. It draws the latest event: the
//! stance (each trait's levels as segments with their exact odds, the level taken highlighted, the phrase it says), the moods
//! with a sparkline of the last 50 events, the inputs (the event's inputs and the history features), the senses when the
//! persona declares them (§2.11: per sense its normal as a tick over the range, the reading, z and n, and an alert when a
//! normal moved over drift_cap / 2 within 50 events), the habits in force and the ones that bound, the learned deltas, the
//! decisions awaiting an outcome with their eligibility (§2.8, with a credit block), the drives when the documents carry them
//! (feature-detected), and the stance line.
//! Under it, the panel (`panel.rs`, docs/persona.md §5.8): every field of the individual as a dial with fixed words, one pure
//! map from this event's reading and the event before's.
//! `--serve` draws the same in a browser: one page embedded in the binary, fed by server-sent events from a server on
//! 127.0.0.1. It changes no file (the demo's week goes through a temporary one, removed at once) and sends nothing anywhere.
use crate::json::{self, Json};
use crate::labels::{self, Labels};
use crate::live::Replay;
use crate::persona::{self, Engine};
use crate::theme::{self, Col, Theme, CYAN, DIM, GREEN, GREY, MAGENTA, YELLOW};
use std::collections::VecDeque;
use std::io::{Read, Seek, Write};
use std::net::{Ipv4Addr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;
mod panel;

/// Moods (warm amber), wanting (orange), a line that diverges or a prediction error below zero (red), the shades of the levels
/// not taken
const AMBER: Col = Col(214, 33);
const ORANGE: Col = Col(208, 33);
const RED: Col = Col(196, 91);
const SHADE: [Col; 2] = [Col(244, 37), Col(239, 90)];
/// Events a mood's sparkline covers
const SPARK: usize = 50;
/// The tutor persona `--demo` plays (a copy of examples/persona/tutor.yaml; a test keeps them equal)
const TUTOR: &str = include_str!("demo-tutor.yaml");
/// Cells of a level bar and of a number bar
const BAR: usize = 20;
const NBAR: usize = 8;
/// The page `--serve` serves at `/`: one file, its styles and script inline; it fetches nothing but `/events` from its own server
const PAGE: &str = include_str!("monitor.html");
/// Seconds between heartbeats on a quiet event stream
const BEAT: u64 = 15;
/// Stance documents `/doc/N` keeps (the latest; about 2.5 KB each)
const KEEP: usize = 10_000;
/// Connections served at once (more are refused with 503)
const CONNS: usize = 64;

fn strs(j: Option<&Json>) -> Vec<String> { j.and_then(Json::as_arr).map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect()).unwrap_or_default() }

/// A trait or a mood: its id, levels and per-level phrases, in the persona's order
struct Var { id: String, levels: Vec<String>, say: Vec<String> }
/// A declared input: its id, kind (flag | level | number) and, for a number, its max
struct Input { id: String, kind: String, max: f64 }
/// A declared sense (§2.11), in declared order: its range on the scale its normal is kept on (a flag's is [0, 1])
struct SenseRow { id: String, say: String, unit: String, flag: bool, log1p: bool, lo: f64, hi: f64, drift_cap: f64 }
/// A declared need (RFC 0002 A.1), in declared order: its phrase, unit, cap and the goal it pays
struct NeedRow { id: String, say: String, unit: String, cap: f64, goal: String }
impl SenseRow {
    /// A reading or a range end on the normal's scale
    fn scaled(&self, x: f64) -> f64 { if self.log1p { x.ln_1p() } else { x } }
}
/// What the layout reads from the strand's header
struct Meta { name: String, version: String, seed: u64, individual: String, engine: String, traits: Vec<Var>, moods: Vec<Var>, inputs: Vec<Input>,
    history: Vec<(String, f64)>, learning: Option<(Vec<String>, f64)>, caps: (f64, f64, f64), goals: Vec<String>, senses: Vec<SenseRow>, needs: Vec<NeedRow>,
    pace: Option<(f64, f64)>, gates: Vec<String>,
    /// The face's mood meters: the two moods `display.glyphs` names (rows, then columns; §2.18), else the persona's first two
    face: Vec<String>,
    /// For the panel: the surprise gate's g_max, the credit block's max_open, the habits declared, a dispositions block
    g_max: f64, max_open: Option<usize>, n_habits: usize, dispositions: bool }
impl Meta {
    fn of(r: &Replay) -> Meta {
        let d = persona::describe(&r.live.p);
        let obj = |k: &str| d.get(k).and_then(Json::as_obj).map(|o| o.to_vec()).unwrap_or_default();
        let decl = |k: &str| r.doc.get(k).and_then(Json::as_arr).map(|a| a.to_vec()).unwrap_or_default();
        let max = |id: &str| decl("inputs").iter().find(|i| i.get("id").and_then(Json::as_str) == Some(id)).and_then(|i| i.get("max")).and_then(Json::as_f64).unwrap_or(1.0);
        let cap = |b: &str, k: &str, d: f64| r.doc.get("drives").and_then(|x| x.get(b)).and_then(|x| x.get(k)).and_then(Json::as_f64).unwrap_or(d);
        Meta { name: r.live.p.name.clone(), version: r.live.p.version.clone(), seed: r.live.st.seed,
            individual: r.header.get("state").and_then(|s| s.get("digest")).and_then(Json::as_str).unwrap_or("").to_string(),
            engine: r.header.get("engine").and_then(Json::as_str).unwrap_or("").to_string(),
            traits: obj("traits").into_iter().map(|(id, t)| Var { levels: strs(t.get("levels")), say: strs(t.get("say")), id }).collect(),
            moods: obj("moods").into_iter().map(|(id, l)| Var { levels: strs(Some(&l)), say: vec![], id }).collect(),
            inputs: obj("inputs").into_iter().map(|(id, x)| Input { kind: x.get("kind").and_then(Json::as_str).unwrap_or("").to_string(), max: max(&id), id }).collect(),
            history: decl("history").iter().filter_map(|h| Some((h.get("id")?.as_str()?.to_string(), h.get("cap").and_then(Json::as_f64).unwrap_or(5.0)))).collect(),
            learning: d.get("learning").map(|l| (strs(l.get("traits")), l.get("total_cap").and_then(Json::as_f64).unwrap_or(1.0))),
            caps: (cap("wanting", "cap", 3.0), cap("afterglow", "cap", 2.0), cap("expectation", "pe_cap", 1.0)),
            // a drives block's goals, in the order of the levels of `pursue` (describe hides that trait; its learned row needs them)
            goals: strs(d.get("drives").and_then(|x| x.get("goals"))),
            senses: persona::normals(&r.live.p, &r.live.st).into_iter().map(|(id, _, _, drift_cap)| {
                let x = d.get("senses").and_then(|o| o.get(&id)); let f = |k: &str| x.and_then(|x| x.get(k));
                let range = f("range").and_then(Json::as_arr).map(|a| (a[0].as_f64().unwrap_or(0.0), a[1].as_f64().unwrap_or(1.0)));
                let txt = |k: &str| f(k).and_then(Json::as_str).unwrap_or("").to_string();
                let (flag, log1p) = (txt("kind") == "flag", txt("scale") == "log1p");
                let (lo, hi) = range.unwrap_or((0.0, 1.0)); let sc = |v: f64| if log1p { v.ln_1p() } else { v };
                SenseRow { say: txt("say"), unit: txt("unit"), flag, log1p, lo: sc(lo), hi: sc(hi), drift_cap, id } }).collect(),
            // a needs block (RFC 0002), from describe's items in declared order
            needs: d.get("needs").and_then(|n| n.get("items")).and_then(Json::as_obj).map_or(vec![], |o| o.iter().map(|(id, x)| { let t = |k: &str| x.get(k).and_then(Json::as_str).unwrap_or("").to_string();
                NeedRow { say: t("say"), unit: t("unit"), cap: x.get("cap").and_then(Json::as_f64).unwrap_or(1.0), goal: t("bind"), id: id.clone() } }).collect()),
            // a pace block (RFC 0002 Design C): r0 and the cap; a surprise block (Design B): the learned traits whose gain is drawn
            pace: d.get("pace").and_then(|p| Some((p.get("r0")?.as_f64()?, p.get("cap")?.as_f64()?))),
            gates: if d.get("surprise").is_some() { strs(d.get("learning").and_then(|l| l.get("traits"))) } else { vec![] },
            g_max: d.get("surprise").and_then(|x| x.get("g_max")).and_then(Json::as_f64).unwrap_or(3.0),
            max_open: d.get("learning").and_then(|l| l.get("credit")).and_then(|c| c.get("max_open")).and_then(Json::as_f64).map(|x| x as usize),
            face: face(&r.doc, &obj("moods").into_iter().map(|(id, _)| id).collect::<Vec<_>>()),
            n_habits: decl("habits").len(), dispositions: d.get("dispositions").is_some() }
    }
}

/// One replayed event: its number, its stance document, its inputs as the strand logs them, the learned deltas after it, the
/// panel's readings and words at it
struct Frame { n: u64, doc: Json, given: Json, learned: Json, read: Vec<panel::Reading>, words: Vec<panel::Word> }
impl Frame {
    /// A frame for event `n` of `rep`; `prev`: the frame before it (None: the first one this replay shows)
    fn of(rep: &Replay, meta: &Meta, n: u64, doc: Json, given: Json, prev: Option<&Frame>) -> Frame {
        let st = rep.live.st.to_json(&rep.live.p);
        let learned = st.get("learned").cloned().unwrap_or(Json::Null);
        let open = persona::open_decisions(&rep.live.p, &rep.live.st).map(|o| o.len());
        let read = readings(meta, &doc, &learned, st.get("dispositions"), open);
        let words = panel::words(&read, prev.map_or(&[][..], |f| &f.read[..]));
        Frame { n, doc, given, learned, read, words }
    }
}
/// A strand being watched: the replay, the layout, the latest frame, the moods' recent positions, the line that diverged (if any)
struct Watch { rep: Replay, meta: Meta, last: Option<Frame>, spark: Vec<VecDeque<f64>>, bad: Option<(usize, String)>,
    /// RFC 0015: the last consolidation line replayed (its line number and the line), drawn as "last rest"
    rest: Option<(usize, Json)> }
impl Watch {
    fn open(head: &str) -> Result<Watch, (usize, String)> {
        let rep = Replay::open(head)?; let meta = Meta::of(&rep); let n = meta.moods.len();
        Ok(Watch { rep, meta, last: None, spark: vec![VecDeque::new(); n], bad: None, rest: None })
    }
    /// Where the replay of `lines` (the header first) starts: the last checkpoint line (index), or None for the header. RFC 0009:
    /// before 0.14.0 a persona with an expectation block folds every event line into its memory, which no checkpoint carries, so
    /// its strand replays from the header (every line, slower; `open_at` refuses that shortcut); from 0.14.0 the checkpoint carries
    /// the memory, and every strand starts at the last one
    /// A segment (§5.7) starts at its sealed line (line 2) when it has no later checkpoint.
    fn start(lines: &[&str]) -> Option<usize> {
        let k = crate::live::last_checkpoint(lines, 0)?; let r = Replay::open(lines[0]);
        if k < 2 && !r.as_ref().is_ok_and(|r| r.header.get("segment").is_some()) { return None; }
        (!r.is_ok_and(|r| r.live.p.expect.is_some() && r.live.dev_full)).then_some(k)
    }
    /// The replay from a checkpoint line (`live::Replay::open_at`; `before` = the event line before it): the lines before it are
    /// not replayed, and the board starts at the checkpoint's event (its stance, the inputs `before` logs, its learned deltas)
    fn open_at(head: &str, before: &str, cp: &str, at: usize) -> Result<Watch, (usize, String)> {
        // a segment's sealed line (line 2, after the header) opens on the header's link
        let rep = if at == 2 { Replay::open_lines(&[head, cp])?.0 } else { Replay::open_at(head, before, cp, at)? }; let meta = Meta::of(&rep); let n = meta.moods.len();
        let mut w = Watch { rep, meta, last: None, spark: vec![VecDeque::new(); n], bad: None, rest: None };
        if let Some(doc) = w.rep.last_stance().cloned() {
            let given = json::parse(before).ok().and_then(|j| j.get("inputs").cloned()).unwrap_or(Json::Null);
            for (m, s) in w.meta.moods.iter().zip(w.spark.iter_mut()) { s.push_back(position(&odds(&doc, "mood", m))); }
            w.last = Some(Frame::of(&w.rep, &w.meta, w.rep.live.n, doc, given, None)); }
        Ok(w)
    }
    /// Replay one event line; after a line that differs, nothing more is replayed
    fn feed(&mut self, l: &str, eng: Engine) {
        if self.bad.is_some() { return; }
        match self.rep.step(l, eng) {
            // a control or checkpoint line: no stance to draw; RFC 0015: a consolidation line is kept for the "last rest" row
            Ok(None) => if let Some(j) = json::parse(l).ok().filter(|j| j.get("consolidation").is_some()) { self.rest = Some((self.rep.line, j)); }
            Ok(Some(doc)) => {
                let given = json::parse(l).ok().and_then(|j| j.get("inputs").cloned()).unwrap_or(Json::Null);
                for (m, s) in self.meta.moods.iter().zip(self.spark.iter_mut()) { if s.len() == SPARK { s.pop_front(); } s.push_back(position(&odds(&doc, "mood", m))); }
                self.last = Some(Frame::of(&self.rep, &self.meta, self.rep.live.n, doc, given, self.last.as_ref()));
            }
            Err(e) => self.bad = Some(e) }
    }
}
/// The face's moods (§2.18): the ids `display.glyphs.moods` names, in its order, else the first two of `moods`
fn face(doc: &Json, moods: &[String]) -> Vec<String> {
    let named: Vec<String> = doc.get("display").and_then(|d| d.get("glyphs")).and_then(|g| g.get("moods")).and_then(Json::as_obj)
        .map_or(vec![], |o| o.iter().map(|(k, _)| k.clone()).filter(|k| moods.contains(k)).collect());
    if named.is_empty() { moods.iter().take(2).cloned().collect() } else { named }
}
/// RFC 0009 for the page's settle animation: the event's exact trajectory per trait and mood, its values named by the levels
/// (a positional program names level i `l<i>`), in the persona's order: the betas, the mean marginal entropy per step and per
/// variable its odds per step, each as the engine computed it (rounded to 6 places); None without `engine.trajectory`
fn settle(m: &Meta, tj: &Json) -> Option<Json> {
    let steps = tj.get("steps").and_then(Json::as_arr).filter(|a| !a.is_empty())?;
    let num = |x: Option<&Json>| json::num(persona::r6(x.and_then(Json::as_f64).unwrap_or(0.0)));
    let kinds = m.traits.iter().map(|v| ("trait", v)).chain(m.moods.iter().map(|v| ("mood", v)));
    let vars: Vec<Json> = kinds.filter_map(|(kind, v)| {
        let at = |st: &Json| st.get("marginals").and_then(|x| x.get(&v.id)).cloned();
        let first = at(&steps[0])?;
        let names: Vec<String> = v.levels.iter().enumerate().map(|(i, l)| if first.get(l).is_some() { l.clone() } else { format!("l{i}") }).collect();
        let odds = steps.iter().map(|st| { let mg = at(st); Json::Arr(names.iter().map(|k| num(mg.as_ref().and_then(|g| g.get(k)))).collect()) }).collect();
        Some(json::obj(vec![("id", json::str(&v.id)), ("kind", json::str(kind)), ("levels", Json::Arr(v.levels.iter().map(|l| json::str(l)).collect())), ("odds", Json::Arr(odds))]))
    }).collect();
    Some(json::obj(vec![("betas", Json::Arr(steps.iter().map(|st| num(st.get("beta"))).collect())), ("h", Json::Arr(steps.iter().map(|st| num(st.get("h"))).collect())), ("vars", Json::Arr(vars))]))
}
/// A variable's odds in its levels' order (0 for a level the document does not list)
fn odds(doc: &Json, group: &str, v: &Var) -> Vec<f64> {
    let o = doc.get(group).and_then(|g| g.get(&v.id)).and_then(|x| x.get("odds"));
    v.levels.iter().map(|l| o.and_then(|o| o.get(l)).and_then(Json::as_f64).unwrap_or(0.0)).collect()
}
/// Where the odds sit along the levels, 0 (the lowest level) to 1 (the highest): the mean level index over (levels - 1)
fn position(ps: &[f64]) -> f64 {
    let (t, k) = (ps.iter().sum::<f64>(), ps.len());
    if k < 2 || t <= 0.0 { return 0.0; }
    ps.iter().enumerate().map(|(i, p)| p * i as f64).sum::<f64>() / t / (k - 1) as f64
}

/// The panel's readings at an event (docs/persona.md §5.8), in the panel's order: the traits (and `pursue`), the moods, the needs,
/// the surprise gates, pace, the dispositions, the learned traits, credit, habits, the senses, room left in the bound. A block the
/// persona does not declare reads as one `not declared` field.
fn readings(m: &Meta, doc: &Json, learned: &Json, disp: Option<&Json>, open: Option<usize>) -> Vec<panel::Reading> {
    use panel::{Field, Reading};
    let r = |key: String, label: &str, field: Field| Reading { key, label: label.to_string(), field };
    let mut out = vec![];
    let level = |group: &str, v: &Var, mood: bool| {
        let x = doc.get(group).and_then(|g| g.get(&v.id));
        Field::Level { levels: v.levels.clone(), level: x.and_then(|x| x.get("level")).and_then(Json::as_str).map(String::from),
            released: x.and_then(|x| x.get("released")) != Some(&Json::Bool(false)), odds: odds(doc, group, v), mood } };
    for v in &m.traits { out.push(r(format!("stance:{}", v.id), &v.id, level("stance", v, false))); }
    if let Some(p) = doc.get("pursue").filter(|x| x.as_obj().is_some()) {
        let goals: Vec<String> = if m.goals.is_empty() { p.get("odds").and_then(Json::as_obj).map(|o| o.iter().map(|(k, _)| k.clone()).collect()).unwrap_or_default() } else { m.goals.clone() };
        let ps = goals.iter().map(|g| p.get("odds").and_then(|o| o.get(g)).and_then(Json::as_f64).unwrap_or(0.0)).collect();
        out.push(r("stance:pursue".into(), "pursue", Field::Level { levels: goals, level: p.get("goal").and_then(Json::as_str).map(String::from), released: p.get("released") != Some(&Json::Bool(false)), odds: ps, mood: false }));
    }
    for v in &m.moods { out.push(r(format!("mood:{}", v.id), &v.id, level("mood", v, true))); }
    if m.needs.is_empty() { out.push(r("need".into(), "needs", Field::Undeclared)); }
    for x in &m.needs {
        let e = doc.get("needs").and_then(|n| n.get(&x.id)); let g = |k: &str| e.and_then(|r| r.get(k)).and_then(Json::as_f64).unwrap_or(0.0);
        let say = if x.say.is_empty() { x.id.replace('_', " ") } else { x.say.clone() };
        out.push(r(format!("need:{}", x.id), &format!("need {say}"), Field::Need { deficit: g("deficit"), cap: x.cap, term: g("term") }));
    }
    if m.gates.is_empty() { out.push(r("surprise".into(), "surprise gate", Field::Undeclared)); }
    for t in &m.gates { let g = doc.get("surprise").and_then(|x| x.get(t)).and_then(Json::as_f64).unwrap_or(1.0);
        out.push(r(format!("surprise:{t}"), &format!("surprise gate on {t}"), Field::Gate { g, g_max: m.g_max })); }
    match m.pace {
        None => out.push(r("pace".into(), "pace (average reward)", Field::Undeclared)),
        Some((_, cap)) => { let p = doc.get("pace");
            out.push(r("pace".into(), "pace (average reward)", Field::Pace { level: p.and_then(|x| x.get("level")).and_then(Json::as_str).unwrap_or("steady").to_string(),
                rbar: p.and_then(|x| x.get("rbar")).and_then(Json::as_f64).unwrap_or(0.0), cap })); }
    }
    if !m.dispositions { out.push(r("disposition".into(), "dispositions", Field::Undeclared)); }
    else { for (gene, x) in disp.and_then(Json::as_obj).unwrap_or(&[]) { let v = x.as_f64().unwrap_or(0.0);
        out.push(r(format!("disposition:{gene}"), &format!("disposition {gene}"), Field::Gene { gene: gene.clone(), x: if gene == "patience" { v.max(1e-12).ln() } else { v } })); } }
    if let Some((traits, cap)) = &m.learning {
        for t in traits {
            let levels = m.traits.iter().find(|v| &v.id == t).map(|v| v.levels.clone()).or_else(|| (t == "pursue").then(|| m.goals.clone())).unwrap_or_default();
            let ds: Vec<f64> = learned.get(t).and_then(Json::as_arr).map(|a| a.iter().filter_map(Json::as_f64).collect()).unwrap_or_default();
            out.push(r(format!("learned:{t}"), &format!("learned {t}"), Field::Learned { levels, deltas: ds, cap: *cap }));
        }
    }
    match (m.max_open, open) { (Some(max), Some(n)) => out.push(r("credit".into(), "credit", Field::Credit { open: n, max })), _ => out.push(r("credit".into(), "credit", Field::Undeclared)) }
    let hb = doc.get("habits"); let n = |k: &str| hb.and_then(|h| h.get(k)).and_then(Json::as_arr).map_or(0, <[Json]>::len);
    out.push(r("habits".into(), "habits", Field::Habits { active: n("active"), bound: n("bound"), declared: m.n_habits }));
    for x in &m.senses { let z = doc.get("senses").and_then(|s| s.get(&x.id)).and_then(|s| s.get("z")).and_then(Json::as_f64);
        out.push(r(format!("sense:{}", x.id), &format!("sense {}", x.id), Field::Sense { z })); }
    // RFC 0014: with the identity self keys the stance carries `integrity`: its dial replaces the room placeholder
    match doc.get("integrity").filter(|x| x.as_obj().is_some()) {
        Some(ig) => { let g = |k: &str| ig.get(k).and_then(Json::as_f64).unwrap_or(0.0);
            out.push(r("integrity".into(), "integrity (drift from the declared self)", Field::Integrity { used: g("used"), drift: g("drift"),
                crossed: ig.get("crossed").and_then(Json::as_arr).map_or(0, <[Json]>::len), traits: ig.get("traits").and_then(Json::as_obj).map_or(0, <[(String, Json)]>::len) })); }
        None => out.push(r("room".into(), "room left in the bound", Field::Room)) }
    out
}

/// How a frame is drawn: the theme (None: no colour), plain ASCII, the columns a line may take
struct Style { th: Option<Theme>, ascii: bool, cols: usize }
impl Style {
    fn paint(&self, c: Col, s: &str) -> String { self.th.map_or_else(|| s.to_string(), |t| t.paint(c, s)) }
    fn bold(&self, c: Col, s: &str) -> String { self.th.map_or_else(|| s.to_string(), |t| t.bold(c, s)) }
    fn g<'a>(&self, uni: &'a str, ascii: &'a str) -> &'a str { if self.ascii { ascii } else { uni } }
}
/// Odds -> cells per level summing to `w` (largest remainder; ties to the earlier level)
fn cells(ps: &[f64], w: usize) -> Vec<usize> {
    let t = ps.iter().map(|p| p.max(0.0)).sum::<f64>();
    if t <= 0.0 { return vec![0; ps.len()]; }
    let raw: Vec<f64> = ps.iter().map(|p| p.max(0.0) / t * w as f64).collect();
    let mut c: Vec<usize> = raw.iter().map(|x| x.floor() as usize).collect();
    let mut order: Vec<usize> = (0..ps.len()).collect();
    order.sort_by(|&a, &b| (raw[b] - raw[b].floor()).total_cmp(&(raw[a] - raw[a].floor())).then(a.cmp(&b)));
    let left = w.saturating_sub(c.iter().sum());
    for &i in order.iter().take(left) { c[i] += 1; }
    c
}
/// A stacked bar: one segment per level, the level taken (`pick`) in `hue`, the others in alternating shades
fn stack(s: &Style, ps: &[f64], pick: Option<usize>, hue: Col) -> String {
    cells(ps, BAR).iter().enumerate().filter(|(_, &n)| n > 0).map(|(i, &n)| {
        if Some(i) == pick { s.paint(hue, &s.g("█", "#").repeat(n)) }
        else { let glyph = if s.th.is_some() { s.g("█", "#") } else if i % 2 == 0 { s.g("▒", "=") } else { s.g("░", "-") }; s.paint(SHADE[i % 2], &glyph.repeat(n)) } }).collect()
}
/// `level 0.37` per level, the level taken in brackets
fn labels(s: &Style, levels: &[String], ps: &[f64], pick: Option<usize>, hue: Col) -> String {
    levels.iter().zip(ps).enumerate().map(|(i, (l, p))| if Some(i) == pick { s.bold(hue, &format!("[{l} {p:.2}]")) } else { s.paint(GREY, &format!(" {l} {p:.2} ")) }).collect::<Vec<_>>().join(" ")
}
/// A number bar: `v / max` of `NBAR` cells
fn nbar(s: &Style, v: f64, max: f64, hue: Col) -> String {
    let k = ((v / max.max(1e-12)).clamp(0.0, 1.0) * NBAR as f64).round() as usize;
    format!("{}{}{}{}", s.paint(DIM, s.g("▕", "[")), s.paint(hue, &s.g("█", "#").repeat(k)), s.paint(DIM, &s.g("░", ".").repeat(NBAR - k)), s.paint(DIM, s.g("▏", "]")))
}
/// A sense's range as `NBAR` cells with its normal as a tick and this event's reading as a dot (both on the normal's scale)
fn tick(s: &Style, lo: f64, hi: f64, mean: f64, at: Option<f64>) -> String {
    let cell = |v: f64| (((v - lo) / (hi - lo).max(1e-12)).clamp(0.0, 1.0) * (NBAR - 1) as f64).round() as usize;
    let (t, r) = (cell(mean), at.map(cell));
    let cells: String = (0..NBAR).map(|i| if Some(i) == r { s.bold(CYAN, s.g("●", "o")) } else if i == t { s.bold(AMBER, s.g("┃", "|")) } else { s.paint(DIM, s.g("─", "-")) }).collect();
    format!("{}{cells}{}", s.paint(DIM, s.g("▕", "[")), s.paint(DIM, s.g("▏", "]")))
}
/// A centred bar for a signed value within ±cap: `h` cells each side of the centre mark
fn centred(s: &Style, v: f64, cap: f64, h: usize) -> String {
    let k = ((v.abs() / cap.max(1e-12)).clamp(0.0, 1.0) * h as f64).round() as usize;
    let (fill, mid) = (s.g("━", if v < 0.0 { "-" } else { "+" }), s.g("┃", "|"));
    let hue = if v < 0.0 { MAGENTA } else { GREEN };
    if v < 0.0 { format!("{}{}{}{}", " ".repeat(h - k), s.paint(hue, &fill.repeat(k)), s.paint(DIM, mid), " ".repeat(h)) }
    else { format!("{}{}{}{}", " ".repeat(h), s.paint(DIM, mid), s.paint(hue, &fill.repeat(k)), " ".repeat(h - k)) }
}
/// The moods' recent positions as a sparkline
fn spark(s: &Style, xs: &VecDeque<f64>) -> String {
    let ramp: Vec<char> = s.g("▁▂▃▄▅▆▇█", "_.:-=+*#").chars().collect();
    xs.iter().map(|x| ramp[(x.clamp(0.0, 1.0) * (ramp.len() - 1) as f64).round() as usize]).collect()
}
/// Hours as a short clock: seconds, minutes or hours
fn hours(h: f64) -> String { if h < 1.0 / 60.0 { format!("+{:.0} s", h * 3600.0) } else if h < 1.0 { format!("+{:.1} min", h * 60.0) } else { format!("+{h:.2} h") } }
/// A count or a cap: whole numbers without decimals
fn whole(x: f64) -> String { if x.fract() == 0.0 && x.abs() < 1e15 { format!("{}", x as i64) } else { format!("{x}") } }
/// A digest's leading 8 hex digits
fn short(d: &str) -> &str { let h = d.strip_prefix("sha256:").unwrap_or(d); &h[..h.len().min(8)] }

/// One frame: the lines to draw (without line endings). `note`: a warning to show (a truncated or rotated strand, an
/// incomplete last line).
fn frame(w: Option<&Watch>, bad_head: Option<&(usize, String)>, path: &str, note: Option<&str>, s: &Style) -> Vec<String> {
    // Strip data's C0, DEL and C1 controls per plain line; the caller adds the frame's line endings.
    let finish = |mut lines: Vec<String>| { if s.ascii { for line in &mut lines { line.retain(|c| !c.is_control()); } } lines };
    let dot = s.paint(DIM, s.g("·", "|"));
    let lab = |t: &str| s.bold(MAGENTA, &format!("{t:<8}"));
    let mut out = vec![];
    let bad = bad_head.or_else(|| w.and_then(|w| w.bad.as_ref()));
    let badge = match (bad, w.and_then(|w| w.rep.from)) { (Some((n, why)), _) => s.bold(RED, &format!("line {n} diverges: {why}")),
        (None, Some(c)) => s.bold(GREEN, s.g(&format!("replay verified from checkpoint {c} ✓"), &format!("replay verified from checkpoint {c} [ok]"))),
        (None, None) => s.bold(GREEN, s.g("replay verified ✓", "replay verified [ok]")) };
    // a paused or retired individual (control lines, docs/persona.md §5.7) says so next to the badge
    let badge = match w.map(|w| w.rep.live.status).filter(|st| *st != crate::live::Status::Active) { Some(st) => format!("{badge} {dot} {}", s.bold(if st == crate::live::Status::Retired { RED } else { YELLOW }, st.name())), None => badge };
    let Some(w) = w else {
        out.push(format!("{} {dot} {badge}", s.bold(CYAN, "probbit monitor")));
        if let Some(t) = note { out.push(s.paint(YELLOW, t)); }
        out.push(footer(path, "", s));
        return finish(out);
    };
    let m = &w.meta;
    let f = w.last.as_ref();
    // §2.18: the engine's glyph next to the persona's name (not in plain ASCII)
    let glyph = f.and_then(|f| f.doc.get("glyph")).and_then(Json::as_str).filter(|_| !s.ascii).map_or(String::new(), |g| format!(" {g}"));
    let mut head = format!("{} {}{glyph} {dot} seed {} {dot} individual {} {dot} event {}", s.bold(CYAN, "probbit monitor"), s.bold(CYAN, &format!("{} {}", m.name, m.version)), m.seed, short(&m.individual), f.map_or(0, |f| f.n));
    if let Some(h) = f.and_then(|f| f.given.get("elapsed_hours")).and_then(Json::as_f64) { head += &format!(" {dot} {} since the event before", hours(h)); }
    out.push(format!("{head} {dot} {badge}"));
    // §2.18: why the engine gave this glyph, under the header (the glyph itself only when the terminal takes UTF-8)
    if let Some(y) = f.and_then(|f| f.doc.get("glyph_why")).and_then(Json::as_str).filter(|y| !y.is_empty()) {
        let g = f.and_then(|f| f.doc.get("glyph")).and_then(Json::as_str).filter(|_| !s.ascii).map_or(String::new(), |g| format!("{g} "));
        let y: String = if s.ascii { y.chars().filter(char::is_ascii).collect() } else { y.to_string() };
        out.push(format!("{}{g}{}", lab("GLYPH"), s.paint(GREY, &y)));
    }
    out.push(s.paint(GREY, s.g("LEGEND  width = odds; colour/[level] = chosen; shades = other levels",
        "LEGEND  width = odds; #/[level] = chosen; =/- = other levels")));
    out.push(s.paint(GREY, s.g("        Mood trail ▁▂▃▄▅▆▇█ = mean level low → high (last 50 events)",
        "        Mood trail _.:-=+*# = mean level low -> high (last 50 events)")));
    out.push(s.paint(GREY, "        Habits: active = in force; [name] = the no-habits twin broke this rule"));
    if let Some(t) = note { out.push(s.paint(YELLOW, t)); }
    let Some(f) = f else {
        out.push(s.paint(GREY, "no events yet: the strand holds its header and no event lines"));
        out.push(footer(path, &m.engine, s));
        return finish(out);
    };
    let doc = &f.doc;
    let idw = m.traits.iter().chain(&m.moods).map(|v| v.id.chars().count()).chain(m.learning.iter().flat_map(|l| l.0.iter().map(|t| t.chars().count()))).max().unwrap_or(4).clamp(4, 14);
    let status = doc.get("status").and_then(Json::as_str).unwrap_or("");
    // 1. the stance: one row per trait
    let mut top = true;
    for v in &m.traits {
        let Some(t) = doc.get("stance").and_then(|x| x.get(&v.id)) else { continue };
        let ps = odds(doc, "stance", v);
        let level = t.get("level").and_then(Json::as_str).unwrap_or("");
        let released = t.get("released") != Some(&Json::Bool(false));
        let pick = v.levels.iter().position(|l| l == level).filter(|_| released);
        let say = pick.and_then(|i| v.say.get(i)).filter(|x| !x.is_empty()).cloned().unwrap_or_default();
        let note = if released { say } else { s.paint(GREY, &format!("({level}: not released)")) };
        out.push(format!("{}{:<idw$}  {}  {}  {note}", lab(if top { "STANCE" } else { "" }), v.id, stack(s, &ps, pick, CYAN), labels(s, &v.levels, &ps, pick, CYAN)).trim_end().to_string());
        top = false;
    }
    if status != "ok" && !status.is_empty() {
        let esc = doc.get("escalate").and_then(Json::as_str).map_or(String::new(), |e| format!(": {e}"));
        out.push(format!("{}{}", lab(""), s.bold(YELLOW, &format!("status {status}{esc}"))));
    }
    // 2. the moods, with their recent positions
    for (k, v) in m.moods.iter().enumerate() {
        let ps = odds(doc, "mood", v);
        let level = doc.get("mood").and_then(|x| x.get(&v.id)).and_then(|x| x.get("level")).and_then(Json::as_str).unwrap_or("");
        let pick = v.levels.iter().position(|l| l == level);
        out.push(format!("{}{:<idw$}  {}  {}  {}", lab(if k == 0 { "MOODS" } else { "" }), v.id, stack(s, &ps, pick, AMBER), labels(s, &v.levels, &ps, pick, AMBER), s.paint(AMBER, &spark(s, &w.spark[k]))));
    }
    // 3. the inputs: the event's inputs as the turn used them, then the history features
    let ins = doc.get("inputs");
    let val = |id: &str| ins.and_then(|i| i.get(id));
    let mut chips = vec![]; let mut nums = vec![];
    for x in &m.inputs {
        match (x.kind.as_str(), val(&x.id)) {
            ("flag", v) => { let on = v == Some(&Json::Bool(true)); chips.push(if on { s.bold(CYAN, &format!("{} {}", s.g("●", "(x)"), x.id)) } else { s.paint(GREY, &format!("{} {}", s.g("○", "( )"), x.id)) }); }
            ("number", v) => { let n = v.and_then(Json::as_f64).unwrap_or(0.0); nums.push(format!("{} {} {n:.2}", x.id, nbar(s, n, x.max, CYAN))); }
            (_, v) => { let l = v.and_then(Json::as_str).unwrap_or(""); chips.push(format!("{} {}", s.paint(GREY, &x.id), s.bold(CYAN, l))); } }
    }
    if let Some(g) = val("goals").filter(|g| !g.is_null()) { chips.push(format!("{} {}", s.paint(GREY, "goals"), persona::canon(g))); }
    out.push(format!("{}{}", lab("INPUTS"), chips.join("  ")));
    if let Some(h) = f.given.get("elapsed_hours").and_then(Json::as_f64) { nums.push(format!("clock {}", hours(h))); }
    if !nums.is_empty() { out.push(format!("{}{}", lab(""), nums.join("  "))); }
    let hist: Vec<String> = m.history.iter().map(|(id, cap)| { let n = val(id).and_then(Json::as_f64).unwrap_or(0.0); format!("{id} {}/{}", whole(n), whole(*cap)) }).collect();
    if !hist.is_empty() { out.push(format!("{}{} {}", lab(""), s.paint(GREY, "history"), hist.join("  "))); }
    if let Some(ig) = doc.get("ignored").and_then(Json::as_arr).filter(|a| !a.is_empty()) { out.push(format!("{}{} {}", lab(""), s.paint(GREY, "ignored"), strs(Some(&Json::Arr(ig.to_vec()))).join(" "))); }
    // 3b. the senses (§2.11): per sense the normal as a tick over the range, this event's reading, z and n (the normal it was
    // judged against); a sense not read keeps its normal; an alert when a normal moved over drift_cap / 2 within 50 events
    let now = persona::normals(&w.rep.live.p, &w.rep.live.st);
    let sw = m.senses.iter().map(|x| x.id.chars().count()).max().unwrap_or(4).clamp(4, 14);
    for (k, x) in m.senses.iter().enumerate() {
        let read = doc.get("senses").and_then(|r| r.get(&x.id));
        let g = |key: &str| read.and_then(|r| r.get(key)).and_then(Json::as_f64);
        let (mean, n) = match read { Some(_) => (g("mean").unwrap_or(0.0), g("n").unwrap_or(0.0)), None => now.get(k).map_or((0.0, 0.0), |v| (v.1, v.2 as f64)) };
        let raw = read.and_then(|r| r.get("raw"));
        let at = raw.and_then(|v| match v { Json::Bool(b) => Some(if *b { 1.0 } else { 0.0 }), _ => v.as_f64() }).map(|v| x.scaled(v));
        let norm = if x.flag { format!("normal rate {mean:.2}") } else if x.log1p { format!("normal {:.2} {}", mean.exp_m1(), x.unit) } else { format!("normal {mean:.2} {}", x.unit) };
        let what = match (raw, g("z")) {
            (Some(v), Some(z)) => s.bold(CYAN, &format!("read {}  z {z:+.1}", persona::canon(v))),
            _ => s.paint(GREY, "not read"),
        };
        out.push(format!("{}{:<sw$}  {}  {what}  {}", lab(if k == 0 { "SENSES" } else { "" }), x.id, tick(s, x.lo, x.hi, mean, at), s.paint(GREY, &format!("{norm} over {} readings", whole(n)))));
        if let Some(mv) = w.rep.normals().moved.get(k).copied().filter(|mv| *mv > x.drift_cap / 2.0) {
            let say = if x.say.is_empty() { x.id.replace('_', " ") } else { x.say.clone() };
            out.push(format!("{}{}", lab(""), s.bold(RED, &format!("{} {say}: the normal moved {mv} within 50 events (over drift_cap / 2 = {})", s.g("⚠", "!"), persona::r6(x.drift_cap / 2.0))))); }
    }
    // 3c. the needs (RFC 0002): per need its deficit as a bar over [0, cap], its pursue term and its goal's omission window; an
    // alert when a deficit sits at its cap
    let nw = m.needs.iter().map(|x| x.id.chars().count()).max().unwrap_or(4).clamp(4, 14);
    for (k, x) in m.needs.iter().enumerate() {
        let e = doc.get("needs").and_then(|n| n.get(&x.id)); let g = |key: &str| e.and_then(|r| r.get(key)).and_then(Json::as_f64).unwrap_or(0.0);
        let (dx, term) = (g("deficit"), g("term"));
        let win = match doc.get("drives").and_then(|d| d.get("window")).and_then(|w| w.get(&x.goal)) { Some(Json::Num(h)) => format!("window {} {h:.2} h left", x.goal), _ => format!("window {} closed", x.goal) };
        out.push(format!("{}{:<nw$}  {}  {}  {}", lab(if k == 0 { "NEEDS" } else { "" }), x.id, nbar(s, dx, x.cap, AMBER), s.bold(CYAN, &format!("deficit {dx:.2}/{} {}  term {term:+.2}", persona::r6(x.cap), x.unit)), s.paint(GREY, &win)));
        if dx >= x.cap - 5e-7 { let say = if x.say.is_empty() { x.id.replace('_', " ") } else { x.say.clone() };
            out.push(format!("{}{}", lab(""), s.bold(RED, &format!("{} {say}: the deficit sits at its cap ({})", s.g("⚠", "!"), persona::r6(x.cap))))); }
    }
    // 3c'. viability (RFC 0008 E): per need with a bound its margin as a bar over [0, 1] (1 at the setpoint, 0 at the bound) and its
    // run of events beyond the bound; the worst need highlighted; the status when lost
    if let Some(vj) = doc.get("viability") { let worst = vj.get("worst").and_then(Json::as_str).unwrap_or("");
        let mm = vj.get("margin_min").and_then(Json::as_f64).unwrap_or(0.0); let status = vj.get("status").and_then(Json::as_str).unwrap_or("viable");
        // the label is wider than the 8-column gutter: it heads its own line
        out.push(format!("{}  {}", s.bold(MAGENTA, "VIABILITY"), s.bold(if status == "lost" { RED } else { CYAN }, &format!("worst {worst} {dot} margin_min {mm:.2} {dot} {status}"))));
        for x in &m.needs { let Some(mg) = doc.get("needs").and_then(|n| n.get(&x.id)).and_then(|e| e.get("margin")).and_then(Json::as_f64) else { continue };
            let by = doc.get("needs").and_then(|n| n.get(&x.id)).and_then(|e| e.get("beyond")).and_then(Json::as_f64).unwrap_or(0.0);
            let txt = format!("margin {mg:.2}  beyond {}{}", whole(by), if x.id == worst { "  worst" } else { "" });
            out.push(format!("{}{:<nw$}  {}  {}", lab(""), x.id, nbar(s, mg, 1.0, if x.id == worst { RED } else { GREEN }),
                if x.id == worst { s.bold(RED, &txt) } else { s.paint(CYAN, &txt) })); }
        if vj.get("status").and_then(Json::as_str) == Some("lost") { out.push(format!("{}{}", lab(""), s.bold(RED, &format!("{} viability lost: a need stayed beyond its bound for its declared run of events", s.g("⚠", "!"))))); } }
    // 3d. pace (RFC 0002 Design C): the average as a dot over [0, cap] with an r0 tick, and the level; 3e. the surprise gate
    // (Design B): per learned trait its gain g (1 when the stance lists none: the gate is closed)
    if let Some((r0, cap)) = m.pace { let p = doc.get("pace"); let rb = p.and_then(|x| x.get("rbar")).and_then(Json::as_f64).unwrap_or(0.0);
        let lv = p.and_then(|x| x.get("level")).and_then(Json::as_str).unwrap_or("steady");
        out.push(format!("{}{}  {}", lab("PACE"), tick(s, 0.0, cap, r0, Some(rb)), s.bold(CYAN, &format!("rbar {rb:.2}/{} {dot} r0 {} {dot} {lv}", persona::r6(cap), persona::r6(r0))))); }
    for (k, t) in m.gates.iter().enumerate() { let g = doc.get("surprise").and_then(|x| x.get(t)).and_then(Json::as_f64).unwrap_or(1.0);
        out.push(format!("{}{t:<idw$}  {}  {}", lab(if k == 0 { "GAIN" } else { "" }), nbar(s, g - 1.0, 2.0, AMBER),
            s.bold(if g > 1.0 { AMBER } else { GREEN }, &format!("g {g:.2}{}", if g > 1.0 { "  learning fast" } else { "" })))); }
    // 3f. RFC 0009: the deviation from the engine's own expectation (KL in nats, effects {}) and its z against the learned
    // normal; then the event's exact trajectory as the mean marginal entropy per beta step (derived, never logged)
    if let Some(dv) = doc.get("deviation").filter(|d| !d.is_null()) { let g = |k: &str| dv.get(k).and_then(Json::as_f64).unwrap_or(0.0);
        let cold = dv.get("cold") == Some(&Json::Bool(true)); let reg = dv.get("registers") == Some(&Json::Bool(true));
        let z = dv.get("z").and_then(Json::as_f64).map_or("z -".to_string(), |z| format!("z {z:+.2}"));
        let txt = format!("pred {:.4} {dot} self {:.4} {dot} input {:.2} {dot} {}{}", g("pred"), g("self"), g("input"), if cold { "cold".to_string() } else { z }, if reg { "  registers deviation" } else { "" });
        out.push(format!("{}  {}", s.bold(MAGENTA, "DEVIATION"), if reg { s.bold(AMBER, &txt) } else { s.paint(CYAN, &txt) })); }
    // RFC 0015: the last rest pass replayed (only once the strand has one): its clock, the contexts merged, the weights moved
    if let Some((line, j)) = w.rest.as_ref() { let c = j.get("consolidation"); let g = |k: &str| c.and_then(|c| c.get(k)).and_then(Json::as_f64).unwrap_or(0.0);
        let merged = c.and_then(|c| c.get("merged")).and_then(|m| m.get("count")).and_then(Json::as_f64).unwrap_or(0.0);
        let txt = format!("last rest {} {dot} event {} {dot} line {line} {dot} {} merged {dot} moved {:.6} {dot} decay {:.6} {dot} {} events",
            j.get("at").and_then(Json::as_str).unwrap_or("-"), j.get("n").and_then(Json::as_f64).unwrap_or(0.0) as u64, merged as u64, g("moved"), g("decay"), g("events") as u64);
        out.push(format!("{}  {}", s.bold(MAGENTA, "REST"), s.paint(CYAN, &txt))); }
    if let Some(tj) = w.rep.live.traj.as_ref() { let st = tj.get("steps").and_then(Json::as_arr).map_or(vec![], |a| a.to_vec());
        let hs: Vec<String> = st.iter().map(|x| format!("{:.2}", x.get("h").and_then(Json::as_f64).unwrap_or(0.0))).collect();
        let b0 = st.first().and_then(|x| x.get("beta")).and_then(Json::as_f64).unwrap_or(1.0);
        out.push(format!("{}  {}", s.bold(MAGENTA, "TRAJECTORY"), s.paint(CYAN, &format!("exact {dot} beta {b0:.2} -> 1 {dot} mean H {}", hs.join(" ")))));
    }
    // 4. the habits: in force, the ones that bound highlighted, the violations counter (0 by construction)
    let hb = doc.get("habits");
    let list = |k: &str| strs(hb.and_then(|h| h.get(k)));
    let bound = list("bound");
    let active: Vec<String> = list("active").iter().map(|h| if bound.contains(h) { s.bold(CYAN, &format!("[{h}]")) } else { s.paint(GREY, h) }).collect();
    let viol = hb.and_then(|h| h.get("violations")).and_then(Json::as_f64).unwrap_or(0.0);
    let mut hl = format!("{}{} {dot} violations {}", lab("HABITS"), if active.is_empty() { s.paint(GREY, "none in force") } else { active.join("  ") }, s.bold(if viol == 0.0 { GREEN } else { RED }, &whole(viol)));
    for k in ["yielded", "conflict"] { let l = list(k); if !l.is_empty() { hl += &format!(" {dot} {k} {}", l.join(" ")); } }
    out.push(hl);
    // 5. the learned deltas, centred within ±total_cap
    if let Some((traits, cap)) = &m.learning {
        for (k, t) in traits.iter().enumerate() {
            let levels = m.traits.iter().find(|v| &v.id == t).map(|v| v.levels.clone()).or_else(|| (t == "pursue").then(|| m.goals.clone())).unwrap_or_default();
            let ds: Vec<f64> = f.learned.get(t).and_then(Json::as_arr).map(|a| a.iter().filter_map(Json::as_f64).collect()).unwrap_or_default();
            let cellsx: Vec<String> = levels.iter().zip(&ds).map(|(l, d)| format!("{l} {} {d:+.2}", centred(s, *d, *cap, 5))).collect();
            out.push(format!("{}{t:<idw$}  {}{}", lab(if k == 0 { "LEARNED" } else { "" }), cellsx.join("  "), if k == 0 { s.paint(GREY, &format!("  (cap {}{})", s.g("±", "+/-"), whole(*cap))) } else { String::new() }));
        }
    }
    // 5b. action-addressed feedback (§2.8), when the document carries it: the open decisions with the eligibility a payment at
    // the next event would get, then this event's payment and the decisions it closed
    if let Some(c) = doc.get("credit") {
        let open = persona::open_decisions(&w.rep.live.p, &w.rep.live.st).unwrap_or_default();
        let ol = if open.is_empty() { s.paint(GREY, "none") } else { open.iter().map(|(t, e)| format!("{t} {}", s.paint(GREY, &format!("e {e:.2}")))).collect::<Vec<_>>().join("  ") };
        let mut row = format!("{}{} {ol}", lab("CREDIT"), s.paint(GREY, "open"));
        if let Some(pd) = c.get("paid").filter(|x| !x.is_null()) { let g = |k: &str| pd.get(k).and_then(Json::as_f64).unwrap_or(0.0);
            row += &format!(" {dot} {}", s.bold(CYAN, &format!("paid {} e {:.2} sign {}{}", whole(g("turn")), g("e"), whole(g("sign")), if pd.get("absorbed") == Some(&Json::Bool(true)) { " absorbed" } else { "" }))); }
        let closed: Vec<String> = c.get("closed").and_then(Json::as_arr).map_or(vec![], |a| a.iter().map(|x| format!("{} {}", x.get("turn").and_then(Json::as_f64).map_or(String::new(), whole), x.get("as").and_then(Json::as_str).unwrap_or(""))).collect());
        if !closed.is_empty() { row += &format!(" {dot} {} {}", s.paint(GREY, "closed"), closed.join(", ")); }
        out.push(row);
    }
    // 6. the drives, when the document carries them (feature-detected: documents without them draw no row)
    out.extend(drives(doc, m, s, idw, &lab));
    // 7. the line, and why
    if let Some(l) = doc.get("line").and_then(Json::as_str) { out.push(format!("{}{}", lab("LINE"), s.bold(CYAN, l))); }
    if let Some(y) = doc.get("why").and_then(Json::as_str).filter(|y| !y.is_empty()) { out.push(format!("{}{}", lab(""), s.paint(GREY, &format!("why: {}", display_why(y))))); }
    // 8. the panel: every field's words (panel.rs), the number its dial shows beside them
    let pw = f.words.iter().map(|w| w.text.chars().count()).max().unwrap_or(0);
    for (k, w) in f.words.iter().enumerate() {
        let num = if w.value.is_empty() { String::new() } else { format!("  {}", s.paint(GREY, &format!("({})", w.value))) };
        out.push(format!("{}{}{}", lab(if k == 0 { "PANEL" } else { "" }), s.bold(if w.dial.is_some() { CYAN } else { GREY }, &format!("{:<pw$}", w.text)), num).trim_end().to_string());
    }
    // 9. the footer
    out.push(footer(path, &m.engine, s));
    finish(out)
}
/// Clarify the legacy explanation without changing a replayed stance document or its digest.
fn display_why(why: &str) -> &str {
    if why == "baseline (no live evidence)" { "baseline (no salient input or binding habit this event; prior state still applies)" } else { why }
}
/// The footer: the strand, the engine that wrote it and the command that replays it; the demo's week (path "") is in memory, so
/// the command that writes the same week instead
fn footer(path: &str, engine: &str, s: &Style) -> String {
    let dot = s.g("·", "|");
    let by = if engine.is_empty() { String::new() } else { format!(" {dot} written by {engine}") };
    s.paint(GREY, &if path.is_empty() { format!("synthetic demo in memory{by} {dot} tutor week: probbit monitor --demo week {dot} goals: probbit monitor --demo drives") }
        else { format!("strand {path}{by} {dot} replay: probbit live verify {path}") })
}
/// Per goal: a drive's values, from an object keyed by goal or a list in the goals' order
fn per_goal(v: Option<&Json>, goals: &[String]) -> Vec<Option<f64>> {
    match v { Some(Json::Obj(o)) => goals.iter().map(|g| o.iter().find(|(k, _)| k == g).and_then(|(_, x)| x.as_f64())).collect(),
        Some(Json::Arr(a)) => (0..goals.len()).map(|i| a.get(i).and_then(Json::as_f64)).collect(), _ => vec![None; goals.len()] }
}
/// The drives rows (0.8.0 documents with a drives block): `pursue` as a stance row, then per goal wanting, afterglow and the
/// last prediction error
fn drives(doc: &Json, m: &Meta, s: &Style, idw: usize, lab: &dyn Fn(&str) -> String) -> Vec<String> {
    let (pz, dv) = (doc.get("pursue").filter(|x| x.as_obj().is_some()), doc.get("drives").filter(|x| x.as_obj().is_some()));
    if pz.is_none() && dv.is_none() { return vec![]; }
    let mut out = vec![];
    // the goals: pursue's odds (canonical order), else the goals a drive object names
    let mut goals: Vec<String> = pz.and_then(|p| p.get("odds")).and_then(Json::as_obj).map(|o| o.iter().map(|(k, _)| k.clone()).collect()).unwrap_or_default();
    if goals.is_empty() { goals = dv.and_then(Json::as_obj).and_then(|o| o.iter().filter(|(k, _)| k != "curiosity").find_map(|(_, v)| v.as_obj())).map(|o| o.iter().map(|(k, _)| k.clone()).collect()).unwrap_or_default(); }
    if let Some(p) = pz {
        let v = Var { id: "pursue".into(), levels: goals.clone(), say: vec![] };
        let ps: Vec<f64> = goals.iter().map(|g| p.get("odds").and_then(|o| o.get(g)).and_then(Json::as_f64).unwrap_or(0.0)).collect();
        let goal = p.get("goal").and_then(Json::as_str).unwrap_or("goal");
        let released = p.get("released") != Some(&Json::Bool(false));
        let pick = goals.iter().position(|x| x == goal).filter(|_| released);
        let say = if released { p.get("say").and_then(Json::as_str).unwrap_or("").to_string() } else { s.paint(GREY, &format!("({goal}: not released)")) };
        out.push(format!("{}{:<idw$}  {}  {}  {say}", lab("DRIVES"), v.id, stack(s, &ps, pick, ORANGE), labels(s, &v.levels, &ps, pick, ORANGE)).trim_end().to_string());
    }
    if let Some(d) = dv {
        let (want, glow, err) = (per_goal(d.get("want"), &goals), per_goal(d.get("glow"), &goals), per_goal(d.get("surprise"), &goals));
        for (i, g) in goals.iter().enumerate() {
            let mut row = format!("{}{g:<idw$} ", lab(if pz.is_none() && i == 0 { "DRIVES" } else { "" }));
            if let Some(x) = want[i] { row += &format!(" wanting {} {x:.2}", nbar(s, x, m.caps.0, ORANGE)); }
            if let Some(x) = glow[i] { row += &format!("  afterglow {} {x:.2}", nbar(s, x, m.caps.1, GREEN)); }
            if let Some(x) = err[i].filter(|x| *x != 0.0) { row += &format!("  {}", s.bold(if x < 0.0 { RED } else { GREEN }, &format!("{x:+.2} {} expectation", if x < 0.0 { "below" } else { "above" }))); }
            out.push(row);
        }
        // RFC 0016: the curiosity drive, its level and its two inputs (and the stir's class on a self-started event)
        if let Some(c) = d.get("curiosity").filter(|c| c.as_obj().is_some()) {
            let f = |k: &str| c.get(k).and_then(Json::as_f64).unwrap_or(0.0);
            let mut row = format!("{}{:<idw$}  level {} {:.2}  surprise_recent {:.2}  failed_contexts {}", lab(if pz.is_none() && goals.is_empty() { "DRIVES" } else { "" }), "curiosity",
                nbar(s, f("level"), 2.0, ORANGE), f("level"), f("surprise_recent"), f("failed_contexts") as u64);
            if let Some(cl) = c.get("stir").and_then(Json::as_str) { row += &format!("  {}", s.bold(GREEN, &format!("stir: {cl}"))); }
            out.push(row);
        }
    }
    out
}

/// The page's layout (`event: meta`): the strand ("" = the demo week), the engine that wrote it, the persona and its variables
/// in the persona's order (none while there is no header that replays)
fn meta_json(w: Option<&Watch>, path: &str) -> Json {
    let s = |x: &str| Json::Str(x.to_string());
    let strs = |v: &[String]| Json::Arr(v.iter().map(|x| s(x)).collect());
    let o = |kv: Vec<(&str, Json)>| json::obj(kv);
    let mut kv = vec![("path", s(path)), ("demo", Json::Bool(path.is_empty()))];
    if let Some(m) = w.map(|w| &w.meta) {
        let vars = |vs: &[Var]| Json::Arr(vs.iter().map(|v| o(vec![("id", s(&v.id)), ("levels", strs(&v.levels)), ("say", strs(&v.say))])).collect());
        kv.extend([("engine", s(&m.engine)), ("persona", o(vec![("name", s(&m.name)), ("version", s(&m.version)), ("seed", Json::Num(m.seed as f64)), ("individual", s(&m.individual))])),
            ("traits", vars(&m.traits)), ("moods", vars(&m.moods)),
            ("inputs", Json::Arr(m.inputs.iter().map(|x| o(vec![("id", s(&x.id)), ("kind", s(&x.kind)), ("max", Json::Num(x.max))])).collect())),
            ("history", Json::Arr(m.history.iter().map(|(id, cap)| o(vec![("id", s(id)), ("cap", Json::Num(*cap))])).collect())),
            ("learning", m.learning.as_ref().map_or(Json::Null, |(t, cap)| o(vec![("traits", strs(t)), ("total_cap", Json::Num(*cap))]))),
            ("caps", o(vec![("wanting", Json::Num(m.caps.0)), ("afterglow", Json::Num(m.caps.1)), ("prediction_error", Json::Num(m.caps.2))])), ("goals", strs(&m.goals)),
            ("face", strs(&m.face))]);
        // the senses (§2.11), only when the persona declares them: their range on the normal's scale and the alert's cap
        if !m.senses.is_empty() {
            kv.push(("senses", Json::Arr(m.senses.iter().map(|x| o(vec![("id", s(&x.id)), ("say", s(&x.say)), ("unit", s(&x.unit)), ("kind", s(if x.flag { "flag" } else { "number" })),
                ("scale", s(if x.log1p { "log1p" } else { "linear" })), ("lo", json::num(persona::r6(x.lo))), ("hi", json::num(persona::r6(x.hi))), ("drift_cap", Json::Num(x.drift_cap))])).collect())));
        }
        // the needs (RFC 0002), only when the persona declares them: their phrase, unit, cap and bound goal
        if !m.needs.is_empty() {
            kv.push(("needs", Json::Arr(m.needs.iter().map(|x| o(vec![("id", s(&x.id)), ("say", s(&x.say)), ("unit", s(&x.unit)), ("cap", Json::Num(x.cap)), ("goal", s(&x.goal))])).collect())));
        }
        // pace and the surprise gate (RFC 0002 Designs B, C), only when the persona declares them
        if let Some((r0, cap)) = m.pace { kv.push(("pace", o(vec![("r0", Json::Num(r0)), ("cap", Json::Num(cap))]))); }
        if !m.gates.is_empty() { kv.push(("gates", Json::Arr(m.gates.iter().map(|t| s(t)).collect()))); }
    }
    o(kv)
}
/// One frame for the page (`event: frame`): the latest event (its number, stance document, inputs as the strand logs them,
/// learned deltas after it), the moods' recent positions, the line that diverges and the note, if any
fn frame_json(w: Option<&Watch>, bad_head: Option<&(usize, String)>, note: Option<&str>) -> Json {
    let f = w.and_then(|w| w.last.as_ref());
    let bad = bad_head.or_else(|| w.and_then(|w| w.bad.as_ref()));
    let spark = w.map_or(Json::Obj(vec![]), |w| Json::Obj(w.meta.moods.iter().zip(&w.spark).map(|(m, xs)| (m.id.clone(), Json::Arr(xs.iter().map(|x| Json::Num(persona::r6(*x))).collect()))).collect()));
    let of = |g: fn(&Frame) -> &Json| f.map_or(Json::Null, |f| g(f).clone());
    let mut kv = vec![("n", Json::Num(f.map_or(0, |f| f.n) as f64)), ("doc", of(|f| &f.doc)), ("given", of(|f| &f.given)), ("learned", of(|f| &f.learned)), ("spark", spark),
        ("words", f.map_or(Json::Arr(vec![]), |f| panel::to_json(&f.words))),
        ("diverges", bad.map_or(Json::Null, |(l, why)| json::obj(vec![("line", Json::Num(*l as f64)), ("why", json::str(why))]))), ("note", note.map_or(Json::Null, json::str)),
        ("from_checkpoint", w.and_then(|w| w.rep.from).map_or(Json::Null, |n| json::num(n as f64))),
        ("status", w.map_or(Json::Null, |w| json::str(w.rep.live.status.name())))];
    // RFC 0015: the last rest pass (its line number and line), only once the strand has one
    if let Some((line, j)) = w.and_then(|w| w.rest.as_ref()) { kv.push(("last_rest", json::obj(vec![("line", Json::Num(*line as f64)), ("at", j.get("at").cloned().unwrap_or(Json::Null)),
        ("n", j.get("n").cloned().unwrap_or(Json::Null)), ("consolidation", j.get("consolidation").cloned().unwrap_or(Json::Null))]))); }
    // the senses' normals now (§2.11), only when the persona declares them: mean, count and the largest move within 50 events
    if let Some(w) = w.filter(|w| !w.meta.senses.is_empty()) {
        let moved = &w.rep.normals().moved;
        kv.push(("normals", Json::Arr(persona::normals(&w.rep.live.p, &w.rep.live.st).iter().enumerate().map(|(k, (id, mean, n, _))| json::obj(vec![("id", json::str(id)),
            ("mean", json::num(*mean)), ("n", json::num(*n as f64)), ("moved", json::num(moved.get(k).copied().unwrap_or(0.0)))])).collect())));
    }
    // RFC 0009: the last event's exact trajectory, only with engine.trajectory
    if let Some(t) = w.and_then(|w| w.rep.live.traj.clone()) {
        // the same numbers by level name, for the page's settle animation
        if let Some(st) = w.and_then(|w| settle(&w.meta, &t)) { kv.push(("settle", st)); }
        kv.push(("trajectory", t));
    }
    // the open decisions now (§2.8), only with a credit block: each turn and the eligibility a payment at the next event would get
    if let Some(o) = w.and_then(|w| persona::open_decisions(&w.rep.live.p, &w.rep.live.st)) {
        kv.push(("open", Json::Arr(o.iter().map(|(t, e)| json::obj(vec![("turn", json::num(*t as f64)), ("e", json::num(*e))])).collect())));
    }
    json::obj(kv)
}

/// A strand file read as it grows: whole lines (a line counts once its newline is written). It starts over when the
/// file shrinks, disappears, or gets another header (truncated or rotated).
struct Tail { path: String, at: u64, part: Vec<u8>, head: Option<Vec<u8>>, gone: bool, seen: Option<(u64, Option<std::time::SystemTime>)> }
enum Got { Lines(Vec<String>), Restart(&'static str), Nothing }
impl Tail {
    fn new(path: &str) -> Tail { Tail { path: path.to_string(), at: 0, part: vec![], head: None, gone: false, seen: None } }
    fn reset(&mut self) { self.at = 0; self.part.clear(); self.head = None; self.seen = None; }
    fn poll(&mut self) -> Got {
        let Ok(md) = std::fs::metadata(&self.path) else {
            if self.gone { return Got::Nothing; } self.gone = true; self.reset(); return Got::Restart("the strand is gone (removed or rotated): waiting for it");
        };
        if std::mem::replace(&mut self.gone, false) { self.reset(); return Got::Restart("the strand is back: replayed from the start"); }
        let (len, mtime) = (md.len(), md.modified().ok());
        if self.seen == Some((len, mtime)) { return Got::Nothing; }
        let shrank = len < self.at + self.part.len() as u64;
        let mut f = match std::fs::File::open(&self.path) { Ok(f) => f, Err(_) => return Got::Nothing };
        // another header at the top: the file was replaced (rotated) by one at least as long
        let swapped = self.head.as_ref().is_some_and(|h| { let mut b = vec![0u8; h.len()]; f.read_exact(&mut b).is_err() || &b != h });
        if shrank || swapped { self.reset(); return Got::Restart(if shrank { "the strand shrank (truncated or rotated): replayed from the start" } else { "the strand has another header (rotated): replayed from the start" }); }
        self.seen = Some((len, mtime));
        let mut buf = vec![];
        if f.seek(std::io::SeekFrom::Start(self.at + self.part.len() as u64)).and_then(|_| f.read_to_end(&mut buf)).is_err() { return Got::Nothing; }
        self.part.extend_from_slice(&buf);
        let Some(end) = self.part.iter().rposition(|&b| b == b'\n') else { return Got::Nothing };
        let done: Vec<u8> = self.part.drain(..=end).collect();
        self.at += done.len() as u64;
        if self.head.is_none() { self.head = done.iter().position(|&b| b == b'\n').map(|e| done[..e].to_vec()); }
        Got::Lines(crate::live::lines(&String::from_utf8_lossy(&done)).into_iter().map(String::from).collect())
    }
}
/// A strand followed as it grows (`--follow`, `--serve`): the file, the replay (None until a header is in, or when it does not
/// replay), the header's error, the warning to show
struct Followed { tail: Tail, w: Option<Watch>, bad_head: Option<(usize, String)>, note: Option<&'static str> }
impl Followed {
    fn new(path: &str) -> Followed { Followed { tail: Tail::new(path), w: None, bad_head: None, note: None } }
    /// Replay what was appended since the last poll -> None: nothing new; Some(true): the strand starts over (a truncated or
    /// rotated file); Some(false): lines replayed (`each` sees the replay after every event it adds)
    fn poll(&mut self, eng: Engine, each: &mut dyn FnMut(&Watch)) -> Option<bool> {
        match self.tail.poll() {
            Got::Lines(ls) => {
                // the first read of a strand with checkpoint lines starts at the last one (`Watch::start`): its event is drawn at
                // once; after that, appended lines replay from the state in memory
                let mut skip = 0;
                if self.w.is_none() && self.bad_head.is_none() { let refs: Vec<&str> = ls.iter().map(String::as_str).collect();
                    if let Some(k) = Watch::start(&refs) {
                        if let Ok(x) = Watch::open_at(refs[0], refs[k - 1], refs[k], k + 1) { each(&x); self.w = Some(x); skip = k + 1; } } }
                for l in ls.into_iter().skip(skip) { if let Some(x) = &mut self.w { let n = x.rep.live.n; x.feed(&l, eng); if x.rep.live.n != n && x.bad.is_none() { each(x); } continue; }
                    if self.bad_head.is_none() { match Watch::open(&l) { Ok(x) => self.w = Some(x), Err(e) => self.bad_head = Some(e) } } }
                Some(false) }
            Got::Restart(why) => { self.w = None; self.bad_head = None; self.note = Some(why); Some(true) }
            Got::Nothing => None }
    }
}

/// Write to stdout -> false once the reader has gone
fn out(s: &str) -> bool { let mut o = std::io::stdout().lock(); o.write_all(s.as_bytes()).and_then(|_| o.flush()).is_ok() }
/// Plain ASCII: --plain, PROBBIT_THEME set to anything but neon, TERM=dumb
fn ascii(args: &[String]) -> bool {
    args.iter().any(|a| a == "--plain") || std::env::var("PROBBIT_THEME").is_ok_and(|v| !v.is_empty() && v != "neon") || std::env::var("TERM").is_ok_and(|t| t == "dumb")
}

/// `probbit monitor STRAND [--follow] [--once] [--plain] [--fps N]`
pub fn cmd(args: &[String]) {
    if args.iter().any(|a| ["--host", "--bind", "--address"].iter().any(|f| a == f || a.starts_with(&format!("{f}=")))) { crate::fail("monitor --serve binds 127.0.0.1 and no other address, so the page stays on this machine: there is no --host") }
    // §5.8: any number of STRAND [--label KEY=VALUE]... and --dir DIR (the page); the other flags as before
    let (vals, _) = crate::flags_of("monitor");
    let (mut strands, mut dir, mut rest): (Vec<(String, Labels)>, Option<String>, Vec<String>) = (vec![], None, vec![args[0].clone()]);
    let mut i = 1;
    while i < args.len() { let x = &args[i];
        if x == "--label" {
            let Some(v) = args.get(i + 1) else { crate::fail("monitor: --label needs KEY=VALUE") };
            let (k, v) = labels::parse(v).unwrap_or_else(|e| crate::fail(&format!("monitor: {e}")));
            let Some(last) = strands.last_mut() else { crate::fail("monitor: --label KEY=VALUE follows the STRAND it labels: probbit monitor STRAND --label agent=NAME --serve") };
            labels::set(&mut last.1, k, v).unwrap_or_else(|e| crate::fail(&format!("monitor: {}: {e}", last.0))); i += 2;
        } else if x == "--dir" {
            if dir.is_some() { crate::fail("monitor: --dir given twice") }
            let Some(v) = args.get(i + 1).filter(|v| !v.starts_with("--")) else { crate::fail("monitor: --dir needs a value") };
            dir = Some(v.clone()); i += 2;
        } else if x == "--demo" { rest.push(x.clone()); if let Some(v) = args.get(i + 1).filter(|v| !v.starts_with('-')) { rest.push(v.clone()); i += 1; } i += 1; }
        else if vals.contains(&x.as_str()) { rest.extend(args[i..].iter().take(2).cloned()); i += 2; }
        else if x.starts_with("--") { rest.push(x.clone()); i += 1; }
        else { strands.push((x.clone(), Labels::new())); i += 1; }
    }
    let args = &rest[..];
    let path = strands.first().map(|s| s.0.clone());
    let mut a = args.to_vec();
    let mut demo_kind = Demo::Week;
    if a.iter().filter(|x| *x == "--demo").count() > 1 { crate::fail("monitor: --demo given twice") }
    if let Some(i) = a.iter().position(|x| x == "--demo") {
        if let Some(v) = a.get(i + 1).filter(|v| !v.starts_with('-')) {
            demo_kind = match v.as_str() { "week" => Demo::Week, "drives" => Demo::Drives, _ => crate::fail("monitor: --demo expects week or drives") };
            a.remove(i + 1);
        }
    }
    let (v, w) = crate::flags_of("monitor"); crate::check_flags(&a, &v, &w);
    let has = |f: &str| args.iter().any(|a| a == f);
    // --open opens the page, so it serves it: `--open` is `--serve --open`
    let serve_ = has("--serve") || has("--open");
    let sv = if has("--serve") { "--serve" } else { "--open" };
    if has("--follow") && has("--once") { crate::fail("monitor: --once draws one frame and --follow keeps drawing: give one of them") }
    if serve_ && has("--once") { crate::fail(&format!("monitor: --once draws one frame and {sv} keeps serving: give one of them")) }
    if let Some(f) = ["--fps", "--plain"].iter().find(|f| serve_ && has(f)) { crate::fail(&format!("monitor {sv}: {f} does not apply (the page draws itself)")) }
    if !serve_ && has("--port") { crate::fail("monitor: --port goes with --serve (or --open)") }
    let fps: u32 = crate::arg(args, "--fps", 10); if !(1..=60).contains(&fps) { crate::fail("monitor: --fps N, from 1 to 60") }
    let port: u16 = crate::arg(args, "--port", 0);
    let engine = crate::persona_engine(); let eng: Engine = &engine;
    let tty = theme::stdout_is_terminal();
    let s = Style { th: theme::stdout(args), ascii: ascii(args), cols: if tty { theme::size(1).0.max(20) } else { usize::MAX } };
    if has("--demo") {
        if path.is_some() || dir.is_some() { crate::fail("monitor --demo plays a synthetic scenario: give no STRAND") }
        if has("--follow") { crate::fail("monitor --demo: --follow does not apply (the demo is a synthetic scenario)") }
        if serve_ { let hub = Hub::default(); let site = Arc::new(Site { demo: Some(hub.clone()), ..Site::default() });
            serve(port, has("--open"), &site, &mut || feed_demo(&hub, demo_kind, eng)) }
        return demo(&s, has("--once"), demo_kind, eng);
    }
    // several strands, a directory or labels: the page only (its identity header and switcher)
    let many = strands.len() > 1 || dir.is_some() || strands.iter().any(|s| !s.1.is_empty());
    if many && !serve_ { crate::fail(if strands.len() > 1 || dir.is_some() { "monitor: several strands (or --dir) go with --serve or --open: the terminal draws one" } else { "monitor: --label goes with --serve or --open (the page's identity header)" }) }
    if serve_ {
        if strands.is_empty() && dir.is_none() { crate::fail("monitor: the strand file comes before the flags: probbit monitor STRAND [flags] (or probbit monitor --dir DIR --serve, or probbit monitor --demo)") }
        for (p, _) in &strands { std::fs::read(p).unwrap_or_else(|e| crate::fail(&format!("monitor: cannot read {p}: {e}"))); }
        if let Some(d) = &dir { if !std::path::Path::new(d).is_dir() { crate::fail(&format!("monitor: --dir {d}: not a directory")) } }
        let site = Arc::new(Site { dir: dir.clone(), ..Site::default() });
        for (p, l) in strands.iter() { let bytes = std::fs::read(p).unwrap_or_default(); let t = String::from_utf8_lossy(&bytes);
            if let Some(h) = t.split_inclusive('\n').next().filter(|h| h.ends_with('\n')) { if json::parse(h.trim_end()).ok().and_then(|h| h.get("probbit_strand").and_then(Json::as_f64)) != Some(1.0) { crate::fail(&format!("monitor: {p} is not a probbit strand (format 1)")) } }
            add(&site, p.clone(), l.clone()); }
        if let Some(d) = &dir { for p in scan(d) { add(&site, p, Labels::new()); } }
        serve(port, has("--open"), &site, &mut || watch_dir(&site));
    }
    let Some(path) = path else { crate::fail("monitor: the strand file comes before the flags: probbit monitor STRAND [flags] (or probbit monitor --demo)") };
    let bytes = std::fs::read(&path).unwrap_or_else(|e| crate::fail(&format!("monitor: cannot read {path}: {e}")));
    let text = String::from_utf8_lossy(&bytes);
    // whole lines: a last line without its newline is still being written
    let (done, rest) = match text.rfind('\n') { Some(i) => (&text[..=i], &text[i + 1..]), None => ("", &text[..]) };
    let lines = crate::live::lines(done);
    let strand = |h: &str| json::parse(h).ok().and_then(|h| h.get("probbit_strand").and_then(Json::as_f64)) == Some(1.0);
    if let Some(h) = lines.first() { if !strand(h) { crate::fail(&format!("monitor: {path} is not a probbit strand (format 1)")) } }
    // --follow waits for a header still to be written; one frame needs it now
    if has("--follow") { follow(&path, &s, fps, eng) }
    let head = lines.first().copied().unwrap_or(rest);
    if head.is_empty() { crate::fail(&format!("monitor: {path} is empty: not a strand")) }
    if !strand(head) { crate::fail(&format!("monitor: {path} is not a probbit strand (format 1)")) }
    let note = (!rest.is_empty() && !lines.is_empty()).then(|| format!("line {} is incomplete (no newline at its end): not replayed", lines.len() + 1));
    // a strand with checkpoint lines replays from the last one (the board starts at its event and draws the latest one), unless
    // its persona has an expectation block: then from the header (`Watch::start`)
    let start = Watch::start(&lines);
    let opened = match start { Some(k) => Watch::open_at(head, lines[k - 1], lines[k], k + 1), None => Watch::open(head) };
    let (w, bad_head) = match opened { Ok(mut w) => { for l in lines.iter().skip(start.map_or(1, |k| k + 1)) { w.feed(l, eng); } (Some(w), None) } Err(e) => (None, Some(e)) };
    let fr = frame(w.as_ref(), bad_head.as_ref(), &path, note.as_deref(), &s);
    out(&fr.iter().map(|l| crate::tui::clip(l, s.cols) + "\n").collect::<String>());
    if bad_head.is_some() || w.is_some_and(|w| w.bad.is_some()) { std::process::exit(1) }
}

/// Draw a frame in place at a terminal of `size` (columns, rows) -> false once the reader has gone. A frame taller than the
/// terminal would scroll on every redraw: the rows that fit are kept.
fn redraw(mut fr: Vec<String>, size: (usize, usize)) -> bool {
    fr.truncate(size.1.saturating_sub(1).max(1));
    out(&format!("\x1b[H{}\x1b[J", fr.iter().map(|l| crate::tui::clip(l, size.0.max(20)) + "\x1b[K\n").collect::<String>()))
}

/// The existing tutor week stays byte-compatible; drives is a separate, explicitly synthetic scenario.
#[derive(Clone, Copy)]
enum Demo { Week, Drives }
impl Demo {
    fn name(self) -> &'static str { match self { Demo::Week => "week", Demo::Drives => "drives" } }
    fn lines(self, eng: Engine) -> Vec<String> { match self { Demo::Week => week(eng), Demo::Drives => drive_demo(eng) } }
    fn note(self, cycle: u64, note: Option<&str>) -> String {
        format!("synthetic {} demo | loop {cycle}{}", self.name(), note.map_or(String::new(), |n| format!(" | {n}")))
    }
}
/// A small, replayable life: goal cues, progress, wins, a setback and quiet time. No external actions or sources.
fn drive_demo(eng: Engine) -> Vec<String> {
    let doc = json::parse(r#"{
      "probbit_persona":1,"identity":{"name":"Scout","version":"1.0.0","seed":2},
      "traits":[
        {"id":"initiative","levels":["follow","suggest","lead"],"say":["answer the question","suggest a next step","set a small exercise"]},
        {"id":"caution","levels":["bold","measured","careful"],"say":["keep moving","check the essentials","double-check the result"]}],
      "moods":[{"id":"valence","levels":["down","even","up"],"inertia":0.5,"half_life_hours":8}],
      "inputs":[{"id":"deadline","kind":"flag"}],
      "habits":[{"id":"check_when_due","when":{"deadline":true},"then":{"pursue":["check"]},"say":"check before proceeding"}],
      "drives":{"goals":[{"id":"learn","say":"explore the lesson","interest":1.5},{"id":"rest","say":"take a break","interest":0.6},{"id":"check","say":"verify the result","interest":0.8}],
        "effects":{"wanting":{"initiative":0.6},"afterglow":{"valence":0.8},"surprise":{"valence":0.5}}}
    }"#).expect("embedded drives persona");
    let p = persona::build(&doc).expect("valid drives demo persona");
    let st = persona::init(&p, Some(2), true, eng);
    let (mut live, head) = crate::live::Live::start(p, &doc, st, crate::live::Clock::Fixed, &crate::live::engine());
    let mut lines = vec![head];
    for event in [
        r#"{"goals":{"learn":{"cue":true,"novelty":0.8}}}"#,
        r#"{"elapsed_hours":1,"goals":{"learn":{"progress":0.6}}}"#,
        r#"{"elapsed_hours":1,"goals":{"learn":{"win":1}}}"#,
        r#"{"elapsed_hours":1,"goals":{"learn":{"win":0.1}}}"#,
        r#"{"elapsed_hours":1,"goals":{"rest":{"cue":true},"learn":{"setback":0.5}}}"#,
        r#"{"elapsed_hours":12}"#,
        r#"{"elapsed_hours":1,"deadline":true,"goals":{"check":{"cue":true,"deadline_hours":1}}}"#,
        r#"{"elapsed_hours":1,"goals":{"check":{"win":0.8}}}"#,
    ] {
        let ev = json::parse(event).expect("embedded goal signals");
        let (_, line) = live.event(&ev, eng).expect("valid drives demo event"); lines.push(line);
    }
    lines
}
/// The demo's week: the tutor's scripted week (`live::week`, seed 2) written to a temporary strand, read back and removed -> its
/// lines
fn week(eng: Engine) -> Vec<String> {
    let doc = persona::parse_doc(TUTOR, false).unwrap_or_else(|e| crate::fail(&format!("monitor --demo: the tutor: {e}")));
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.subsec_nanos());
    let f = std::env::temp_dir().join(format!("probbit-monitor-demo-{}-{nanos}.strand", std::process::id()));
    let fp = f.to_string_lossy().to_string();
    let made = crate::live::week(&doc, Some(2), Some(&fp), None, &mut |_: &str| {}, eng);
    let text = std::fs::read_to_string(&f); let _ = std::fs::remove_file(&f);
    if let Err(e) = made { crate::fail(&format!("monitor --demo: {}: {}", e.path, e.msg)) }
    let text = text.unwrap_or_else(|e| crate::fail(&format!("monitor --demo: cannot read the week back: {e}")));
    crate::live::lines(&text).into_iter().map(String::from).collect()
}
/// Replay the week paced, 1 s per hour and each night in 2 s: `show` gets the replay before the week, before each night (with the
/// night's note) and after each event -> false once `show` returns false
fn play(lines: &[String], eng: Engine, show: &mut dyn FnMut(&Watch, Option<&str>) -> bool) -> bool {
    let mut w = Watch::open(&lines[0]).unwrap_or_else(|(n, why)| crate::fail(&format!("monitor --demo: line {n}: {why}")));
    if !show(&w, None) { return false; }
    for l in &lines[1..] {
        let h = json::parse(l).ok().and_then(|j| j.get("inputs").and_then(|i| i.get("elapsed_hours")).and_then(Json::as_f64)).unwrap_or(0.0);
        if h > 1.0 { if !show(&w, Some(&format!("night: {} quiet hours; moods decay by their half-lives", whole(h)))) { return false; }
            std::thread::sleep(Duration::from_secs(2)); }
        else { std::thread::sleep(Duration::from_secs_f64(h)); }
        w.feed(l, eng);
        if !show(&w, None) { return false; }
    }
    true
}
/// `--demo`: the week replayed: at a terminal paced (`play`), otherwise (or with --once) straight to its last frame
fn demo(s: &Style, once: bool, kind: Demo, eng: Engine) {
    let lines = kind.lines(eng);
    if s.cols == usize::MAX || once {
        let mut w = Watch::open(&lines[0]).unwrap_or_else(|(n, why)| crate::fail(&format!("monitor --demo: line {n}: {why}")));
        for l in &lines[1..] { w.feed(l, eng); }
        out(&(frame(Some(&w), None, "", Some(&kind.note(1, Some("complete; --serve replays in a loop"))), s).join("\n") + "\n")); return;
    }
    theme::restore_cursor_on_interrupt();
    let size = || theme::size(1);
    if !out("\x1b[?25l\x1b[2J") || !play(&lines, eng, &mut |w, note| redraw(frame(Some(w), None, "", Some(&kind.note(1, note)), &Style { cols: size().0, ..*s }), size())) { std::process::exit(0) }
    out("\x1b[?25h");
}

// ---------------------------------------------------------------- --serve
/// What the page shows, shared by the thread that replays and the connections: the layout and the latest frame (each with a
/// counter that moves when it changes; layout 0 = nothing replayed yet) and the latest stance documents (`last` = the event
/// number of the newest)
#[derive(Default)]
struct Board { meta: String, frame: String, layout: u64, seq: u64, docs: VecDeque<String>, last: u64, name: String, glyph: String }
type Hub = Arc<(Mutex<Board>, Condvar)>;
fn board(hub: &Hub) -> std::sync::MutexGuard<'_, Board> { hub.0.lock().unwrap_or_else(|e| e.into_inner()) }
/// The board once the replay has published (a request that comes in while the strand is read waits for it, at most 30 s)
fn ready(hub: &Hub) -> std::sync::MutexGuard<'_, Board> {
    let mut b = board(hub); let until = std::time::Instant::now() + Duration::from_secs(30);
    while b.layout == 0 { let left = until.saturating_duration_since(std::time::Instant::now()); if left.is_zero() { break; } b = hub.1.wait_timeout(b, left).unwrap_or_else(|e| e.into_inner()).0; }
    b
}
/// Show a new frame (and the layout, if it changed) to every page; `docs`: the events replayed since the last publish, with their
/// numbers; `over`: the strand starts over (the documents kept so far go)
fn publish(hub: &Hub, meta: &Json, frame: &Json, docs: Vec<(u64, String)>, over: bool) {
    // the switcher's row (`/strands`): the persona's name and the latest glyph
    let name = meta.get("persona").and_then(|p| p.get("name")).and_then(Json::as_str).unwrap_or("").to_string();
    let glyph = frame.get("doc").and_then(|d| d.get("glyph")).and_then(Json::as_str).unwrap_or("").to_string();
    let (meta, frame) = (persona::canon(meta), persona::canon(frame));
    let mut b = board(hub); b.name = name; b.glyph = glyph;
    if over { b.docs.clear(); b.last = 0; }
    // an event already kept (a segment opens at its sealed line, the last event of the file before) is not kept twice
    for (n, d) in docs { if n <= b.last { continue; } b.docs.push_back(d); b.last = n; if b.docs.len() > KEEP { b.docs.pop_front(); } }
    if b.meta != meta || b.layout == 0 { b.meta = meta; b.layout += 1; }
    b.frame = frame; b.seq += 1;
    hub.1.notify_all();
}
/// An event's stance document as `probbit live` prints it
fn doc_of(w: &Watch) -> (u64, String) { w.last.as_ref().map_or((0, String::new()), |f| (f.n, persona::canon(&f.doc))) }

/// One strand the page serves (§5.8): its id (the file name without its extension, made unique), its path, the labels given
/// with `--label` after it (over its sidecar's), and its board
struct Entry { id: String, path: String, over: Labels, hub: Hub }
/// What `--serve` serves: the demo's board, or the strands (in the order given, then as found in `dir`)
#[derive(Default)]
struct Site { demo: Option<Hub>, list: Mutex<Vec<Arc<Entry>>>, dir: Option<String> }
fn entries(site: &Site) -> std::sync::MutexGuard<'_, Vec<Arc<Entry>>> { site.list.lock().unwrap_or_else(|e| e.into_inner()) }
/// The strand's id: its file name without the extension, characters other than A-Z a-z 0-9 _ . - as `-`, and `-2`, `-3`, ...
/// after one already taken
fn id_of(path: &str, taken: &[Arc<Entry>]) -> String {
    let stem: String = std::path::Path::new(path).file_stem().map_or(String::new(), |s| s.to_string_lossy().chars().map(|c| if c.is_ascii_alphanumeric() || "_.-".contains(c) { c } else { '-' }).collect());
    let stem = if stem.is_empty() { "strand".to_string() } else { stem };
    let mut id = stem.clone(); let mut k = 2;
    while taken.iter().any(|e| e.id == id) { id = format!("{stem}-{k}"); k += 1; }
    id
}
/// Serve one more strand: its entry, and a thread that follows it (`feed_strand`, with its own engine)
fn add(site: &Site, path: String, over: Labels) {
    let hub = Hub::default();
    { let mut l = entries(site); if l.iter().any(|e| e.path == path) { return; } let id = id_of(&path, &l); l.push(Arc::new(Entry { id, path: path.clone(), over, hub: hub.clone() })); }
    std::thread::spawn(move || { let engine = crate::persona_engine(); feed_strand(&path, &hub, &engine); });
}
/// The `*.strand` files in `dir`, by name; a life's segments (§5.7: `NAME.0001.strand`, `NAME.0002.strand`, ... after
/// `NAME.strand`, without a gap) are one strand, `NAME.strand`
fn scan(dir: &str) -> Vec<String> {
    let mut v: Vec<String> = std::fs::read_dir(dir).into_iter().flatten().flatten().map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "strand") && p.is_file()).map(|p| p.to_string_lossy().into_owned()).collect();
    v.sort();
    let segs: Vec<String> = v.iter().flat_map(|f| (1..).map(|k| crate::live::segment_name(f, k)).take_while(|s| v.contains(s)).collect::<Vec<_>>()).collect();
    v.retain(|f| !segs.contains(f)); v
}
/// The newest segment of the strand `base` from index `k` on (§5.7): the next one counts once its header and sealed line are
/// both in (a seal writes them together; a reader can see the header first)
fn newest(base: &str, mut k: u64) -> u64 {
    let ready = |f: &str| { let Ok(mut h) = std::fs::File::open(f) else { return false }; let (mut n, mut b) = (0, [0u8; 8192]);
        loop { match h.read(&mut b) { Ok(0) | Err(_) => return false, Ok(m) => { n += b[..m].iter().filter(|&&c| c == b'\n').count(); if n >= 2 { return true } } } } };
    while ready(&crate::live::segment_name(base, k + 1)) { k += 1; }
    k
}
/// With --dir, every second: the strands added to it since are served too (a strand removed stays listed, as gone); without, nothing
fn watch_dir(site: &Site) -> ! {
    loop { if let Some(d) = &site.dir { for p in scan(d) { add(site, p, Labels::new()); } } std::thread::sleep(Duration::from_secs(1)); }
}
/// `/strands`: the strands served, for the page's identity header and switcher: id, file name, labels (the sidecar's, with
/// `--label` over them; `labels_error` when the sidecar breaks the rules), persona, latest event, its glyph, and the seconds
/// since the strand file was last written (null when it cannot be read)
fn strands_json(site: &Site) -> String {
    let name = |p: &str| std::path::Path::new(p).file_name().map_or(String::new(), |n| n.to_string_lossy().into_owned());
    let now = std::time::SystemTime::now();
    let rows: Vec<Json> = entries(site).iter().map(|e| {
        let (mut l, mut err) = match labels::read(&e.path) { Ok(l) => (l, None), Err(x) => (Labels::new(), Some(x.replace(&labels::sidecar(&e.path), &name(&labels::sidecar(&e.path))))) };
        for (k, v) in &e.over { if let Err(x) = labels::set(&mut l, k.clone(), v.clone()) { err = Some(format!("{k}: {x}")); } }
        // a life in segments: its newest file (the one written to)
        let k = newest(&e.path, 0);
        let age = std::fs::metadata(crate::live::segment_name(&e.path, k)).and_then(|m| m.modified()).ok().and_then(|t| now.duration_since(t).ok()).map_or(Json::Null, |d| json::num(d.as_secs() as f64));
        let b = board(&e.hub); let opt = |x: &str| if x.is_empty() { Json::Null } else { json::str(x) };
        let mut kv = vec![("id", json::str(&e.id)), ("strand", json::str(&name(&e.path))), ("labels", labels::to_json(&l)), ("persona", opt(&b.name)), ("n", json::num(b.last as f64)), ("glyph", opt(&b.glyph)), ("age_s", age)];
        if let Some(x) = err { kv.push(("labels_error", json::str(&x))); }
        if k > 0 { kv.push(("segment", json::num(k as f64))); }
        json::obj(kv) }).collect();
    persona::canon(&json::obj(vec![("demo", Json::Bool(site.demo.is_some())), ("dir", site.dir.as_deref().map_or(Json::Null, |d| json::str(&name(d)))), ("strands", Json::Arr(rows))])) + "\n"
}
/// The board for `/events` and `/doc/N` (the demo's, else the first strand's) or for `/s/ID/...` (strand ID's)
fn hub_for(site: &Site, id: Option<&str>) -> Option<Hub> {
    match id { None => site.demo.clone().or_else(|| entries(site).first().map(|e| e.hub.clone())), Some(id) => entries(site).iter().find(|e| e.id == id).map(|e| e.hub.clone()) }
}

/// `--serve`: listen on 127.0.0.1:`port` (0: a free port), say the URL on stdout, open the browser if asked, serve every
/// connection on its own thread, and run `feed` (it publishes the frames, or watches the directory; it never returns)
fn serve(port: u16, open: bool, site: &Arc<Site>, feed: &mut dyn FnMut()) -> ! {
    let l = TcpListener::bind((Ipv4Addr::LOCALHOST, port)).unwrap_or_else(|e| crate::fail(&if e.kind() == std::io::ErrorKind::AddrInUse {
        format!("monitor --serve: cannot listen on 127.0.0.1:{port}: {e}: another program holds this port; pick another with --port N, or leave --port out for a free one")
    } else { format!("monitor --serve: cannot listen on 127.0.0.1:{port}: {e}") }));
    let port = l.local_addr().map_or(port, |a| a.port());
    let url = format!("http://127.0.0.1:{port}/");
    out(&format!("{url}\n"));
    crate::err_line(&format!("probbit monitor: the page is at {url} (on this machine, 127.0.0.1); Ctrl-C quits"));
    let h = site.clone();
    std::thread::spawn(move || {
        let open = Arc::new(AtomicUsize::new(0));
        for c in l.incoming() { match c {
            Ok(c) => { let (h, open) = (h.clone(), open.clone()); std::thread::spawn(move || { if open.fetch_add(1, Ordering::SeqCst) < CONNS { handle(c, &h, port); } else { busy(c); } open.fetch_sub(1, Ordering::SeqCst); }); }
            Err(_) => std::thread::sleep(Duration::from_millis(50)) } }
    });
    if open { browse(&url); }
    feed();
    loop { std::thread::park(); }
}
/// The strand followed for the page: every 100 ms, what was appended is replayed and published. A life in segments (§5.7) is
/// followed in its newest file: when a seal writes the next one, the rest of the sealed file is replayed, then the page goes
/// on in the new file (it opens at its sealed line, the event already shown); the layout and the event count carry on
fn feed_strand(path: &str, hub: &Hub, eng: Engine) -> ! {
    let mut k = newest(path, 0);
    let mut fl = Followed::new(&crate::live::segment_name(path, k));
    let mut fresh = true;
    loop { step(&mut fl, path, hub, eng, &mut fresh, CATCH_UP);
        let n = newest(path, k);
        if n != k { step(&mut fl, path, hub, eng, &mut fresh, CATCH_UP); k = n; fl = Followed::new(&crate::live::segment_name(path, k)); continue; }
        std::thread::sleep(Duration::from_millis(100)); }
}
/// One poll of the strand for the page: what was appended is replayed and published. A long replay (a large strand read from
/// its header) is also published every `every` on the way, with a note, so the page draws while it catches up.
fn step(fl: &mut Followed, path: &str, hub: &Hub, eng: Engine, fresh: &mut bool, every: Duration) {
    let (mut docs, mut tick) = (vec![], std::time::Instant::now());
    let got = fl.poll(eng, &mut |w| { docs.push(doc_of(w));
        if tick.elapsed() >= every { tick = std::time::Instant::now();
            publish(hub, &meta_json(Some(w), path), &frame_json(Some(w), None, Some(CATCHING_UP)), std::mem::take(&mut docs), false); } });
    if got.is_some() || *fresh { publish(hub, &meta_json(fl.w.as_ref(), path), &frame_json(fl.w.as_ref(), fl.bad_head.as_ref(), fl.note), docs, got == Some(true)); *fresh = false; }
}
/// How often a long replay shows where it is, and what it says meanwhile
const CATCH_UP: Duration = Duration::from_millis(250);
const CATCHING_UP: &str = "replaying the strand from its header: the board catches up";
/// A synthetic scenario for the page, paced and explicitly labelled. Completion holds for 5 s, then the cycle number
/// advances with a fresh event counter and an empty document cache.
fn feed_demo(hub: &Hub, kind: Demo, eng: Engine) -> ! {
    let lines = kind.lines(eng);
    let mut cycle = 1;
    loop {
        let mut over = true;
        let mut last = None;
        play(&lines, eng, &mut |w, note| {
            let docs = if w.last.is_some() && note.is_none() { vec![doc_of(w)] } else { vec![] };
            let mut meta = meta_json(Some(w), "");
            if let Json::Obj(v) = &mut meta { v.push(("demo_kind".into(), json::str(kind.name()))); }
            let frame = frame_json(Some(w), None, Some(&kind.note(cycle, note)));
            publish(hub, &meta, &frame, docs, std::mem::take(&mut over)); last = Some((meta, frame)); true });
        if let Some((meta, mut frame)) = last {
            if let Json::Obj(v) = &mut frame { if let Some((_, note)) = v.iter_mut().find(|(k, _)| k == "note") {
                *note = json::str(&kind.note(cycle, Some("complete; restarting in 5 s"))); } }
            publish(hub, &meta, &frame, vec![], false);
        }
        std::thread::sleep(Duration::from_secs(5)); cycle += 1;
    }
}

/// One connection: a GET of `/`, `/events`, `/doc/N`, `/strands`, `/s/ID/events` or `/s/ID/doc/N` addressed to 127.0.0.1:PORT
/// or localhost:PORT (a page of another site that resolves its name to 127.0.0.1 is refused: it would read the documents);
/// anything else is answered with an error
fn handle(mut c: TcpStream, site: &Site, port: u16) {
    let _ = c.set_read_timeout(Some(Duration::from_secs(5)));
    let _ = c.set_write_timeout(Some(Duration::from_secs(30)));
    let mut head = vec![]; let mut buf = [0u8; 2048];
    while !head.windows(4).any(|w| w == b"\r\n\r\n") {
        if head.len() > 16_384 { return reply(&mut c, "431 Request Header Fields Too Large", "text/plain", b"request head too large\n"); }
        match c.read(&mut buf) { Ok(0) | Err(_) => return, Ok(k) => head.extend_from_slice(&buf[..k]) }
    }
    let text = String::from_utf8_lossy(&head);
    let mut ls = text.split("\r\n");
    let mut req = ls.next().unwrap_or("").split(' ');
    let (method, target) = (req.next().unwrap_or(""), req.next().unwrap_or(""));
    let host = ls.find_map(|l| l.split_once(':').filter(|(k, _)| k.trim().eq_ignore_ascii_case("host")).map(|(_, v)| v.trim().to_ascii_lowercase()));
    if !host.is_some_and(|h| h == format!("127.0.0.1:{port}") || h == format!("localhost:{port}")) { return reply(&mut c, "403 Forbidden", "text/plain", b"this monitor answers requests addressed to 127.0.0.1 or localhost\n"); }
    if method != "GET" { return reply(&mut c, "405 Method Not Allowed", "text/plain", b"this server answers GET\n"); }
    let path = target.split(['?', '#']).next().unwrap_or("");
    // `/s/ID/rest`: strand ID's `/rest`; every other path is the single-strand one (the demo's, else the first strand's)
    let (id, path) = match path.strip_prefix("/s/") { Some(r) => r.split_once('/').map_or((Some(r), ""), |(id, _)| (Some(id), &path[3 + id.len()..])), None => (None, path) };
    if id.is_none() && path == "/" { return reply(&mut c, "200 OK", "text/html; charset=utf-8", PAGE.as_bytes()); }
    if id.is_none() && path == "/strands" { return reply(&mut c, "200 OK", "application/json; charset=utf-8", strands_json(site).as_bytes()); }
    // RFC 0013, the Mirror tab: `/reflection?last=N` (default 50), computed from the strand file as it is now
    if path == "/reflection" {
        let file = match id { None => entries(site).first().map(|e| e.path.clone()), Some(id) => entries(site).iter().find(|e| e.id == id).map(|e| e.path.clone()) };
        let Some(file) = file else { return reply(&mut c, "404 Not Found", "text/plain", b"no strand file: the reflection reads a strand (the demo has none)\n") };
        let last = target.split_once('?').and_then(|(_, q)| q.split('&').find_map(|kv| kv.strip_prefix("last="))).map_or(Some(50), |v| v.parse::<u64>().ok().filter(|n| *n >= 1));
        let Some(last) = last else { return reply(&mut c, "400 Bad Request", "text/plain", b"last=N: a whole number >= 1\n") };
        return match std::fs::read_to_string(&file) { Ok(t) => reply(&mut c, "200 OK", "application/json; charset=utf-8", format!("{}\n", mirror(&t, last)).as_bytes()),
            Err(_) => reply(&mut c, "404 Not Found", "text/plain", b"the strand file cannot be read\n") };
    }
    let Some(hub) = hub_for(site, id).filter(|_| path == "/events" || path.starts_with("/doc/")) else {
        return reply(&mut c, "404 Not Found", "text/plain", if id.is_some() { b"not found: /s/ID/events and /s/ID/doc/N serve the strand ID that /strands lists\n" } else { b"not found: / is the page, /events its stream, /doc/N a stance document, /strands the strands served\n" }) };
    let hub = &hub;
    match path {
        "/events" => events(c, hub, Duration::from_secs(BEAT)),
        p if p.starts_with("/doc/") => {
            let b = ready(hub); let k = b.docs.len() as u64;
            let doc = p[5..].parse::<u64>().ok().filter(|&n| n >= 1 && n <= b.last && b.last - n < k).map(|n| b.docs[(k - 1 - (b.last - n)) as usize].clone() + "\n");
            let msg = format!("no document for {}: events {}..={} are kept\n", &p[5..], (b.last + 1).saturating_sub(k).max(1), b.last);
            drop(b);
            match doc { Some(d) => reply(&mut c, "200 OK", "application/json; charset=utf-8", d.as_bytes()), None => reply(&mut c, "404 Not Found", "text/plain", msg.as_bytes()) } }
        _ => {} }
}
/// The Mirror's document (RFC 0013): `live`, the reflection of the last `last` events computed now (no solve); `logged`, the
/// latest reflection line the strand carries (its line number, its document, and whether it is the reflection of the lines
/// before it: `matches`), or null; `error` when the strand cannot be reflected
pub fn mirror(text: &str, last: u64) -> String {
    use probbit_persona::reflect;
    let ls = crate::live::lines(text); let o = |v: Vec<(&str, Json)>| Json::Obj(v.into_iter().map(|(k, x)| (k.to_string(), x)).collect());
    let logged = (1..ls.len()).rev().find(|&i| reflect::is_reflection_line(ls[i])).map_or(Json::Null, |i| o(vec![("line", Json::Num((i + 1) as f64)),
        ("doc", json::parse(ls[i]).ok().and_then(|j| j.get("reflection").cloned()).unwrap_or(Json::Null)), ("matches", Json::Bool(reflect::check_line(&ls[..i], ls[i]).is_ok()))]));
    persona::canon(&match reflect::reflect_window(&ls, reflect::Window::Last(last)) {
        Ok(r) => o(vec![("live", r.doc), ("logged", logged)]),
        Err(e) => o(vec![("error", Json::Str(e.msg)), ("logged", logged)]) })
}
/// The headers every answer carries: no caching, no sniffing, and for the page a policy that lets it load nothing and connect
/// to nothing but its own server
const SAFE: &str = "Cache-Control: no-store\r\nX-Content-Type-Options: nosniff\r\nReferrer-Policy: no-referrer\r\nContent-Security-Policy: default-src 'none'; style-src 'unsafe-inline'; script-src 'unsafe-inline'; connect-src 'self'; img-src data:; base-uri 'none'; form-action 'none'; frame-ancestors 'none'\r\n";
fn reply(c: &mut TcpStream, status: &str, kind: &str, body: &[u8]) {
    let head = format!("HTTP/1.1 {status}\r\nContent-Type: {kind}\r\nContent-Length: {}\r\n{SAFE}Connection: close\r\n\r\n", body.len());
    let _ = c.write_all(head.as_bytes()).and_then(|_| c.write_all(body)).and_then(|_| c.flush());
}
fn busy(mut c: TcpStream) { let _ = c.set_write_timeout(Some(Duration::from_secs(5))); reply(&mut c, "503 Service Unavailable", "text/plain", b"too many connections\n"); }
/// `/events`: the layout and the latest frame, then each new frame (at most 20 a second: a page that falls behind gets the latest),
/// a new layout before the frame when the strand starts over as another, and a heartbeat comment after `beat` quiet (15 s)
fn events(mut c: TcpStream, hub: &Hub, beat: Duration) {
    let _ = c.set_nodelay(true);
    // X-Accel-Buffering: a reverse proxy (nginx) would otherwise hold the stream in its buffer
    let head = format!("HTTP/1.1 200 OK\r\nContent-Type: text/event-stream; charset=utf-8\r\n{SAFE}X-Accel-Buffering: no\r\n\r\n");
    if c.write_all(head.as_bytes()).and_then(|_| c.flush()).is_err() { return; }
    let (mut layout, mut seq) = (0u64, 0u64);
    loop {
        let (meta, frame) = {
            let mut b = board(hub);
            let until = std::time::Instant::now() + beat;
            while b.layout == 0 || (b.layout == layout && b.seq == seq) {
                let left = until.saturating_duration_since(std::time::Instant::now());
                if left.is_zero() { break; }
                b = hub.1.wait_timeout(b, left).unwrap_or_else(|e| e.into_inner()).0;
            }
            let meta = (b.layout != layout && b.layout != 0).then(|| { layout = b.layout; b.meta.clone() });
            let frame = (b.seq != seq && b.layout != 0).then(|| { seq = b.seq; b.frame.clone() });
            (meta, frame)
        };
        let mut msg = String::new();
        if let Some(m) = meta { msg += &format!("event: meta\ndata: {m}\n\n"); }
        if let Some(f) = frame { msg += &format!("event: frame\ndata: {f}\n\n"); }
        if msg.is_empty() { msg = ": heartbeat\n\n".into(); }
        if c.write_all(msg.as_bytes()).and_then(|_| c.flush()).is_err() { return; }
        std::thread::sleep(Duration::from_millis(50));
    }
}
/// `--open`: the page in the default browser (`BROWSER` if set; else macOS `open`, Windows `cmd /c start`, elsewhere `xdg-open`),
/// best effort: a browser that does not start is said on stderr and fails nothing
fn browse(url: &str) {
    use std::process::{Command, Stdio};
    let mut cmd = match std::env::var("BROWSER").ok().filter(|b| !b.trim().is_empty()) {
        Some(b) => Command::new(b.trim()),
        None if cfg!(target_os = "macos") => Command::new("open"),
        None if cfg!(windows) => { let mut c = Command::new("cmd"); c.args(["/c", "start", ""]); c }
        None => Command::new("xdg-open") };
    match cmd.arg(url).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn() {
        Ok(mut child) => { std::thread::spawn(move || { let _ = child.wait(); }); }
        Err(e) => crate::err_line(&format!("probbit monitor: --open: no browser started ({e}); open {url} by hand")) }
}

/// `--follow`: poll the strand every 100 ms, replay the lines appended, redraw (at most `fps` times a second; in place at a
/// terminal, else one frame after another, each followed by an empty line). Runs until Ctrl-C or until the reader goes away.
fn follow(path: &str, s: &Style, fps: u32, eng: Engine) -> ! {
    let tty = s.cols != usize::MAX;
    if tty { theme::restore_cursor_on_interrupt(); if !out("\x1b[?25l\x1b[2J") { std::process::exit(0) } }
    let mut fl = Followed::new(path);
    let (mut dirty, mut drawn, mut size) = (true, None::<std::time::Instant>, (0, 0));
    let gap = std::time::Duration::from_secs_f64(1.0 / fps as f64);
    loop {
        // a long replay (a large strand read from its header) is drawn on the way at a terminal, with a note
        let mut tick = std::time::Instant::now();
        let mut catch_up = |w: &Watch| if tty && tick.elapsed() >= CATCH_UP { tick = std::time::Instant::now(); let size = theme::size(1);
            redraw(frame(Some(w), None, path, Some(CATCHING_UP), &Style { cols: size.0.max(20), ..*s }), size); };
        if fl.poll(eng, &mut catch_up).is_some() { dirty = true; }
        let now = if tty { theme::size(1) } else { (0, 0) };
        if now != size { size = now; dirty = true; }
        if dirty && drawn.map_or(true, |t| t.elapsed() >= gap) {
            let cols = if tty { size.0.max(20) } else { usize::MAX };
            let fr = frame(fl.w.as_ref(), fl.bad_head.as_ref(), path, fl.note, &Style { cols, ..*s });
            if !(if tty { redraw(fr, size) } else { out(&(fr.join("\n") + "\n\n")) }) { std::process::exit(0) }
            dirty = false; drawn = Some(std::time::Instant::now());
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
}

#[cfg(test)]
mod tests {
    //! The monitor's documents are the strand's stances: replayed with `live::Replay`, each one's sha256 is the line's `stance`
    //! digest, on strands of the three example personas; a document with drives fields draws without error
    use super::*;
    use crate::live::{self, Clock, Live};

    fn run(prog: &Json, f: &persona::Flags) -> Json { persona::run_program(prog, f, 1, 100, 0) }
    /// A strand of `n` events for the persona at `path`: every declared flag on in turn, numbers 0 / 0.5 / max, level inputs
    /// through their levels, the clock 0.5 to 13 h
    fn strand(path: &str, n: usize) -> String {
        let (p, doc) = persona::load(path).unwrap();
        let d = persona::describe(&p);
        let ins: Vec<(String, Json)> = d.get("inputs").and_then(Json::as_obj).unwrap().to_vec();
        let (mut lv, header) = Live::start(p.clone(), &doc, persona::init(&p, Some(5), true, &run), Clock::Fixed, "probbit test");
        let mut text = format!("{header}\n");
        for t in 0..n {
            let mut ev = vec![("elapsed_hours".to_string(), Json::Num([0.5, 1.0, 13.0, 2.25][t % 4]))];
            for (k, (id, x)) in ins.iter().enumerate() {
                match x.get("kind").and_then(Json::as_str) {
                    Some("flag") => if (t + k) % 3 == 0 { ev.push((id.clone(), Json::Bool(true))); },
                    Some("number") => ev.push((id.clone(), Json::Num([0.0, 0.5, 1.0][(t + k) % 3]))),
                    _ => { let l = strs(x.get("levels")); if !l.is_empty() { ev.push((id.clone(), Json::Str(l[(t + k) % l.len()].clone()))); } } }
            }
            let (_, line) = lv.event(&Json::Obj(ev), &run).unwrap(); text += &line; text.push('\n');
        }
        text
    }

    /// A strand with checkpoint lines (docs/persona.md §5.7): the first poll of `--follow` / `--serve` starts at the last checkpoint
    /// and draws the event the full replay draws; after a pause line the frame says `paused` next to the badge
    #[test]
    fn following_starts_at_the_last_checkpoint() {
        let (p, doc) = persona::load(concat!(env!("CARGO_MANIFEST_DIR"), "/../examples/persona/tutor.yaml")).unwrap();
        let (mut lv, header) = Live::start(p.clone(), &doc, persona::init(&p, Some(5), true, &run), Clock::Fixed, "probbit test");
        let mut text = format!("{header}\n");
        for t in 0..25u32 { let ev = json::parse(&format!(r#"{{"praise":{},"elapsed_hours":1}}"#, t % 2 == 0)).unwrap();
            let (s, line) = lv.event(&ev, &run).unwrap(); text += &line; text.push('\n'); if lv.n % 10 == 0 { text += &lv.checkpoint(&s); text.push('\n'); } }
        let f = std::env::temp_dir().join(format!("probbit-monitor-cp-{}.strand", std::process::id())); std::fs::write(&f, &text).unwrap();
        let mut fl = Followed::new(f.to_str().unwrap()); let mut seen = vec![];
        assert_eq!(fl.poll(&run, &mut |w: &Watch| seen.push(w.rep.live.n)), Some(false));
        let w = fl.w.as_ref().unwrap(); assert_eq!((w.rep.from, w.rep.live.n, seen.first().copied()), (Some(20), 25, Some(20)));
        let lines = live::lines(&text); let mut full = Watch::open(lines[0]).unwrap(); for l in &lines[1..] { full.feed(l, &run); }
        assert!(full.bad.is_none()); assert_eq!(persona::canon(&w.last.as_ref().unwrap().doc), persona::canon(&full.last.as_ref().unwrap().doc));
        text += &lv.control("pause", "human:owner", "a check", "t").unwrap(); text.push('\n'); std::fs::write(&f, &text).unwrap();
        assert_eq!(fl.poll(&run, &mut |_: &Watch| {}), Some(false));
        let fr = frame(fl.w.as_ref(), None, "x", None, &Style { th: None, ascii: true, cols: usize::MAX }); assert!(fr[0].contains("paused"), "{}", fr[0]);
        let page = frame_json(fl.w.as_ref(), None, None);
        assert_eq!(page.get("status").and_then(Json::as_str), Some("paused"));
        assert_eq!(page.get("from_checkpoint").and_then(Json::as_f64), Some(20.0));
        let _ = std::fs::remove_file(&f);
    }

    /// RFC 0009: before 0.14.0 a persona with an expectation block replays a strand with checkpoint lines from its header (the
    /// checkpoint does not carry the expectation's memory): `--once` and the first poll of `--follow` / `--serve` draw the latest
    /// event with no checkpoint start, and lines appended after it replay from the state in memory, one by one. From 0.14.0 the
    /// checkpoint carries the memory and the same life starts at its last checkpoint.
    #[test]
    fn an_expectation_persona_replays_from_the_header() {
        let (p, doc) = persona::load(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/monitor/tutor-expect.yaml")).unwrap();
        assert!(p.expect.is_some(), "the fixture has an expectation block");
        {   let (mut lv, header) = Live::start(p.clone(), &doc, persona::init(&p, Some(2), true, &run), Clock::Fixed, "probbit 0.14.0"); let mut text = format!("{header}\n");
            for t in 0..17u32 { let (s, line) = lv.event(&json::parse(&format!(r#"{{"praise":{},"elapsed_hours":1}}"#, t % 3 == 0)).unwrap(), &run).unwrap();
                text += &line; text.push('\n'); if lv.n % 5 == 0 { text += &lv.checkpoint(&s); text.push('\n'); } }
            let lines = live::lines(&text); let k = live::last_checkpoint(&lines, 0).unwrap(); assert_eq!(Watch::start(&lines), Some(k), "0.14.0: the checkpoint start");
            let mut w = Watch::open_at(lines[0], lines[k - 1], lines[k], k + 1).unwrap(); for l in &lines[k + 1..] { w.feed(l, &run); }
            let mut full = Watch::open(lines[0]).unwrap(); for l in &lines[1..] { full.feed(l, &run); }
            assert!(w.bad.is_none(), "{:?}", w.bad); assert_eq!(persona::canon(&w.last.as_ref().unwrap().doc), persona::canon(&full.last.as_ref().unwrap().doc)); }
        let (mut lv, header) = Live::start(p.clone(), &doc, persona::init(&p, Some(2), true, &run), Clock::Fixed, "probbit 0.13.0");
        let mut text = format!("{header}\n");
        let add = |lv: &mut Live, text: &mut String, t: u32| { let ev = json::parse(&format!(r#"{{"praise":{},"elapsed_hours":1}}"#, t % 3 == 0)).unwrap();
            let (s, line) = lv.event(&ev, &run).unwrap(); *text += &line; text.push('\n'); if lv.n % 5 == 0 { *text += &lv.checkpoint(&s); text.push('\n'); } };
        for t in 0..17u32 { add(&mut lv, &mut text, t); }
        let lines = live::lines(&text); let k = live::last_checkpoint(&lines, 0).unwrap();
        assert_eq!((lines.iter().filter(|l| l.starts_with(r#"{"checkpoint":"#)).count(), Watch::start(&lines)), (3, None), "3 checkpoints, no checkpoint start");
        assert!(Replay::open_at(lines[0], lines[k - 1], lines[k], k + 1).is_err_and(|e| e.1.contains("expectation block")), "the shortcut itself still refuses it");
        let f = std::env::temp_dir().join(format!("probbit-monitor-expect-{}.strand", std::process::id())); std::fs::write(&f, &text).unwrap();
        let mut fl = Followed::new(f.to_str().unwrap()); let mut seen = vec![];
        assert_eq!(fl.poll(&run, &mut |w: &Watch| seen.push(w.rep.live.n)), Some(false));
        let w = fl.w.as_ref().unwrap(); assert!(w.bad.is_none(), "{:?}", w.bad);
        assert_eq!((w.rep.from, w.rep.live.n, seen.first().copied(), seen.len()), (None, 17, Some(1), 17), "every event replayed from the header");
        let mut full = Watch::open(lines[0]).unwrap(); for l in &lines[1..] { full.feed(l, &run); }
        assert_eq!(persona::canon(&w.last.as_ref().unwrap().doc), persona::canon(&full.last.as_ref().unwrap().doc));
        assert_eq!(frame_json(fl.w.as_ref(), None, None).get("from_checkpoint"), Some(&Json::Null));
        // appended: only the new lines replay, on the state in memory
        for t in 17..21u32 { add(&mut lv, &mut text, t); } std::fs::write(&f, &text).unwrap();
        seen.clear(); assert_eq!(fl.poll(&run, &mut |w: &Watch| seen.push(w.rep.live.n)), Some(false));
        assert_eq!(seen, vec![18, 19, 20, 21]); assert!(fl.w.as_ref().unwrap().bad.is_none());
        assert_eq!(fl.w.as_ref().unwrap().rep.summary(), live::verify(&text, &run).unwrap(), "ends where verify does");
        let _ = std::fs::remove_file(&f);
        // the tutor without the block takes the shortcut: `following_starts_at_the_last_checkpoint`
    }

    /// Watch a strand line by line: every document's sha256 is its line's `stance` digest, and the replay ends where `verify` does
    fn replays_to_its_digests(text: &str, name: &str) -> Watch {
        let lines = live::lines(text);
        let mut w = Watch::open(lines[0]).unwrap();
        for l in &lines[1..] {
            w.feed(l, &run); assert!(w.bad.is_none(), "{name}: {:?}", w.bad);
            let want = json::parse(l).unwrap().get("stance").and_then(Json::as_str).unwrap().to_string();
            assert_eq!(persona::sha(&w.last.as_ref().unwrap().doc), want, "{name}: line {}", w.rep.line);
        }
        assert_eq!(w.rep.summary(), live::verify(text, &run).unwrap(), "{name}: the monitor's replay ends where verify does");
        w
    }
    /// More strands to check, named in PROBBIT_MONITOR_STRANDS (paths joined as in PATH; unset: nothing to do)
    #[test]
    fn strands_named_in_the_environment_replay_to_their_digests() {
        let Some(v) = std::env::var_os("PROBBIT_MONITOR_STRANDS") else { return };
        for p in std::env::split_paths(&v) { let w = replays_to_its_digests(&std::fs::read_to_string(&p).unwrap(), &p.display().to_string());
            eprintln!("{}: {} events, every document's sha256 is its line's stance digest", p.display(), w.rep.live.n); }
    }

    #[test]
    fn the_monitors_documents_are_the_strands_stances() {
        for name in ["tutor.yaml", "ops-engineer.yaml", "trader-assistant.yaml"] {
            let text = strand(&format!("{}/../examples/persona/{name}", env!("CARGO_MANIFEST_DIR")), 40);
            let w = replays_to_its_digests(&text, name);
            // every frame style draws it; plain ASCII with --plain
            for (ascii, th) in [(true, None), (false, None), (false, Some(Theme { depth: theme::Depth::Ansi256 }))] {
                let fr = frame(Some(&w), None, "x.strand", None, &Style { th, ascii, cols: usize::MAX });
                assert!(fr.len() > 8 && fr[0].contains(&format!("event {} ", w.rep.live.n)), "{name}: {fr:?}");
                if ascii { assert!(fr.iter().all(|l| l.is_ascii()), "{name}: {fr:?}"); }
            }
            // a changed input: the frame names the line and keeps the event before it
            let bent = text.replacen(r#""elapsed_hours":13"#, r#""elapsed_hours":12"#, 1);
            let bl = live::lines(&bent); let mut b = Watch::open(bl[0]).unwrap(); for l in &bl[1..] { b.feed(l, &run); }
            let (line, _) = b.bad.clone().unwrap(); assert_eq!(live::verify(&bent, &run).unwrap_err().0, line, "{name}");
            assert_eq!(b.last.as_ref().map(|f| f.n as usize), Some(line - 2), "{name}: the frame shows the event before the line that differs");
            assert!(frame(Some(&b), None, "x", None, &Style { th: None, ascii: true, cols: usize::MAX })[0].contains(&format!("event {} ", line - 2)), "{name}");
        }
    }

    /// Each example persona's scripted week (seed 2, as `probbit live PERSONA --seed 2 --demo week --strand` writes it) replays to
    /// its digests
    #[test]
    fn the_example_personas_demo_weeks_replay_to_their_digests() {
        for name in ["tutor.yaml", "ops-engineer.yaml", "trader-assistant.yaml"] {
            let doc = persona::parse_doc(&std::fs::read_to_string(format!("{}/../examples/persona/{name}", env!("CARGO_MANIFEST_DIR"))).unwrap(), false).unwrap();
            let f = std::env::temp_dir().join(format!("probbit-monitor-week-{}-{name}.strand", std::process::id()));
            let fp = f.to_string_lossy().to_string(); let _ = std::fs::remove_file(&f);
            assert!(live::week(&doc, Some(2), Some(&fp), None, &mut |_: &str| {}, &run).is_ok(), "{name}: the demo week");
            let text = std::fs::read_to_string(&f).unwrap(); let _ = std::fs::remove_file(&f);
            let w = replays_to_its_digests(&text, name); assert!(w.rep.live.n >= 40, "{name}: {} events", w.rep.live.n);
        }
    }

    /// A 0.8.0 document with `pursue` and `drives` (hand-written) draws the drives rows; a document without them draws none
    /// §2.18: the header shows the document's glyph next to the persona's name (not in plain ASCII); the page's frame carries it
    /// in `doc`; a document without one draws the header as before
    #[test]
    fn the_header_shows_the_glyph() {
        let text = strand(&format!("{}/../plugins/openclaw/personas/assistant.yaml", env!("CARGO_MANIFEST_DIR")), 3);
        let lines = live::lines(&text); let mut w = Watch::open(lines[0]).unwrap(); for l in &lines[1..] { w.feed(l, &run); }
        let g = w.last.as_ref().unwrap().doc.get("glyph").and_then(Json::as_str).unwrap().to_string();
        let uni = frame(Some(&w), None, "x", None, &Style { th: None, ascii: false, cols: usize::MAX });
        assert!(uni[0].starts_with(&format!("probbit monitor Assistant 1.0.0 {g} · seed ")), "{}", uni[0]);
        let why = w.last.as_ref().unwrap().doc.get("glyph_why").and_then(Json::as_str).unwrap().to_string();
        assert_eq!(uni[1], format!("GLYPH   {g} {why}"), "the glyph's why under the header");
        let plain = frame(Some(&w), None, "x", None, &Style { th: None, ascii: true, cols: usize::MAX });
        assert!(plain[0].starts_with("probbit monitor Assistant 1.0.0 | seed "), "{}", plain[0]);
        assert_eq!(plain[1], format!("GLYPH   {why}"), "plain ASCII: the why without the glyph");
        assert_eq!(frame_json(Some(&w), None, None).get("doc").and_then(|d| d.get("glyph")).and_then(Json::as_str), Some(g.as_str()));
        let Json::Obj(mut kv) = w.last.as_ref().unwrap().doc.clone() else { panic!() }; kv.retain(|(k, _)| k != "glyph" && k != "glyph_why");
        w.last.as_mut().unwrap().doc = Json::Obj(kv);
        let bare = frame(Some(&w), None, "x", None, &Style { th: None, ascii: false, cols: usize::MAX });
        assert!(bare[0].starts_with("probbit monitor Assistant 1.0.0 · seed ") && bare[1].starts_with("LEGEND"), "{}", bare[..2].join("\n"));
    }

    /// The face (the page's header): its mood meters are the two moods `display.glyphs` names, else the persona's first two
    /// (one for a persona with one mood, none without moods); `meta.face` carries them
    #[test]
    fn the_face_names_its_mood_meters() {
        let face_of = |path: &str| { let text = strand(path, 1); let w = Watch::open(live::lines(&text)[0]).unwrap();
            let ids = strs(meta_json(Some(&w), "x").get("face")); assert_eq!(ids, w.meta.face); ids };
        let dir = env!("CARGO_MANIFEST_DIR");
        assert_eq!(face_of(&format!("{dir}/../plugins/openclaw/personas/assistant.yaml")), ["valence", "pressure"]);
        assert_eq!(face_of(&format!("{dir}/../examples/persona/tutor.yaml")), ["valence", "arousal"], "no display block: the first two moods");
        let doc = json::parse(r#"{"display":{"glyphs":{"moods":{"b":["x"],"a":["y"]}}}}"#).unwrap();
        assert_eq!(face(&doc, &["a".into(), "b".into(), "c".into()]), ["b", "a"], "the display's order");
        assert_eq!(face(&Json::Null, &["a".into()]), ["a"]);
        assert!(face(&Json::Null, &[]).is_empty());
    }

    /// RFC 0009 for the page: `settle` is the frame's `trajectory` by level name, per trait and mood in the persona's order, the
    /// betas rising to 1; a strand without `engine.trajectory` has neither key
    #[test]
    fn settle_is_the_trajectory_by_level() {
        let yaml = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../plugins/openclaw/personas/assistant.yaml")).unwrap()
            .replace("\r\n", "\n").replace("engine:\n  on_conflict: yield\n", "expectation: {}\n\nengine:\n  on_conflict: yield\n  trajectory: {k: 5, beta0: 0.3}\n");
        let f = std::env::temp_dir().join(format!("probbit-monitor-settle-{}.yaml", std::process::id())); std::fs::write(&f, yaml).unwrap();
        let text = strand(f.to_str().unwrap(), 3); let _ = std::fs::remove_file(&f);
        let lines = live::lines(&text); let mut w = Watch::open(lines[0]).unwrap(); for l in &lines[1..] { w.feed(l, &run); }
        let fr = frame_json(Some(&w), None, None);
        let (st, tj) = (fr.get("settle").unwrap(), fr.get("trajectory").unwrap());
        let betas: Vec<f64> = st.get("betas").and_then(Json::as_arr).unwrap().iter().filter_map(Json::as_f64).collect();
        assert_eq!((betas.len(), betas[0], betas[4]), (5, 0.3, 1.0));
        assert!(betas.windows(2).all(|b| b[0] < b[1]) && st.get("h").and_then(Json::as_arr).map(<[Json]>::len) == Some(5));
        let vars = st.get("vars").and_then(Json::as_arr).unwrap();
        let ids: Vec<&str> = vars.iter().filter_map(|v| v.get("id").and_then(Json::as_str)).collect();
        let want: Vec<&str> = w.meta.traits.iter().chain(&w.meta.moods).map(|v| v.id.as_str()).collect();
        assert_eq!(ids, want);
        let steps = tj.get("steps").and_then(Json::as_arr).unwrap();
        for v in vars {
            let id = v.get("id").and_then(Json::as_str).unwrap();
            let odds = v.get("odds").and_then(Json::as_arr).unwrap(); assert_eq!(odds.len(), 5);
            for (k, row) in odds.iter().enumerate() { let row = row.as_arr().unwrap();
                assert!((row.iter().filter_map(Json::as_f64).sum::<f64>() - 1.0).abs() < 1e-5, "{id} step {k}");
                for (i, x) in row.iter().enumerate() {
                    let exact = steps[k].get("marginals").and_then(|m| m.get(id)).and_then(|m| m.get(&format!("l{i}"))).and_then(Json::as_f64).unwrap();
                    assert_eq!(x.as_f64(), Some(persona::r6(exact)), "{id} step {k} level {i}"); } }
        }
        let caution = vars.iter().find(|v| v.get("id").and_then(Json::as_str) == Some("caution")).unwrap();
        assert_eq!(strs(caution.get("levels")), ["bold", "measured", "careful"]);
        let plain = strand(concat!(env!("CARGO_MANIFEST_DIR"), "/../examples/persona/tutor.yaml"), 2);
        let lines = live::lines(&plain); let mut w = Watch::open(lines[0]).unwrap(); for l in &lines[1..] { w.feed(l, &run); }
        let fr = frame_json(Some(&w), None, None); assert!(fr.get("settle").is_none() && fr.get("trajectory").is_none());
    }

    #[test]
    fn drives_are_drawn_when_the_document_carries_them() {
        let text = strand(&format!("{}/../examples/persona/tutor.yaml", env!("CARGO_MANIFEST_DIR")), 3);
        let lines = live::lines(&text); let mut w = Watch::open(lines[0]).unwrap(); for l in &lines[1..] { w.feed(l, &run); }
        let s = Style { th: None, ascii: true, cols: usize::MAX };
        assert!(!frame(Some(&w), None, "x", None, &s).iter().any(|l| l.starts_with("DRIVES")));
        let Json::Obj(mut kv) = w.last.as_ref().unwrap().doc.clone() else { panic!("a document") };
        kv.push(("pursue".into(), json::parse(r#"{"goal":"ship","p":0.62,"odds":{"rest":0.38,"ship":0.62},"released":true,"say":"ship the next small piece","lift":{}}"#).unwrap()));
        kv.push(("drives".into(), json::parse(r#"{"want":{"rest":0.2,"ship":1.4},"glow":{"rest":0,"ship":0.9},"expect":{"rest":0.1,"ship":0.5},"surprise":{"rest":0,"ship":1},"deadline":{"rest":null,"ship":6},"since":{"rest":3,"ship":0}}"#).unwrap()));
        w.last.as_mut().unwrap().doc = Json::Obj(kv);
        let fr = frame(Some(&w), None, "x", None, &s);
        let at = fr.iter().position(|l| l.starts_with("DRIVES")).unwrap_or_else(|| panic!("{fr:?}"));
        assert!(fr[at].contains("[ship 0.62]") && fr[at].contains("ship the next small piece"), "{}", fr[at]);
        assert!(fr[at + 2].contains("wanting [") && fr[at + 2].contains("+1.00 above expectation"), "{fr:?}");
        let mut held = w.last.as_ref().unwrap().doc.clone();
        if let Json::Obj(kv) = &mut held { if let Some((_, Json::Obj(p))) = kv.iter_mut().find(|(k, _)| k == "pursue") {
            if let Some((_, released)) = p.iter_mut().find(|(k, _)| k == "released") { *released = Json::Bool(false); }
        } }
        let held = drives(&held, &w.meta, &s, 4, &|_| String::new()).join("\n");
        assert!(held.contains("ship: not released"), "{held}");
        assert!(!held.contains("ship the next small piece") && !held.contains("[ship 0.62]"), "{held}");
        // odd shapes (lists, strings, nulls) draw without a panic
        let Json::Obj(mut kv) = w.last.as_ref().unwrap().doc.clone() else { panic!() };
        kv.retain(|(k, _)| k != "drives"); kv.push(("drives".into(), json::parse(r#"{"want":[1,"x",null],"glow":"?","surprise":[-0.5]}"#).unwrap()));
        w.last.as_mut().unwrap().doc = Json::Obj(kv); let fr = frame(Some(&w), None, "x", None, &s);
        assert!(fr.iter().any(|l| l.contains("-0.50 below expectation")) || fr.iter().any(|l| l.contains("wanting")), "{fr:?}");
    }

    /// A real drives strand (the fixture persona, goal signals in the events, praise so `pursue` learns): it replays to its
    /// digests, the frame draws `pursue` and every goal's wanting and afterglow, and the learned row of `pursue` names the goals
    #[test]
    fn a_drives_strand_lights_the_drives_rows() {
        let (p, doc) = persona::load(&format!("{}/tests/fixtures/persona/drives-adversary.json", env!("CARGO_MANIFEST_DIR"))).unwrap();
        let (mut lv, header) = Live::start(p.clone(), &doc, persona::init(&p, Some(4), true, &run), Clock::Fixed, "probbit test");
        let mut text = format!("{header}\n");
        for e in [r#"{"goals":{"fun":{"cue":true,"win":1.5}},"praise":true}"#, r#"{"goals":{"fun":{"win":1.5}},"elapsed_hours":2}"#,
            r#"{"goals":{"craft":{"progress":0.6}},"praise":true,"elapsed_hours":1}"#, r#"{"goals":{"chores":{"deadline_hours":5}},"security":true}"#] {
            let (_, line) = lv.event(&json::parse(e).unwrap(), &run).unwrap(); text += &line; text.push('\n');
        }
        let w = replays_to_its_digests(&text, "drives");
        assert_eq!(w.meta.goals, ["fun", "craft", "chores", "safety"]);
        let fr = frame(Some(&w), None, "x", None, &Style { th: None, ascii: true, cols: usize::MAX });
        let at = fr.iter().position(|l| l.starts_with("DRIVES")).unwrap_or_else(|| panic!("{fr:?}"));
        assert!(fr[at].contains("[chores 1.00]"), "{}", fr[at]);
        for g in ["fun", "craft", "chores", "safety"] { assert!(fr.iter().any(|l| l.trim_start().starts_with(g) && l.contains("wanting [") && l.contains("afterglow [")), "{g}: {fr:?}"); }
        let learned = fr.iter().find(|l| l.trim_start().starts_with("pursue") && !l.starts_with("DRIVES")).unwrap_or_else(|| panic!("{fr:?}"));
        assert!(["fun ", "craft ", "chores ", "safety "].iter().all(|g| learned.contains(g)), "{learned}");
        assert_eq!(meta_json(Some(&w), "x").get("goals").map(|g| strs(Some(g))), Some(w.meta.goals.clone()));
    }

    /// The drives demo is a real strand, with positive and negative prediction errors and an enforced goal habit.
    #[test]
    fn synthetic_drives_demo_replays_and_exercises_its_signals() {
        let lines = drive_demo(&run); let text = lines.join("\n") + "\n";
        let complete = replays_to_its_digests(&text, "synthetic drives demo");
        assert_eq!(complete.rep.live.n, 8);
        let mut w = Watch::open(&lines[0]).unwrap(); let mut positive = false; let mut negative = false; let mut glowing = false;
        for l in &lines[1..] {
            w.feed(l, &run); let d = &w.last.as_ref().unwrap().doc;
            let values = |key: &str| d.get("drives").and_then(|x| x.get(key)).and_then(Json::as_obj).unwrap().iter().filter_map(|(_, x)| x.as_f64()).collect::<Vec<_>>();
            positive |= values("surprise").iter().any(|x| *x > 0.0); negative |= values("surprise").iter().any(|x| *x < 0.0);
            glowing |= values("glow").iter().any(|x| *x > 0.0);
            if w.rep.live.n == 7 { assert_eq!(d.get("pursue").and_then(|p| p.get("goal")).and_then(Json::as_str), Some("check")); }
        }
        assert!(positive && negative && glowing);
    }

    /// Monitor scales come from this persona, including a non-default prediction-error cap.
    #[test]
    fn drive_metadata_preserves_declared_scales() {
        let doc = json::parse(r#"{"probbit_persona":1,"identity":{"name":"Scale fixture","version":"1"},
            "traits":[{"id":"approach","levels":["consider","act"]}],
            "drives":{"goals":[{"id":"repair"},{"id":"explore"}],"wanting":{"cap":7},
                "afterglow":{"cap":4},"expectation":{"pe_cap":2.5}}}"#).unwrap();
        let p = persona::build(&doc).unwrap();
        let (_, header) = Live::start(p.clone(), &doc, persona::init(&p, Some(2), true, &run), Clock::Fixed, "probbit test");
        let w = Watch::open(&header).unwrap();
        let meta = meta_json(Some(&w), "fixture.strand");
        let caps = meta.get("caps").unwrap();
        assert_eq!(caps.get("wanting").and_then(Json::as_f64), Some(7.0));
        assert_eq!(caps.get("afterglow").and_then(Json::as_f64), Some(4.0));
        assert_eq!(caps.get("prediction_error").and_then(Json::as_f64), Some(2.5));
    }

    /// The page's layout lists the persona's variables in its order; a frame carries the event's document as replayed (drives
    /// fields included, as they are), the moods' recent positions, the line that diverges and the note
    #[test]
    fn the_pages_frames_carry_the_documents() {
        let text = strand(&format!("{}/../examples/persona/tutor.yaml", env!("CARGO_MANIFEST_DIR")), 5);
        let lines = live::lines(&text); let mut w = Watch::open(lines[0]).unwrap(); for l in &lines[1..] { w.feed(l, &run); }
        let m = meta_json(Some(&w), "x.strand");
        let ids: Vec<&str> = m.get("traits").and_then(Json::as_arr).unwrap().iter().filter_map(|t| t.get("id").and_then(Json::as_str)).collect();
        assert_eq!(ids, w.meta.traits.iter().map(|v| v.id.as_str()).collect::<Vec<_>>());
        assert_eq!((m.get("demo"), m.get("persona").and_then(|p| p.get("name"))), (Some(&Json::Bool(false)), Some(&Json::Str("Pip".into()))));
        let f = frame_json(Some(&w), None, None);
        assert_eq!((f.get("n"), f.get("diverges"), f.get("note")), (Some(&Json::Num(5.0)), Some(&Json::Null), Some(&Json::Null)));
        assert_eq!(persona::canon(f.get("doc").unwrap()), persona::canon(&w.last.as_ref().unwrap().doc));
        assert_eq!(f.get("spark").and_then(|s| s.get("valence")).and_then(Json::as_arr).map(<[Json]>::len), Some(5));
        let Json::Obj(mut kv) = w.last.as_ref().unwrap().doc.clone() else { panic!("a document") };
        let dv = json::parse(r#"{"want":{"ship":1.4},"surprise":{"ship":-0.5}}"#).unwrap(); kv.push(("drives".into(), dv.clone()));
        w.last.as_mut().unwrap().doc = Json::Obj(kv);
        assert_eq!(frame_json(Some(&w), None, None).get("doc").and_then(|d| d.get("drives")), Some(&dv));
        let bad = frame_json(Some(&w), Some(&(3, "the stance differs".into())), Some("a note"));
        assert_eq!((bad.get("diverges").and_then(|d| d.get("line")), bad.get("note")), (Some(&Json::Num(3.0)), Some(&Json::Str("a note".into()))));
        assert!(meta_json(None, "x").get("persona").is_none() && frame_json(None, None, None).get("doc") == Some(&Json::Null));
    }

    /// A long replay is published on the way (every `every`, with a note), then at its end without the note; a short one once
    #[test]
    fn a_long_replay_is_published_on_the_way() {
        let text = strand(&format!("{}/../examples/persona/tutor.yaml", env!("CARGO_MANIFEST_DIR")), 5);
        let f = std::env::temp_dir().join(format!("probbit-monitor-step-{}.strand", std::process::id()));
        std::fs::write(&f, &text).unwrap(); let path = f.to_string_lossy().to_string();
        for (every, publishes) in [(Duration::ZERO, 6), (Duration::from_secs(3600), 1)] {
            let hub: Hub = Arc::new((Mutex::new(Board::default()), Condvar::new()));
            let (mut fl, mut fresh) = (Followed::new(&path), true);
            step(&mut fl, &path, &hub, &run, &mut fresh, every);
            let b = board(&hub);
            assert_eq!((b.seq, b.docs.len(), b.last, b.layout), (publishes, 5, 5, 1), "{every:?}");
            assert_eq!(json::parse(&b.frame).unwrap().get("note"), Some(&Json::Null), "{every:?}: the last frame has no note");
        }
        let _ = std::fs::remove_file(&f);
    }

    /// Read a stream until `what` is in what came after byte `from`
    fn read_until(s: &mut TcpStream, got: &mut String, from: usize, what: &str) {
        let t = std::time::Instant::now();
        while !got[from..].contains(what) {
            assert!(t.elapsed() < Duration::from_secs(10), "no {what:?} within 10 s: {got}");
            let mut b = [0u8; 4096]; let k = s.read(&mut b).unwrap(); assert!(k > 0, "the stream closed: {got}");
            got.push_str(&String::from_utf8_lossy(&b[..k]));
        }
    }
    /// `/events` on a quiet board: a heartbeat comment after each quiet `beat` (15 s when served; 100 ms here), before the
    /// strand has replayed and after; a frame published meanwhile is pushed at once, the layout before it
    #[test]
    fn a_quiet_event_stream_carries_heartbeats() {
        let hub: Hub = Arc::new((Mutex::new(Board::default()), Condvar::new()));
        let l = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap(); let port = l.local_addr().unwrap().port();
        let h = hub.clone();
        std::thread::spawn(move || { if let Ok((c, _)) = l.accept() { events(c, &h, Duration::from_millis(100)); } });
        let mut s = TcpStream::connect((Ipv4Addr::LOCALHOST, port)).unwrap(); s.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
        let mut got = String::new();
        read_until(&mut s, &mut got, 0, ": heartbeat\n\n");
        assert!(got.starts_with("HTTP/1.1 200 OK\r\n") && got.contains("text/event-stream") && !got.contains("event:"), "{got}");
        let at = got.len();
        publish(&hub, &json::obj(vec![("path", json::str("x"))]), &json::obj(vec![("n", Json::Num(1.0))]), vec![], false);
        read_until(&mut s, &mut got, at, "event: meta\ndata: {\"path\":\"x\"}\n\nevent: frame\ndata: {\"n\":1}\n\n");
        let at = got.len(); read_until(&mut s, &mut got, at, ": heartbeat\n\n");
        assert!(!got[at..].contains("event:"), "nothing new, nothing but heartbeats: {got}");
    }

    /// The panel on strands named in PROBBIT_PANEL_STRANDS (paths joined as in PATH; unset: nothing to do), docs/persona.md §5.8:
    /// a field's words change exactly when its (level, size, direction) buckets change; a twin with the same header and empty
    /// events (no inputs, no clock, no feedback) changes no word after its second event; every `why` naming a trait is counted
    /// against a word change on that trait (hits and misses, reported, not a bar); the time per event to read, word and
    /// serialize the page's frame. One `PANEL-STRAND {json}` line per strand on stderr.
    #[test]
    fn panel_words_on_strands_named_in_the_environment() {
        let Some(v) = std::env::var_os("PROBBIT_PANEL_STRANDS") else { return };
        for p in std::env::split_paths(&v) {
            let text = std::fs::read_to_string(&p).unwrap(); let lines = live::lines(&text);
            let mut w = Watch::open(lines[0]).unwrap();
            let (mut evs, mut slow, mut slow_all): (Vec<(Vec<panel::Word>, String)>, f64, f64) = (vec![], 0.0, 0.0);
            for l in &lines[1..] {
                let (n, t0) = (w.rep.live.n, std::time::Instant::now()); w.feed(l, &run); assert!(w.bad.is_none(), "{}: {:?}", p.display(), w.bad);
                if w.rep.live.n == n { continue; }
                let all = t0.elapsed().as_secs_f64() * 1e3;
                // the panel's own cost: readings, words and the page frame's bytes for this event
                let t1 = std::time::Instant::now(); let f = w.last.as_ref().unwrap();
                let st = w.rep.live.st.to_json(&w.rep.live.p); let open = persona::open_decisions(&w.rep.live.p, &w.rep.live.st).map(|o| o.len());
                let rd = readings(&w.meta, &f.doc, &f.learned, st.get("dispositions"), open); let prev = evs.len();
                let _ = panel::words(&rd, &rd); let bytes = persona::canon(&frame_json(Some(&w), None, None)).len(); assert!(bytes > 0 && prev == evs.len());
                let ms = t1.elapsed().as_secs_f64() * 1e3; slow = slow.max(ms); slow_all = slow_all.max(all + ms);
                evs.push((f.words.clone(), f.doc.get("why").and_then(Json::as_str).unwrap_or("").to_string()));
            }
            let key = |x: &panel::Word| (x.level.clone(), x.size, x.dir.clone());
            let (mut changes, mut mismatch, mut flaps, mut hits, mut misses, mut fields) = (0, 0, 0, 0, 0, std::collections::BTreeMap::<String, usize>::new());
            let ids: Vec<String> = w.meta.traits.iter().chain(&w.meta.moods).map(|v| v.id.clone()).collect();
            for i in 1..evs.len() {
                for (k, now) in evs[i].0.iter().enumerate() {
                    let before = &evs[i - 1].0[k]; assert_eq!(before.key, now.key);
                    let (wc, kc) = (before.text != now.text, key(before) != key(now));
                    if wc != kc { mismatch += 1; }
                    if wc { changes += 1; *fields.entry(now.key.clone()).or_default() += 1; }
                    if i >= 2 && wc && evs[i - 2].0[k].text == now.text { flaps += 1; }
                }
                let why = &evs[i].1; let named: Vec<&String> = ids.iter().filter(|id| why.split(|c: char| !(c.is_alphanumeric() || c == '_')).any(|t| t == id.as_str())).collect();
                for id in named {
                    let moved = evs[i].0.iter().zip(&evs[i - 1].0).any(|(a, b)| [format!("stance:{id}"), format!("mood:{id}"), format!("learned:{id}")].contains(&a.key) && a.text != b.text);
                    if moved { hits += 1 } else { misses += 1 }
                }
            }
            // the twin: the same individual, the same number of events, each event {}
            let mut maker = Replay::open(lines[0]).unwrap(); let mut twin = Watch::open(lines[0]).unwrap(); let mut tw: Vec<Vec<String>> = vec![];
            for _ in 0..evs.len() { let (_, line) = maker.live.event(&Json::Obj(vec![]), &run).unwrap(); twin.feed(&line, &run); assert!(twin.bad.is_none());
                tw.push(twin.last.as_ref().unwrap().words.iter().map(|x| x.text.clone()).collect()); }
            let diff = |a: &Vec<String>, b: &Vec<String>| a.iter().zip(b).filter(|(x, y)| x != y).count();
            let twin_first = if tw.len() > 1 { diff(&tw[0], &tw[1]) } else { 0 };
            let twin_after: usize = (2..tw.len()).map(|i| diff(&tw[i - 1], &tw[i])).sum();
            eprintln!("PANEL-STRAND {}", persona::canon(&json::obj(vec![("strand", json::str(&p.display().to_string())), ("events", json::num(evs.len() as f64)),
                ("fields", json::num(evs.first().map_or(0, |e| e.0.len()) as f64)), ("word_changes", json::num(changes as f64)), ("mismatch", json::num(mismatch as f64)),
                ("flaps", json::num(flaps as f64)), ("why_hits", json::num(hits as f64)), ("why_misses", json::num(misses as f64)),
                ("twin_changes_event2", json::num(twin_first as f64)), ("twin_changes_after", json::num(twin_after as f64)),
                ("panel_ms_max", json::num((slow * 1000.0).round() / 1000.0)), ("event_ms_max_with_replay", json::num((slow_all * 1000.0).round() / 1000.0)),
                ("changes_by_field", Json::Obj(fields.into_iter().map(|(k, n)| (k, json::num(n as f64))).collect()))])));
            assert_eq!(mismatch, 0, "{}: words changed without a bucket change, or the reverse", p.display());
            assert_eq!(twin_after, 0, "{}: the twin with empty events changed words", p.display());
        }
    }

    /// The tutor `--demo` embeds is examples/persona/tutor.yaml
    #[test]
    fn the_demo_plays_the_example_tutor() {
        let ex = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../examples/persona/tutor.yaml")).unwrap();
        assert_eq!(TUTOR.replace("\r\n", "\n"), ex.replace("\r\n", "\n"), "copy examples/persona/tutor.yaml to probbit-cli/src/demo-tutor.yaml");
    }

    #[test]
    fn cells_sum_to_the_bar() {
        for ps in [vec![0.37, 0.56, 0.07], vec![1.0, 0.0, 0.0], vec![0.333334, 0.333333, 0.333333], vec![0.0, 0.0], vec![0.05, 0.05, 0.05, 0.85]] {
            let c = cells(&ps, BAR); assert!(c.iter().sum::<usize>() == BAR || ps.iter().sum::<f64>() == 0.0, "{ps:?} {c:?}");
        }
        assert_eq!(cells(&[0.5, 0.5], 5), vec![3, 2]);
    }
}
