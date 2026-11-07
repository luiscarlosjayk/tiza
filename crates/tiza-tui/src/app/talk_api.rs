//! The socket methods of a talk: `talk.get`, `talk.goto`, `talk.put` and
//! `talk.insert`, the `talk` of `status` and the `scene` of a step reply.
//!
//! Each method does every check that can fail before it writes, and it
//! changes the talk in memory last, after the master is written: an error
//! or a panic on the way leaves the talk as it was.

use std::path::{Path, PathBuf};

use serde_json::Value;
use tiza_core::protocol::{
    Request, SceneAt, StepRef, StepReply, TalkEntryInfo, TalkGotoParams, TalkInsertParams,
    TalkPutParams, TalkReply, TalkStatus,
};
use tiza_core::talk::{self, SceneEntry};
use tiza_core::{Error, fs};

use super::{App, Landing, parse, scene_label};

/// The error of a `talk.*` method with no talk open.
fn no_talk() -> Error {
    Error::Unavailable("no talk is open: open a master .tiza with `tiza talk.tiza`".into())
}

/// An `unsaved_changes` error of a scene change, with what an agent does
/// next: `with` names the request that goes on without the changes.
pub(super) fn unsaved_hint(e: Error, with: &str) -> Error {
    if e.code() != "unsaved_changes" {
        return e;
    }
    Error::Because {
        message: format!("{e}; {with} goes on without them"),
        inner: Box::new(Error::UnsavedChanges),
    }
}

impl App {
    /// `status.talk`.
    pub(super) fn talk_status(&self) -> Option<TalkStatus> {
        let t = self.talk.as_ref()?;
        let s = t.scene();
        let shown = (!s.skip).then(|| {
            t.loaded.scenes[..=t.index]
                .iter()
                .filter(|x| !x.skip)
                .count() as u32
        });
        Some(TalkStatus {
            master: std::path::absolute(&t.loaded.path)
                .unwrap_or_else(|_| t.loaded.path.clone())
                .display()
                .to_string(),
            title: t.loaded.talk.title.clone(),
            position: s.position as u32,
            name: s.name.clone(),
            shown,
            shown_count: t.loaded.shown().count() as u32,
        })
    }

    /// The reply of a step method: in a talk, with the scene reached.
    pub(super) fn step_reply(&self) -> StepReply {
        let mut r = StepReply::of(&self.doc, self.revision);
        r.scene = self.talk.as_ref().map(|t| SceneAt {
            position: t.scene().position as u32,
            name: t.scene().name.clone(),
        });
        r
    }

    /// `talk.get`, and the reply of `talk.put` and `talk.insert`.
    pub(super) fn talk_reply(
        &self,
        removed: Vec<String>,
        created: Option<String>,
    ) -> Result<TalkReply, Error> {
        let status = self.talk_status().ok_or_else(no_talk)?;
        let t = self.talk.as_ref().ok_or_else(no_talk)?;
        let scenes = t
            .loaded
            .talk
            .scenes
            .iter()
            .zip(&t.loaded.scenes)
            .enumerate()
            .map(|(i, (e, s))| TalkEntryInfo {
                entry: e.to_value(),
                position: s.position as u32,
                name: s.name.clone(),
                skip: s.skip,
                // The scene on screen as it is now; the others as their
                // files hold them.
                steps: if i == t.index {
                    Some(self.doc.max_step())
                } else if s.skip {
                    None
                } else {
                    fs::load_document(&s.path).ok().map(|d| d.max_step())
                },
            })
            .collect();
        Ok(TalkReply {
            status,
            scenes,
            revision: self.revision,
            removed,
            created,
        })
    }

    /// `talk.goto {scene, step?, force?}`.
    pub(super) fn talk_goto(&mut self, params: Value) -> Result<StepReply, Error> {
        let p: TalkGotoParams = parse(params)?;
        let t = self.talk.as_ref().ok_or_else(no_talk)?;
        let spec = p.scene.spec()?;
        let index = match t.loaded.resolve(&spec) {
            Ok(i) => i,
            // The scene on screen can have `skip` (a `talk.put` gave it):
            // its own position or name still goes to it.
            Err(e) => {
                let s = t.scene();
                if s.skip
                    && (spec.trim() == s.position.to_string()
                        || s.name.as_deref() == Some(spec.trim()))
                {
                    t.index
                } else {
                    return Err(e);
                }
            }
        };
        let step = match p.step {
            None => None,
            Some(StepRef::Number(n)) => Some(n.to_string()),
            Some(StepRef::Id(s)) => Some(s),
            Some(StepRef::Other(v)) => {
                return Err(Error::Invalid(format!(
                    "`step` must be a step number or a step id, not {v}"
                )));
            }
        };
        if index == t.index {
            self.goto_step(step.as_deref().unwrap_or("0"))?;
        } else {
            let landing = step.map_or(Landing::First, Landing::Named);
            self.try_enter_scene(index, landing, p.force)
                .map_err(|e| unsaved_hint(e, "talk.goto with force: true"))?;
        }
        Ok(self.step_reply())
    }

