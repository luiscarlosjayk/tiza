//! Named steps (ADR 0007 §4, t-099): an optional `steps` list in the
//! document gives a step a title, an id that `step.goto` / `tiza goto`
//! jump to, and an `auto_advance_ms` after which the TUI (and the native
//! GIF/MP4) moves on by itself. Each entry names its step by number
//! (`step: N`), never by its position in the list: moving an annotation to
//! another step with `+`/`-` does not shift a title. Entries may be sparse
//! and are saved sorted by `step`.
//!
//! Automation only (decided 2026-10-17): no presenter notes, no timer, no
//! clicker features.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize, Serializer};

use crate::Error;
use crate::model::{Document, single_line};

/// One named step.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct NamedStep {
    /// The step this entry names (0 = the code alone). Explicit, so
    /// reordering the list or moving annotations never rebinds it.
    pub step: u32,
    /// What `step.goto {id}`, MCP `goto_step` and `tiza goto ID` jump to.
    /// Unique, no blanks, not all digits (a number means a step number) and
    /// not starting with `-` (`tiza goto -1` would read it as an option).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// Shown on the status line (`step 3/7 · Shared borrow`) and in `status`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Go to the next step on its own this many milliseconds after the step
    /// is ready (its typing finished and its `wait` moves elapsed). The
    /// native GIF/MP4 hold the step's frame this long instead of `--interval`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = 1))]
    pub auto_advance_ms: Option<u64>,
    /// Keys this version does not know (the dropped `timer_min`, a typo,
    /// a field from a newer Tiza): kept in the file as written, never used,
    /// and said on load ([`Document::step_warnings`]) instead of ignored in
    /// silence. Not part of the schema.
    #[serde(flatten, default, skip_serializing_if = "BTreeMap::is_empty")]
    #[schemars(skip)]
    pub unknown: BTreeMap<String, serde_json::Value>,
}

/// Saves the entries sorted by `step`, whatever order they are in memory.
pub fn serialize_sorted<S: Serializer>(steps: &[NamedStep], s: S) -> Result<S::Ok, S::Error> {
    let mut sorted: Vec<&NamedStep> = steps.iter().collect();
    sorted.sort_by_key(|n| n.step);
    s.collect_seq(sorted)
}

/// Checks and tidies a `steps` list (load, `scene.put`, `scene.patch`):
/// single-line titles, ids trimmed, unique, without blanks, not all digits
/// and not starting with `-`, one entry per step, `auto_advance_ms >= 1`;
/// sorted by `step`.
pub fn normalize(steps: &mut [NamedStep]) -> Result<(), Error> {
    let mut ids = std::collections::BTreeSet::new();
    let mut numbers = std::collections::BTreeSet::new();
    for n in steps.iter_mut() {
        crate::model::check_step(n.step, "steps")?;
        if let Some(ms) = n.auto_advance_ms {
            crate::model::check_hold_ms(ms, &format!("steps: auto_advance_ms of step {}", n.step))?;
        }
        if !numbers.insert(n.step) {
            return Err(Error::Invalid(format!(
                "steps: two entries name step {} (one entry per step)",
                n.step
            )));
        }
        if let Some(id) = &mut n.id {
            *id = id.trim().to_string();
            if id.is_empty() {
                return Err(Error::Invalid(format!(
                    "steps: the entry of step {} has an empty `id` (leave it out instead)",
                    n.step
                )));
            }
            if id.starts_with('-') {
                return Err(Error::Invalid(format!(
                    "steps: id {id:?} (step {}) starts with `-`, which `tiza goto` would read as an option; start it with a letter or a digit",
                    n.step
                )));
            }
            if id.chars().all(|c| c.is_ascii_digit()) {
                return Err(Error::Invalid(format!(
                    "steps: id {id:?} (step {}) is all digits; a number means a step number, so ids need a letter",
                    n.step
                )));
            }
            if id.chars().any(|c| c.is_whitespace() || c.is_control()) {
                return Err(Error::Invalid(format!(
                    "steps: id {id:?} (step {}) has blanks; use one word (`shared-borrow`)",
                    n.step
                )));
            }
            if !ids.insert(id.clone()) {
                return Err(Error::Invalid(format!(
                    "steps: id {id:?} is used twice (ids must be unique)"
                )));
            }
        }
        if let Some(t) = &mut n.title {
            *t = single_line(t).trim().to_string();
        }
        if n.title.as_deref() == Some("") {
            n.title = None;
        }
        if n.auto_advance_ms == Some(0) {
            return Err(Error::Invalid(format!(
                "steps: `auto_advance_ms` of step {} is 0; give at least 1 ms or leave it out",
                n.step
            )));
        }
    }
    steps.sort_by_key(|n| n.step);
    Ok(())
}

