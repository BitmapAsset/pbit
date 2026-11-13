//! Damaged strands: `live verify` and `explain` answer a broken file with an exit code (0, 1, 2 or 5) and a JSON document,
//! never a panic. A small strand is written by the engine (3 events of the sensed example, a checkpoint after the 2nd),
//! then damaged one place at a time.
use probbit_persona::json::{self, Json};
use probbit_persona::persona;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

const SENSED: &str = "examples/persona/sensed.yaml";

fn root() -> PathBuf { Path::new(env!("CARGO_MANIFEST_DIR")).join("..") }
fn tmp(name: &str) -> PathBuf {
    static ID: AtomicUsize = AtomicUsize::new(0);
    PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!("damaged-{}-{}-{name}", std::process::id(), ID.fetch_add(1, Ordering::Relaxed)))
}
fn probbit(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_probbit")).args(args).current_dir(root()).output().unwrap()
}

/// The strand's text: header, events 1 and 2, the checkpoint after event 2, event 3.
fn small_strand() -> String {
    let day = std::fs::read_to_string(root().join("examples/persona/golden/sensed/day.json")).unwrap();
    let events: String = json::parse(&day).unwrap().as_arr().unwrap()[..3].iter().map(|e| json::write(e, false) + "\n").collect();
    let (ev, st, strand) = (tmp("events.jsonl"), tmp("state.json"), tmp("small.strand"));
    std::fs::write(&ev, events).unwrap();
    let o = probbit(&["persona", "init", SENSED, "--seed", "3", "--out", st.to_str().unwrap()]);
    assert_eq!(o.status.code(), Some(0), "{o:?}");
    let o = probbit(&["live", SENSED, "--state", st.to_str().unwrap(), "--clock", "fixed", "--events", ev.to_str().unwrap(),
        "--strand", strand.to_str().unwrap(), "--checkpoint-every", "2"]);
    assert_eq!(o.status.code(), Some(0), "{o:?}");
    let text = std::fs::read_to_string(&strand).unwrap().replace("\r\n", "\n");
    for f in [&ev, &st, &strand] { let _ = std::fs::remove_file(f); }
    let _ = std::fs::remove_file(format!("{}.lock", strand.display()));
    assert_eq!(text.lines().count(), 5, "{text}");
    assert!(text.lines().nth(3).unwrap().starts_with("{\"checkpoint\":2"), "{text}");
    text
}

/// Both readers on `text`: (`live verify` output, `explain --event 1 --json` output).
fn read_both(text: &str) -> (Output, Output) {
    let [verify, explain, _] = read_all(text);
    (verify, explain)
}

/// `live verify`, `explain --event 1 --json` and `explain --event 3 --json` (the last event, so explain reads every line).
fn read_all(text: &str) -> [Output; 3] {
    let p = tmp("variant.strand");
    std::fs::write(&p, text).unwrap();
    let path = p.to_str().unwrap();
    let out = [
        probbit(&["live", "verify", path]),
        probbit(&["explain", path, "--event", "1", "--json"]),
        probbit(&["explain", path, "--event", "3", "--json"]),
    ];
    let _ = std::fs::remove_file(&p);
    out
}

fn header_with_persona_digest(text: &str, digest: &str) -> String {
    let mut lines: Vec<String> = text.lines().map(str::to_owned).collect();
    let mut h = json::parse(&lines[0]).unwrap();
    if let Json::Obj(kv) = &mut h {
        let st = &mut kv.iter_mut().find(|(k, _)| k == "state").unwrap().1;
        if let Json::Obj(skv) = st {
            let p = &mut skv.iter_mut().find(|(k, _)| k == "persona").unwrap().1;
            if let Json::Obj(pkv) = p { pkv.iter_mut().find(|(k, _)| k == "digest").unwrap().1 = Json::Str(digest.into()); }
        }
    }
    lines[0] = persona::canon(&h);
    lines.join("\n") + "\n"
}

#[test]
fn a_non_ascii_persona_digest_in_the_header_is_a_divergence_not_a_panic() {
    let text = header_with_persona_digest(&small_strand(), &"é".repeat(10));
    let (verify, explain) = read_both(&text);
    for (name, o) in [("live verify", &verify), ("explain", &explain)] {
        assert_eq!(o.status.code(), Some(1), "{name}: {o:?}");
        let d = json::parse(std::str::from_utf8(&o.stdout).unwrap().trim()).unwrap_or_else(|e| panic!("{name}: not JSON ({})", e.msg));
        let why = d.get("diverges").and_then(Json::as_str).unwrap_or_else(|| panic!("{name}: no diverges: {o:?}"));
        assert!(why.contains("state.persona.digest") && why.contains("éééééééééé"), "{name}: {why}");
    }
}