    /// `talk.put {scenes, force?}`.
    pub(super) fn talk_put(&mut self, params: Value) -> Result<TalkReply, Error> {
        let p: TalkPutParams = parse(params)?;
        if self.talk.is_none() {
            return Err(no_talk());
        }
        self.refuse_no_save_list()?;
        let new = talk::entries_from(&p.scenes)?;
        let removed = self.put_list(new, p.force)?;
        self.talk_reply(removed, None)
    }

    /// `talk.insert {file, at, scene?, name?, force?}`.
    pub(super) fn talk_insert(&mut self, params: Value) -> Result<TalkReply, Error> {
        let p: TalkInsertParams = parse(params)?;
        let t = self.talk.as_ref().ok_or_else(no_talk)?;
        self.refuse_no_save_list()?;
        let count = t.loaded.talk.scenes.len();
        if p.at == 0 || p.at as usize > count + 1 {
            return Err(Error::Invalid(format!(
                "`at` must be a position from 1 to {} (the talk has {count} entries)",
                count + 1
            )));
        }
        let entry = match &p.name {
            Some(n) => serde_json::json!({"file": p.file, "name": n}),
            None => Value::String(p.file.clone()),
        };
        let mut raw: Vec<Value> = t
            .loaded
            .talk
            .scenes
            .iter()
            .map(SceneEntry::to_value)
            .collect();
        raw.insert(p.at as usize - 1, entry);
        // The rules of an entry first (a .tiza file, a name), then the
        // path, then the list on disk: all before a file is written.
        let new = talk::entries_from(&raw)?;
        let base = fs::document_dir(&t.loaded.path);
        let Some(scene) = p.scene else {
            let removed = self.put_list(new, p.force)?;
            return self.talk_reply(removed, None);
        };
        let target = talk::new_scene_path(&base, &p.file)?;
        if !p.force {
            talk::check_list_on_disk(&t.loaded.path, &t.loaded.talk.scenes)?;
        }
        talk::create_scene_file(&target, scene)?;
        match self.put_list(new, p.force) {
            Ok(removed) => self.talk_reply(removed, Some(p.file)),
            Err(e) => {
                let _ = std::fs::remove_file(&target);
                Err(Error::Because {
                    message: format!("{e}; the new file {} is removed again", p.file),
                    inner: Box::new(e),
                })
            }
        }
    }

    /// `talk.put` and `talk.insert` write the master: refused with
    /// `--no-save`, unless the presenter turned autosave on (the rule of
    /// the `at` that Tiza writes at exit).
    fn refuse_no_save_list(&self) -> Result<(), Error> {
        if self.no_save {
            return Err(Error::Invalid(
                "the talk is open with --no-save: Tiza does not write the master (`:autosave on` writes again)"
                    .into(),
            ));
        }
        Ok(())
    }

    /// Makes `new` the list of the talk: every entry that shows is read
    /// (the errors of a master), the scene on screen keeps its place by
    /// its file, the master is written, and then the talk in memory
    /// changes. Returns the files that the new list does not have.
    fn put_list(&mut self, new: Vec<SceneEntry>, force: bool) -> Result<Vec<String>, Error> {
        let t = self.talk.as_ref().ok_or_else(no_talk)?;
        let base = fs::document_dir(&t.loaded.path);
        let mut scenes = talk::load_scenes(&base, &new)?;
        let here: PathBuf = t.scene().path.clone();
        let Some(index) = scenes.iter().position(|s| talk::same_file(&s.path, &here)) else {
            return Err(Error::Invalid(format!(
                "the new list does not have {}, the scene on screen: go to another scene first",
                scene_label(t.scene())
            )));
        };
        // A skipped entry keeps the path as written: the scene on screen
        // keeps its canonical path, which its parked history and the next
        // change of scene use.
        scenes[index].path = here;
        let removed: Vec<String> = t
            .loaded
            .talk
            .scenes
            .iter()
            .zip(&t.loaded.scenes)
            .filter(|(_, old)| !scenes.iter().any(|s| talk::same_file(&s.path, &old.path)))
            .map(|(e, _)| e.file.clone())
            .collect();
        let value = Value::Array(new.iter().map(SceneEntry::to_value).collect());
        let old = t.loaded.talk.scenes.clone();
        talk::update_talk(
            &t.loaded.path,
            (!force).then_some(old.as_slice()),
            "scenes",
            value,
        )?;
        // From here nothing fails: the talk in memory changes.
        for s in &mut scenes {
            s.doc = None;
        }
        let t = self.talk.as_mut().ok_or_else(no_talk)?;
        t.loaded.talk.scenes = new;
        t.loaded.scenes = scenes;
        t.index = index;
        self.revision += 1;
        Ok(removed)
    }

