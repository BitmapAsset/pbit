//! A read-only explanation of one strand event. Replay owns all verification and
//! turn semantics; this module selects an event and renders the existing documents.
use crate::json::{self, num, obj, str as text, InErr, Json};
use crate::{persona, persona_engine};
use probbit_persona::strand::{self, Kind, Replay};

enum Error {
    Input(InErr),
    Replay(usize, String),
}

fn field(j: &Json, name: &str) -> Json {
    j.get(name).cloned().unwrap_or(Json::Null)
}

fn read_line(line: &str, at: usize) -> Result<Json, Error> {
    let j =
        json::parse(line).map_err(|e| Error::Input(json::schema(&format!("line {at}"), e.msg)))?;
    if j.as_obj().is_none() {
        return Err(Error::Input(json::schema(
            &format!("line {at}"),
            "a strand line must be an object",
        )));
    }
    Ok(j)
}

fn replay_error((line, why): (usize, String)) -> Error {
    Error::Replay(line, why)
}

/// Initial/resting levels are recorded, but no preceding turn has been solved.
/// In particular, priors are not the missing previous turn's marginal odds.
fn initial_stance(r: &Replay) -> Json {
    let state = r.live.st.to_json(&r.live.p);
    let prev = field(&state, "prev");
    let variables = |key: &str| {
        Json::Obj(
            r.doc
                .get(key)
                .and_then(Json::as_arr)
                .unwrap_or(&[])
                .iter()
                .filter_map(|v| {
                    let id = v.get("id")?.as_str()?;
                    Some((
                        id.to_owned(),
                        obj(vec![("level", field(&prev, id)), ("odds", Json::Null)]),
                    ))
                })
                .collect(),
        )
    };
    obj(vec![
        ("status", text("initial_state")),
        ("stance", variables("traits")),
        ("mood", variables("moods")),
        ("agenda", field(&prev, "agenda")),
    ])
}

fn bound_habits(doc: &Json, after: &Json) -> Json {
    let ids = after
        .get("habits")
        .and_then(|h| h.get("bound"))
        .and_then(Json::as_arr)
        .unwrap_or(&[]);
    let definitions = doc.get("habits").and_then(Json::as_arr).unwrap_or(&[]);
    Json::Arr(
        ids.iter()
            .filter_map(|id| {
                let h = definitions.iter().find(|h| h.get("id") == Some(id))?;
                let held = h
                    .get("then")
                    .and_then(Json::as_obj)
                    .unwrap_or(&[])
                    .iter()
                    .map(|(name, _)| {
                        let value = after
                            .get("stance")
                            .and_then(|s| s.get(name))
                            .or_else(|| after.get("mood").and_then(|s| s.get(name)))
                            .cloned()
                            .unwrap_or(Json::Null);
                        (name.clone(), value)
                    })
                    .collect();
                Some(obj(vec![
                    ("id", id.clone()),
                    ("when", field(h, "when")),
                    ("then", field(h, "then")),
                    ("rules", field(h, "rules")),
                    ("say", field(h, "say")),
                    ("held", Json::Obj(held)),
                ]))
            })
            .collect(),
    )
}

fn flags(inputs: &Json) -> Json {
    let mut flags = Vec::new();
    for (key, value) in inputs.as_obj().unwrap_or(&[]) {
        if matches!(value, Json::Bool(_)) {
            flags.push((key.clone(), value.clone()));
        }
        if key == "senses" {
            for (id, reading) in value.as_obj().unwrap_or(&[]) {
                let raw = reading.get("v").unwrap_or(reading);
                if matches!(raw, Json::Bool(_)) {
                    flags.push((format!("senses.{id}"), raw.clone()));
                }
            }
        }
    }
    Json::Obj(flags)
}

