//! Explain existing, frozen strands without changing them or their replay semantics.
use probbit_persona::{json, persona};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

fn explain(path: &Path, event: &str, json: bool) -> Output {
    let mut c = Command::new(env!("CARGO_BIN_EXE_probbit"));
    c.arg("explain").arg(path).args(["--event", event]);
    if json {
        c.arg("--json");
    }
    c.output().unwrap()
}

fn doc(o: &Output) -> json::Json {
    json::parse(std::str::from_utf8(&o.stdout).unwrap()).unwrap()
}

fn at<'a>(j: &'a json::Json, keys: &[&str]) -> &'a json::Json {
    keys.iter().fold(j, |v, k| v.get(k).unwrap())
}

struct Copy(PathBuf);
impl Copy {
    fn new(text: &str) -> Self {
        static ID: AtomicUsize = AtomicUsize::new(0);
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../target/explain-tests");
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join(format!(
            "{}-{}.strand",
            std::process::id(),
            ID.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::write(&p, text).unwrap();
        Self(p)
    }
}
impl Drop for Copy {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

#[test]
fn normal_event_matches_the_frozen_checkpoint_and_has_prior_odds() {
    let p = fixture("monitor/tutor-expect.strand");
    let original = std::fs::read_to_string(&p).unwrap();
    let o = explain(&p, "5", true);
    assert_eq!(o.status.code(), Some(0), "{:?}", o);
    let d = doc(&o);
    assert_eq!(at(&d, &["result"]).as_str(), Some("recomputes"));
    assert_eq!(at(&d, &["event", "n"]).as_f64(), Some(5.0));
    let checkpoint = original
        .lines()
        .map(|l| json::parse(l).unwrap())
        .find(|j| j.get("checkpoint").and_then(json::Json::as_f64) == Some(5.0))
        .unwrap();
    assert_eq!(at(&d, &["after"]), at(&checkpoint, &["stance"]));
    assert!(at(&d, &["before", "stance", "humour", "odds"])
        .as_obj()
        .is_some());
    let habits = at(&d, &["bound_habits"]).as_arr().unwrap();
    assert!(habits
        .iter()
        .any(
            |h| h.get("id").and_then(json::Json::as_str) == Some("flourish_budget")
                && h.get("rules").is_some()
        ));
    let event_line = original
        .lines()
        .find(|l| {
            json::parse(l)
                .unwrap()
                .get("n")
                .and_then(json::Json::as_f64)
                == Some(5.0)
        })
        .unwrap();
    assert_eq!(
        at(&d, &["chain", "this"]).as_str(),
        Some(persona::digest_of(event_line).as_str())
    );
    let human = explain(&p, "5", false);
    let human = String::from_utf8(human.stdout).unwrap();
    for expected in [
        "Event 5",
        "Before",
        "After",
        "odds",
        "flourish_budget",
        "->",
        "recomputes",
    ] {
        assert!(human.contains(expected), "missing {expected}: {human}");
    }
    assert_eq!(std::fs::read_to_string(p).unwrap(), original);
}

#[test]
fn first_event_has_initial_levels_but_no_invented_odds() {
    let o = explain(&fixture("strands/agentdojo-0.11.0.strand"), "1", true);
    assert_eq!(o.status.code(), Some(0), "{:?}", o);
    let d = doc(&o);
    assert_eq!(at(&d, &["before_source"]).as_str(), Some("initial_state"));
    assert_eq!(
        at(&d, &["before", "stance", "caution", "level"]).as_str(),
        Some("measured")
    );
    assert!(at(&d, &["before", "stance", "caution", "odds"]).is_null());
    assert!(at(&d, &["time", "wall_time"]).is_null());
}

#[test]
fn flags_and_conditional_habits_explain_the_held_level() {
    let o = explain(&fixture("monitor/tutor-week.strand"), "18", true);
    assert_eq!(o.status.code(), Some(0), "{:?}", o);
    let d = doc(&o);
    assert_eq!(at(&d, &["flags", "loss"]), &json::Json::Bool(true));
    assert_eq!(
        at(&d, &["after", "stance", "humour", "level"]).as_str(),
        Some("none")
    );
    assert_eq!(
        at(&d, &["after", "stance", "humour", "odds", "none"]).as_f64(),
        Some(1.0)
    );
    assert!(at(&d, &["bound_habits"]).as_arr().unwrap().iter().any(|h| h
        .get("then")
        .and_then(|t| t.get("humour"))
        .is_some()
        && h.get("held")
            .and_then(|t| t.get("humour"))
            .and_then(|t| t.get("level"))
            .and_then(json::Json::as_str)
            == Some("none")));
}

#[test]
fn checkpoint_is_not_an_event_and_crlf_is_accepted() {
    let original =
        std::fs::read_to_string(fixture("strands/sensed-credit-0.9.0-dev.strand")).unwrap();
    let copy = Copy::new(&original.replace('\n', "\r\n"));
    let o = explain(&copy.0, "5", true);
    assert_eq!(o.status.code(), Some(0), "{:?}", o);
    assert_eq!(at(&doc(&o), &["line"]).as_f64(), Some(7.0));
}

#[test]
fn missing_out_of_range_and_invalid_flags_are_input_errors() {
    let p = fixture("strands/agentdojo-0.11.0.strand");
    for event in ["2", "0", "-1", "1.5", "9007199254740993"] {
        let o = explain(&p, event, true);
        assert_eq!(o.status.code(), Some(2), "{event}: {:?}", o);
        assert!(doc(&o).get("error").is_some());
        assert!(!o.stderr.is_empty());
    }
    let o = explain(&fixture("does-not-exist.strand"), "1", true);
    assert_eq!(o.status.code(), Some(2));
    assert!(String::from_utf8(o.stderr).unwrap().contains("cannot read"));
}

#[test]
fn malformed_line_is_a_clear_input_error() {
    let original = std::fs::read_to_string(fixture("strands/agentdojo-0.11.0.strand")).unwrap();
    let copy = Copy::new(&format!(
        "{}\n{{invalid\n",
        original.lines().next().unwrap()
    ));
    let o = explain(&copy.0, "1", true);
    assert_eq!(o.status.code(), Some(2), "{:?}", o);
    assert!(String::from_utf8_lossy(&o.stderr).contains("line 2"));
    assert!(doc(&o).get("error").is_some());
}

#[test]
fn tampered_digest_reports_differs_and_the_recomputed_stance() {
    let original = std::fs::read_to_string(fixture("strands/agentdojo-0.11.0.strand")).unwrap();
    let mut lines: Vec<String> = original.lines().map(str::to_owned).collect();
    let mut j = json::parse(&lines[1]).unwrap();
    if let json::Json::Obj(v) = &mut j {
        v.iter_mut().find(|(k, _)| k == "stance").unwrap().1 = json::str("sha256:tampered");
    }
    lines[1] = persona::canon(&j);
    let copy = Copy::new(&(lines.join("\n") + "\n"));
    let o = explain(&copy.0, "1", true);
    assert_eq!(o.status.code(), Some(1), "{:?}", o);
    let d = doc(&o);
    assert_eq!(at(&d, &["result"]).as_str(), Some("differs"));
    assert!(at(&d, &["verification", "reason"])
        .as_str()
        .unwrap()
        .contains("stance differs"));
    assert!(at(&d, &["after", "stance", "caution", "odds"])
        .as_obj()
        .is_some());
    let human = explain(&copy.0, "1", false);
    assert!(String::from_utf8(human.stdout).unwrap().contains("differs"));
}

#[test]
fn earlier_corruption_is_not_presented_as_a_trustworthy_before_stance() {
    let original = std::fs::read_to_string(fixture("monitor/tutor-expect.strand")).unwrap();
    let mut lines: Vec<String> = original.lines().map(str::to_owned).collect();
    lines[1] = lines[1].replace("\"state\":\"sha256:", "\"state\":\"sha256:bad");
    let copy = Copy::new(&(lines.join("\n") + "\n"));
    let o = explain(&copy.0, "2", true);
    assert_eq!(o.status.code(), Some(1), "{:?}", o);
    assert!(doc(&o).get("after").is_none());
    assert_eq!(at(&doc(&o), &["line"]).as_f64(), Some(2.0));
}

#[test]
fn unknown_logged_field_is_unsupported_not_a_divergence() {
    let original = std::fs::read_to_string(fixture("strands/agentdojo-0.11.0.strand")).unwrap();
    let mut lines: Vec<String> = original.lines().map(str::to_owned).collect();
    let mut j = json::parse(&lines[1]).unwrap();
    if let json::Json::Obj(v) = &mut j {
        v.push(("future_field".into(), json::Json::Bool(true)));
    }
    lines[1] = persona::canon(&j);
    let copy = Copy::new(&(lines.join("\n") + "\n"));
    let o = explain(&copy.0, "1", true);
    assert_eq!(o.status.code(), Some(5), "{:?}", o);
    assert!(doc(&o).get("unsupported").is_some());
}

#[test]
fn unknown_line_kind_at_the_requested_position_is_unsupported() {
    for event in ["6", "7"] {
        let o = explain(&fixture("strands/next-line.strand"), event, true);
        assert_eq!(o.status.code(), Some(5), "{event}: {:?}", o);
        assert_eq!(at(&doc(&o), &["unsupported"]).as_str(), Some("nap"));
    }
}

#[test]
fn malformed_event_envelope_is_bad_input_even_before_the_selected_event() {
    let source = std::fs::read_to_string(fixture("monitor/tutor-expect.strand")).unwrap();
    for (key, bad) in [
        ("n", json::str("bad")),
        ("inputs", json::Json::Arr(vec![])),
        ("state", json::Json::Null),
    ] {
        let mut lines: Vec<String> = source.lines().map(str::to_owned).collect();
        let mut j = json::parse(&lines[1]).unwrap();
        if let json::Json::Obj(v) = &mut j {
            v.iter_mut().find(|(k, _)| k == key).unwrap().1 = bad;
        }
        lines[1] = persona::canon(&j);
        let copy = Copy::new(&(lines.join("\n") + "\n"));
        for event in ["1", "2"] {
            let o = explain(&copy.0, event, true);
            assert_eq!(o.status.code(), Some(2), "{key}, {event}: {:?}", o);
            assert!(doc(&o).get("error").is_some());
        }
    }
}

#[test]
fn human_output_does_not_emit_terminal_controls_from_a_tampered_hash() {
    let source = std::fs::read_to_string(fixture("strands/agentdojo-0.11.0.strand")).unwrap();
    let mut lines: Vec<String> = source.lines().map(str::to_owned).collect();
    let mut j = json::parse(&lines[1]).unwrap();
    if let json::Json::Obj(v) = &mut j {
        v.iter_mut().find(|(k, _)| k == "prev").unwrap().1 =
            json::str("\u{1b}]0;FAKE\u{7}\r\nRecomputation: recomputes");
    }
    lines[1] = persona::canon(&j);
    let copy = Copy::new(&(lines.join("\n") + "\n"));
    let o = explain(&copy.0, "1", false);
    assert_eq!(o.status.code(), Some(1), "{:?}", o);
    let human = String::from_utf8(o.stdout).unwrap();
    assert!(human.chars().all(|c| !c.is_control() || c == '\n'));
    assert!(!human.lines().any(|l| l == "Recomputation: recomputes"));
    assert!(human.contains("Recomputation: differs"));
}

#[test]
fn an_event_after_pause_preserves_the_authoritative_divergence() {
    let source = std::fs::read_to_string(fixture("strands/agentdojo-0.11.0.strand")).unwrap();
    let lines: Vec<&str> = source.lines().collect();
    let pause = probbit_persona::strand::control_line(
        &persona::digest_of(lines[0]),
        "pause",
        "human:test",
        "test",
        "2026-11-03T00:00:00Z",
    )
    .unwrap();
    let mut j = json::parse(lines[1]).unwrap();
    if let json::Json::Obj(v) = &mut j {
        v.iter_mut().find(|(k, _)| k == "prev").unwrap().1 = json::str(&persona::digest_of(&pause));
    }
    let copy = Copy::new(&format!("{}\n{pause}\n{}\n", lines[0], persona::canon(&j)));
    let o = explain(&copy.0, "1", true);
    assert_eq!(o.status.code(), Some(1), "{:?}", o);
    let d = doc(&o);
    assert!(at(&d, &["diverges"]).as_str().unwrap().contains("paused"));
    assert!(d.get("after").is_none());
}