    /// A `talk.*` request in supervised mode with no talk open: refused
    /// now, not parked as a change that cannot happen.
    pub(super) fn talk_refused_before_review(&self, req: &Request) -> Option<Error> {
        (req.method.starts_with("talk.") && self.talk.is_none()).then(no_talk)
    }

    /// The line of a waiting `talk.*` request in the review list, and the
    /// file of the scene that a `talk.goto` goes to then.
    pub(super) fn talk_proposal(&self, req: &Request) -> Option<(String, Option<PathBuf>)> {
        let t = self.talk.as_ref()?;
        match req.method.as_str() {
            "talk.goto" => {
                let spec = req
                    .params
                    .get("scene")
                    .map(|v| v.as_str().map_or_else(|| v.to_string(), str::to_string))?;
                let i = t.loaded.resolve(&spec).ok()?;
                let s = &t.loaded.scenes[i];
                Some((format!("go to {}", scene_label(s)), Some(s.path.clone())))
            }
            "talk.put" => {
                let new = req
                    .params
                    .get("scenes")
                    .and_then(Value::as_array)
                    .and_then(|l| talk::entries_from(l).ok())?;
                Some((list_change(&t.loaded.talk.scenes, &new), None))
            }
            "talk.insert" => {
                let file = req.params.get("file").and_then(Value::as_str)?;
                let at = req.params.get("at").and_then(Value::as_u64)?;
                let new = if req.params.get("scene").is_some() {
                    " (a new file)"
                } else {
                    ""
                };
                Some((format!("add scene {file} at {at}{new}"), None))
            }
            _ => None,
        }
    }

    /// A waiting `talk.goto` whose scene is not at its place any more (a
    /// `talk.put` came first): `conflict`, not a change to another scene.
    pub(super) fn talk_target_moved(&self, req: &Request, target: &Path) -> Option<Error> {
        let t = self.talk.as_ref()?;
        let spec = req
            .params
            .get("scene")
            .map(|v| v.as_str().map_or_else(|| v.to_string(), str::to_string))?;
        let now = t
            .loaded
            .resolve(&spec)
            .ok()
            .map(|i| &t.loaded.scenes[i].path);
        (now.is_none_or(|p| !talk::same_file(p, target))).then(|| {
            Error::Conflict(format!(
                "scene {spec} is not the scene it was when the change waited: the list changed; ask again"
            ))
        })
    }
}

/// `change the list of scenes: +1 −1, 2 moved, skip: move`.
fn list_change(old: &[SceneEntry], new: &[SceneEntry]) -> String {
    let added = new
        .iter()
        .filter(|n| !old.iter().any(|o| o.file == n.file))
        .count();
    let gone = old
        .iter()
        .filter(|o| !new.iter().any(|n| n.file == o.file))
        .count();
    let kept_old: Vec<&str> = old
        .iter()
        .filter(|o| new.iter().any(|n| n.file == o.file))
        .map(|o| o.file.as_str())
        .collect();
    let kept_new: Vec<&str> = new
        .iter()
        .filter(|n| old.iter().any(|o| o.file == n.file))
        .map(|n| n.file.as_str())
        .collect();
    let moved = kept_old
        .iter()
        .zip(&kept_new)
        .filter(|(a, b)| a != b)
        .count();
    let skip: Vec<String> = new
        .iter()
        .filter(|n| n.skip && !old.iter().any(|o| o.file == n.file && o.skip))
        .map(|n| n.name().unwrap_or_else(|| n.file.clone()))
        .collect();
    let mut parts = Vec::new();
    if added > 0 || gone > 0 {
        parts.push(format!("+{added} −{gone}"));
    }
    if moved > 0 {
        parts.push(format!("{moved} moved"));
    }
    if !skip.is_empty() {
        parts.push(format!("skip: {}", skip.join(", ")));
    }
    if parts.is_empty() {
        "change the list of scenes (names only)".into()
    } else {
        format!("change the list of scenes: {}", parts.join(", "))
    }
}