// Check only the event envelope; all engine-owned fields and turn semantics
// are still interpreted by Replay. Unknown fields must be handled there first.
fn event_inputs(logged: &Json, at: usize) -> Result<&Json, Error> {
    let inputs = logged
        .get("inputs")
        .filter(|v| v.as_obj().is_some())
        .ok_or_else(|| {
            Error::Input(json::schema(
                &format!("line {}.inputs", at),
                "missing or invalid inputs object",
            ))
        })?;
    for key in ["prev", "stance", "state"] {
        if logged.get(key).and_then(Json::as_str).is_none() {
            return Err(Error::Input(json::schema(
                &format!("line {}.{key}", at),
                "missing or invalid digest string",
            )));
        }
    }
    let n = logged.get("n").ok_or_else(|| {
        Error::Input(json::schema(
            &format!("line {}.n", at),
            "missing event number",
        ))
    })?;
    json::count_u64(n, &format!("line {}.n", at)).map_err(Error::Input)?;
    Ok(inputs)
}

fn report(source: &str, event: u64, eng: persona::Engine) -> Result<(Json, i32), Error> {
    let lines = strand::lines(source);
    let header = read_line(lines.first().copied().unwrap_or(""), 1)?;
    if header.get("probbit_strand").and_then(Json::as_f64) != Some(strand::FORMAT) {
        return Err(Error::Input(json::schema(
            "line 1",
            "not a probbit strand (format 1)",
        )));
    }
    let (mut replay, start) = Replay::open_lines(&lines).map_err(replay_error)?;
    for i in start..lines.len() {
        let logged = read_line(lines[i], i + 1)?;
        // Select by the next sequential event number, so a forged n is detected
        // by Replay rather than hiding the event as an out-of-range request.
        if matches!(strand::kind(&logged), Kind::Event) && replay.live.n + 1 == event {
            let before_source = if replay.last_stance().is_some() {
                "previous_event"
            } else {
                "initial_state"
            };
            let before = replay
                .last_stance()
                .cloned()
                .unwrap_or_else(|| initial_stance(&replay));
            let result = replay.step_at(&lines, i, eng);
            // Unknown line kinds are classified as Event by kind(); Replay must
            // get the first chance to reject them as unsupported (exit 5).
            if let Err((at, why)) = &result {
                if strand::unsupported_field(why).is_some() {
                    return Err(Error::Replay(*at, why.clone()));
                }
            }
            let inputs = event_inputs(&logged, i + 1)?;
            let (after, verdict, reason, failed_line, code) = match result {
                Ok(Some(stance)) => (stance, "recomputes", Json::Null, Json::Null, 0),
                Ok(None) => unreachable!("selected an event"),
                Err((at, why)) => {
                    if strand::unsupported_field(&why).is_some() {
                        return Err(Error::Replay(at, why));
                    }
                    // Replay is spent after an error. Rebuild only the already
                    // checked prefix, then compute the candidate through Live.
                    // It is labelled recomputed, never passed off as logged data.
                    let (mut candidate, from) = Replay::open_lines(&lines).map_err(replay_error)?;
                    for k in from..i {
                        candidate.step_at(&lines, k, eng).map_err(replay_error)?;
                    }
                    let (stance, _) = candidate.live.event(inputs, eng).map_err(|e| {
                        if matches!(e.code, "paused" | "retired") {
                            return Error::Replay(at, why.clone());
                        }
                        Error::Input(json::schema(
                            &format!("line {}.inputs.{}", i + 1, e.path),
                            e.msg,
                        ))
                    })?;
                    (stance, "differs", text(&why), num(at as f64), 1)
                }
            };
            return Ok((
                obj(vec![
                    ("event", logged.clone()),
                    ("requested_event", num(event as f64)),
                    ("line", num((i + 1) as f64)),
                    (
                        "time",
                        obj(vec![
                            ("elapsed_hours", field(inputs, "elapsed_hours")),
                            ("wall_time", Json::Null),
                        ]),
                    ),
                    ("flags", flags(inputs)),
                    ("before_source", text(before_source)),
                    ("before", before),
                    ("bound_habits", bound_habits(&replay.doc, &after)),
                    ("after", after),
                    ("after_source", text("recomputed")),
                    (
                        "chain",
                        obj(vec![
                            ("previous", field(&logged, "prev")),
                            ("expected_previous", text(&persona::digest_of(lines[i - 1]))),
                            ("this", text(&persona::digest_of(lines[i]))),
                        ]),
                    ),
                    ("result", text(verdict)),
                    (
                        "verification",
                        obj(vec![
                            ("ok", Json::Bool(code == 0)),
                            (
                                "scope",
                                text("prefix through selected event; later lines not checked"),
                            ),
                            ("reason", reason),
                            ("failed_line", failed_line),
                            ("engine", field(&header, "engine")),
                            ("verified_by", text(&strand::engine())),
                            (
                                "trusted_link",
                                header
                                    .get("segment")
                                    .map(|s| field(s, "sealed"))
                                    .unwrap_or(Json::Null),
                            ),
                        ]),
                    ),
                ]),
                code,
            ));
        }
        let result = replay.step_at(&lines, i, eng);
        if let Err((at, why)) = &result {
            if strand::unsupported_field(why).is_some() {
                return Err(Error::Replay(*at, why.clone()));
            }
        }
        if matches!(strand::kind(&logged), Kind::Event) {
            event_inputs(&logged, i + 1)?;
        }
        result.map_err(replay_error)?;
    }
    replay.end().map_err(replay_error)?;
    Err(Error::Input(json::value(
        "--event",
        format!(
            "event {event} is out of range for this strand (last event {})",
            replay.live.n
        ),
    )))
}

