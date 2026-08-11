//! The day files double as Mermaid stateDiagram-v2 documents. This locks the
//! emitted line grammar: `id: name [id N] [q ...]? [HH#colon;MM-HH#colon;MM]`,
//! with `:` never appearing raw after the first separator (Mermaid treats it
//! as a new description) and the q-tag body restricted to the name charset,
//! digits, `=` and single spaces.

use successlib::{add_goal, add_session, QuantityValue};

fn qv(name: &str, value: u32) -> QuantityValue {
    QuantityValue { name: name.into(), value }
}

#[test]
fn emitted_day_files_are_valid_state_diagrams() {
    let dir = tempfile::tempdir().unwrap();
    let archive = dir.path().to_str().unwrap().to_string();
    let goal = add_goal(archive.clone(), "Japanese".into(), false, vec![],
        vec!["cards".into(), "known".into()]).unwrap();
    let session = add_session(archive.clone(), goal.id, "Japanese".into(),
        1_786_766_400, 1500, false, vec![qv("known", 1520), qv("cards", 42)]).unwrap();

    let date = successlib::timestamp_to_date_iso(session.start_at);
    let path = dir.path().join("graphs").join(format!("{date}.mmd"));
    let text = std::fs::read_to_string(path).unwrap();

    let mut lines = text.lines();
    assert_eq!(lines.next(), Some("stateDiagram-v2"));
    for line in lines {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.contains("-->") {
            continue;
        }
        // A session line: exactly one raw ':' — the id/description separator.
        assert_eq!(
            trimmed.matches(':').count(),
            1,
            "raw ':' beyond the separator breaks Mermaid: {trimmed}"
        );
        // The q tag, when present, holds only name=value pairs.
        if let Some(q_start) = trimmed.find("[q ") {
            let body_start = q_start + 3;
            let body_end = body_start + trimmed[body_start..].find(']').expect("closed tag");
            let body = &trimmed[body_start..body_end];
            assert!(
                body.chars().all(|c| c.is_ascii_lowercase()
                    || c.is_ascii_digit()
                    || matches!(c, '_' | '-' | '=' | ' ')),
                "q tag smuggled a hostile character: {body:?}"
            );
            assert_eq!(body, "cards=42 known=1520", "sorted, space-separated: {body:?}");
        }
    }
}