impl Document {
    /// The entry that names `step`, if any.
    pub fn named_step(&self, step: u32) -> Option<&NamedStep> {
        self.steps.iter().find(|n| n.step == step)
    }

    /// What the status line shows after the number: the title, else the id.
    pub fn step_title(&self, step: u32) -> Option<String> {
        let n = self.named_step(step)?;
        // Shown text: a title that repeats a masked value is masked (t-174).
        n.title
            .as_deref()
            .or(n.id.as_deref())
            .map(|t| crate::redact::shown(t, self))
    }

    /// `step 3/7`, or `step 3/7 · Shared borrow` on a named step.
    pub fn step_label(&self, step: u32) -> String {
        let base = format!("step {step}/{}", self.max_step());
        match self.step_title(step) {
            Some(t) => format!("{base} · {t}"),
            None => base,
        }
    }

    /// The `auto_advance_ms` of `step`, when it has one and the step exists.
    pub fn auto_advance_ms(&self, step: u32) -> Option<u64> {
        if step > self.max_step() {
            return None;
        }
        self.named_step(step).and_then(|n| n.auto_advance_ms)
    }

    /// The step after the current one: one more, or step 0 past the last
    /// one when the document loops; `None` at the last step without `loop`.
    pub fn next_step_from(&self, step: u32) -> Option<u32> {
        if step < self.max_step() {
            Some(step + 1)
        } else if self.looping {
            Some(0)
        } else {
            None
        }
    }

    /// The step `goto ID` names: a number is a step number (0 to the last
    /// step); anything else is an `id` of `steps`.
    pub fn resolve_step(&self, id: &str) -> Result<u32, Error> {
        let id = id.trim();
        let max = self.max_step();
        if !id.is_empty() && id.chars().all(|c| c.is_ascii_digit()) {
            let n: u32 = id
                .parse()
                .map_err(|_| Error::Invalid(format!("step {id} is not a valid number")))?;
            if n > max {
                return Err(Error::Invalid(format!(
                    "step {n} is beyond the last step ({max})"
                )));
            }
            return Ok(n);
        }
        let Some(n) = self.steps.iter().find(|n| n.id.as_deref() == Some(id)) else {
            let known: Vec<&str> = self.steps.iter().filter_map(|n| n.id.as_deref()).collect();
            return Err(Error::StepNotFound(if known.is_empty() {
                format!("no step is named {id:?}: the document has no named steps")
            } else {
                format!(
                    "no step is named {id:?} (named steps: {})",
                    known.join(", ")
                )
            }));
        };
        if n.step > max {
            return Err(Error::Invalid(format!(
                "step {:?} names step {}, beyond the last step ({max})",
                id, n.step
            )));
        }
        Ok(n.step)
    }