fn render_stance(out: &mut String, title: &str, stance: &Json) {
    out.push_str(&format!("{title}:\n"));
    for key in ["stance", "mood"] {
        for (id, value) in stance.get(key).and_then(Json::as_obj).unwrap_or(&[]) {
            let odds = match value.get("odds") {
                Some(Json::Obj(v)) => v
                    .iter()
                    .map(|(level, p)| format!("{level}={}", persona::canon(p)))
                    .collect::<Vec<_>>()
                    .join(", "),
                _ => "unavailable (no preceding event was solved)".into(),
            };
            out.push_str(&format!(
                "  {id}: {}; odds {odds}\n",
                value
                    .get("level")
                    .and_then(Json::as_str)
                    .unwrap_or("unavailable")
            ));
        }
    }
    if let Some(agenda) = stance.get("agenda").filter(|x| !x.is_null()) {
        out.push_str(&format!("  Agenda: {}\n", persona::canon(agenda)));
    }
    if let Some(line) = stance.get("line").and_then(Json::as_str) {
        out.push_str(&format!("  {line}\n"));
    }
    if let Some(status) = stance.get("status").and_then(Json::as_str) {
        out.push_str(&format!("  Status: {status}\n"));
    }
}

// Data cannot introduce terminal controls or extra report lines. JSON output
// keeps the original values, escaped by the existing canonical JSON writer.
fn display_string(s: &str) -> String {
    s.chars().filter(|c| !c.is_control()).collect()
}

fn display_data(j: &Json) -> Json {
    match j {
        Json::Str(s) => Json::Str(display_string(s)),
        Json::Arr(a) => Json::Arr(a.iter().map(display_data).collect()),
        Json::Obj(o) => Json::Obj(
            o.iter()
                .map(|(k, v)| (display_string(k), display_data(v)))
                .collect(),
        ),
        _ => j.clone(),
    }
}

fn render(d: &Json) -> String {
    let safe = display_data(d);
    let d = &safe;
    let event = field(d, "event");
    let time = field(d, "time");
    let mut out = format!("Event {} (strand line {})\nTime: +{} hours; wall timestamp not recorded.\nInputs: {}\nFlags: {}\n",
        persona::canon(&field(d, "requested_event")), persona::canon(&field(d, "line")),
        persona::canon(&field(&time, "elapsed_hours")), persona::canon(&field(&event, "inputs")), persona::canon(&field(d, "flags")));
    render_stance(&mut out, "Before", &field(d, "before"));
    let after = field(d, "after");
    render_stance(&mut out, "After (recomputed from logged inputs)", &after);
    let habits = field(&after, "habits");
    out.push_str(&format!(
        "Active habits: {}\n",
        persona::canon(&field(&habits, "active"))
    ));
    let bound = d.get("bound_habits").and_then(Json::as_arr).unwrap_or(&[]);
    if bound.is_empty() {
        out.push_str("Binding habits: none (no habit-free twin violation reported).\n");
    }
    for h in bound {
        out.push_str(&format!(
            "Binding habit {}:\n",
            h.get("id").and_then(Json::as_str).unwrap_or("")
        ));
        for (id, value) in h.get("held").and_then(Json::as_obj).unwrap_or(&[]) {
            out.push_str(&format!(
                "  Held {id} at {}; allowed levels: {}.\n",
                value
                    .get("level")
                    .and_then(Json::as_str)
                    .unwrap_or("unavailable"),
                persona::canon(&h.get("then").map(|t| field(t, id)).unwrap_or(Json::Null))
            ));
        }
        if let Some(rules) = h.get("rules").filter(|r| !r.is_null()) {
            out.push_str(&format!(
                "  Constrained the trait levels or agenda shown above with: {}\n",
                persona::canon(rules)
            ));
        }
        if let Some(say) = h.get("say").and_then(Json::as_str) {
            out.push_str(&format!("  Instruction: {say}\n"));
        }
    }
    out.push_str(&format!(
        "Yielded habits: {}; conflicts: {}\n",
        persona::canon(&field(&habits, "yielded")),
        persona::canon(&field(&habits, "conflict"))
    ));
    let chain = field(d, "chain");
    out.push_str(&format!(
        "Chain: {} -> {}\n",
        chain.get("previous").and_then(Json::as_str).unwrap_or(""),
        chain.get("this").and_then(Json::as_str).unwrap_or("")
    ));
    if chain.get("previous") != chain.get("expected_previous") {
        out.push_str(&format!(
            "Expected previous hash: {}\n",
            chain
                .get("expected_previous")
                .and_then(Json::as_str)
                .unwrap_or("")
        ));
    }
    let verification = field(d, "verification");
    out.push_str(&format!(
        "Recomputation: {}",
        d.get("result").and_then(Json::as_str).unwrap_or("")
    ));
    if let Some(reason) = verification.get("reason").and_then(Json::as_str) {
        out.push_str(&format!(
            " (line {}: {reason})",
            persona::canon(&field(&verification, "failed_line"))
        ));
    }
    out.push_str(". Checked only the prefix through this event.\n");
    if let Some(link) = verification.get("trusted_link").and_then(Json::as_str) {
        out.push_str(&format!(
            "Segment starts at a trusted sealed link: {link}; previous segments not checked.\n"
        ));
    }
    out.push_str("The receipt proves the computed stance, not the model's words or actions.");
    out
}

pub(crate) fn cmd(args: &[String]) {
    let path = args
        .get(1)
        .filter(|p| !p.starts_with("--"))
        .unwrap_or_else(|| crate::fail("explain: probbit explain STRAND --event N [--json]"));
    let mut flags = vec!["explain".to_owned()];
    flags.extend(args.iter().skip(2).cloned());
    let (values, switches) = crate::flags_of("explain");
    crate::check_flags(&flags, &values, &switches);
    let event = flags
        .iter()
        .position(|s| s == "--event")
        .and_then(|i| flags.get(i + 1))
        .and_then(|s| s.parse::<u64>().ok())
        .filter(|n| (1..=9_007_199_254_740_992).contains(n))
        .unwrap_or_else(|| {
            crate::fail("explain: --event N is required; N must be a whole number from 1 to 2^53")
        });
    let source = std::fs::read_to_string(path).unwrap_or_else(|e| {
        crate::bad_input(
            "explain: ",
            json::value("strand", format!("cannot read {path}: {e}")),
        )
    });
    let engine = persona_engine();
    match report(&source, event, &engine) {
        Ok((d, code)) => {
            crate::emit(&if crate::json_flag() {
                persona::canon(&d)
            } else {
                render(&d)
            });
            if code != 0 {
                crate::exit(code);
            }
        }
        Err(Error::Input(e)) => crate::bad_input("explain: ", e),
        Err(Error::Replay(line, why)) => {
            let unsupported = strand::unsupported_field(&why).is_some();
            if crate::json_flag() {
                crate::emit(&persona::canon(&strand::failure(&source, line, &why)));
            } else {
                let why = display_string(&why);
                crate::emit(&format!(
                    "Cannot explain event {event}: {} at line {line}: {why}",
                    if unsupported {
                        "unsupported"
                    } else {
                        "differs"
                    }
                ));
            }
            crate::exit(if unsupported { 5 } else { 1 });
        }
    }
}