    /// What the author should know about `steps` (`status.warnings`, the
    /// TUI on open, `render` and `tiza check`): entries that name a step
    /// past the last one — kept in the file (deleting the last step's
    /// annotations must not lose them, nor make the file fail to load),
    /// never shown — and keys an entry has that this version does not know
    /// (kept as written, never used).
    pub fn step_warnings(&self) -> Vec<String> {
        let max = self.max_step();
        let name = |n: &NamedStep| {
            n.id.as_deref()
                .or(n.title.as_deref())
                // Shown text (t-174).
                .map(|t| format!(" «{}»", crate::redact::shown(t, self)))
                .unwrap_or_default()
        };
        let mut out: Vec<String> = self
            .steps
            .iter()
            .filter(|n| n.step > max)
            .map(|n| {
                format!(
                    "steps: entry{} names step {}, beyond the last step ({max}); kept, not used",
                    name(n),
                    n.step
                )
            })
            .collect();
        // Shown text (t-174): a step's title, id or unknown key repeating a
        // masked value is masked.
        let shown = |s: String| crate::redact::shown_message(&s, self);
        out = out.into_iter().map(shown).collect();
        for n in self.steps.iter().filter(|n| !n.unknown.is_empty()) {
            let keys: Vec<String> = n.unknown.keys().map(|k| format!("`{k}`")).collect();
            out.push(format!(
                "steps: entry{} (step {}) has unknown {} {}; kept in the file, not used (known: step, id, title, auto_advance_ms)",
                name(n),
                n.step,
                if keys.len() == 1 { "key" } else { "keys" },
                keys.join(", ")
            ));
        }
        out.into_iter().map(shown).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn doc(steps: serde_json::Value) -> Result<Document, Error> {
        let mut d: Document = serde_json::from_value(json!({
            "schema": 1, "source": {"language": "rs"}, "code": ["a", "b"],
            "annotations": [
                {"id": "n1", "step": 1, "type": "note", "line": 1, "text": "x"},
                {"id": "n2", "step": 2, "type": "note", "line": 2, "text": "y"}
            ],
            "steps": steps
        }))
        .unwrap();
        d.code = vec!["a".into(), "b".into()];
        d.normalize_loaded().map(|_| d)
    }

    #[test]
    fn titles_by_explicit_step_sorted_on_save() {
        let d = doc(json!([
            {"step": 2, "id": "two", "title": "Second"},
            {"step": 0, "id": "intro"}
        ]))
        .unwrap();
        assert_eq!(d.step_label(2), "step 2/2 · Second");
        assert_eq!(d.step_label(0), "step 0/2 · intro");
        assert_eq!(d.step_label(1), "step 1/2");
        let v = serde_json::to_value(&d).unwrap();
        assert_eq!(v["steps"][0]["step"], 0);
        assert_eq!(v["steps"][1]["step"], 2);
        assert_eq!(d.resolve_step("two").unwrap(), 2);
        assert_eq!(d.resolve_step("1").unwrap(), 1);
        assert_eq!(d.resolve_step("nope").unwrap_err().code(), "not_found");
    }

    #[test]
    fn bad_entries_are_rejected_on_load() {
        for bad in [
            json!([{"step": 1, "id": "a"}, {"step": 2, "id": "a"}]),
            json!([{"step": 1, "id": "3"}]),
            json!([{"step": 1, "id": "two words"}]),
            json!([{"step": 1}, {"step": 1, "id": "b"}]),
            json!([{"step": 1, "auto_advance_ms": 0}]),
            json!([{"step": 1, "id": "-1"}]),
            json!([{"step": 1, "id": " -x"}]),
        ] {
            assert!(doc(bad.clone()).is_err(), "{bad}");
        }
    }

    #[test]
    fn an_entry_past_the_last_step_is_kept_and_warned() {
        let d = doc(json!([{"step": 9, "id": "later"}])).unwrap();
        assert_eq!(d.steps.len(), 1);
        assert_eq!(d.step_warnings().len(), 1);
        assert!(d.step_warnings()[0].contains("step 9"));
        assert!(d.resolve_step("later").is_err());
    }

    #[test]
    fn an_unknown_key_is_kept_and_warned() {
        let d = doc(json!([
            {"step": 1, "id": "one", "timer_min": 5},
            {"step": 2, "title": "Two", "auto_advance_ms": 10}
        ]))
        .unwrap();
        let w = d.step_warnings();
        assert_eq!(w.len(), 1, "{w:?}");
        assert!(
            w[0].contains("«one» (step 1) has unknown key `timer_min`"),
            "{w:?}"
        );
        let v = serde_json::to_value(&d).unwrap();
        assert_eq!(v["steps"][0]["timer_min"], 5);
        assert!(v["steps"][1].get("unknown").is_none());
        assert_eq!(d.auto_advance_ms(2), Some(10));
    }

    #[test]
    fn loop_wraps_only_past_the_last_step() {
        let mut d = doc(json!([])).unwrap();
        assert_eq!(d.next_step_from(2), None);
        d.looping = true;
        assert_eq!(d.next_step_from(1), Some(2));
        assert_eq!(d.next_step_from(2), Some(0));
        let v = serde_json::to_value(&d).unwrap();
        assert_eq!(v["loop"], true);
    }
}
