//! A talk: a master `.tiza` that plays several scenes in a row.
//!
//! Each scene is an [`App`] of its own, opened as `tiza SCENE` opens it.
//! What belongs to the session goes from one scene to the next when the
//! presenter crosses: the options of the command line and the config
//! ([`SessionStart`]), the terminal, the socket, the counters, and what the
//! presenter changed live (presentation mode, the mouse, follow, autosave,
//! the layout). Everything else starts again, as when a file opens.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use tiza_core::history::History;
use tiza_core::talk::{At, LoadedTalk, TalkScene};
use tiza_core::{Document, Error, fs};

use super::App;
use crate::SessionStart;

/// Where a scene opens when the presenter comes to it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Landing {
    /// Step 0, with its countdown: `n` past the end of the scene before.
    First,
    /// The last step, the countdown stopped until the next key: `p` from
    /// the scene after (an intro that advances by itself does not push the
    /// presenter forward again).
    Last,
    /// A step number (`at`), the countdown stopped. A number past the last
    /// step is the last step.
    At(u32),
    /// A step as `:goto` names it (`:scene NAME STEP`), the countdown
    /// stopped.
    Named(String),
}

/// The talk of an instance: the master, the scene on screen, and what goes
/// from one scene to the next.
pub struct TalkState {
    pub loaded: LoadedTalk,
    /// The scene on screen: an index into `loaded.scenes`.
    pub index: usize,
    /// The options every scene opens with: the ones of the command line
    /// that are for the first scene only (`--step`, `--focus`) and the
    /// warnings of the config are gone.
    start: SessionStart,
    /// Follow is on for the scenes that have a file to follow. A scene
    /// without one leaves it as it was.
    follow_wanted: bool,
    /// The undo history of each scene left with all its changes saved, and
    /// what its files held then: it comes back only when they are the same.
    /// By the canonical path of the scene, so that a new order of the list
    /// gives no scene the history of another.
    parked: HashMap<PathBuf, Parked>,
}

struct Parked {
    history: History,
    fingerprint: Vec<u8>,
    /// With `--no-save`, the document with the changes that were not
    /// saved.
    doc: Option<Document>,
}

impl TalkState {
    /// The next scene that shows after the one on screen.
    fn next_shown(&self) -> Option<usize> {
        (self.index + 1..self.loaded.scenes.len()).find(|&i| !self.loaded.scenes[i].skip)
    }

    /// The scene that shows before the one on screen.
    fn prev_shown(&self) -> Option<usize> {
        (0..self.index).rev().find(|&i| !self.loaded.scenes[i].skip)
    }

    pub fn scene(&self) -> &TalkScene {
        &self.loaded.scenes[self.index]
    }
}

/// `scene 3 (borrow)`, or `scene 3 (intro.tiza)` for a scene without a
/// name.
pub fn scene_label(s: &TalkScene) -> String {
    let name = s.name.clone().unwrap_or_else(|| {
        s.path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default()
    });
    format!("scene {} ({name})", s.position)
}

/// What the files of a scene hold: the `.tiza` and every file it names.
/// `None` when the `.tiza` does not read.
fn fingerprint(doc: &Document, path: &Path) -> Option<Vec<u8>> {
    let mut out = std::fs::read(path).ok()?;
    let files = RefCell::new(Vec::new());
    doc.clone().map_files(&|f| {
        files.borrow_mut().push(f.to_string());
        f.to_string()
    });
    let dir = fs::document_dir(path);
    let mut files = files.into_inner();
    files.sort();
    files.dedup();
    for f in files {
        out.push(0);
        out.extend(f.as_bytes());
        out.push(0);
        out.extend(std::fs::read(dir.join(&f)).unwrap_or_default());
    }
    Some(out)
}

impl App {
    /// The scene a talk opens on: `scene` (`--scene`, a position or a
    /// name) at its own step, else the one its `at` names, else the first
    /// scene that shows. The app is not started yet: the binary opens it
    /// with `start` as it opens a document.
    pub fn first_scene(
        mut loaded: LoadedTalk,
        start: &SessionStart,
        scene: Option<&str>,
    ) -> Result<(App, Option<String>), Error> {
        let (index, at) = match scene {
            Some(spec) => (loaded.resolve(spec)?, None),
            None => {
                let (i, step) = loaded.resume();
                (i, loaded.talk.at.is_some().then_some(step))
            }
        };
        for (i, s) in loaded.scenes.iter_mut().enumerate() {
            if i != index {
                s.doc = None;
            }
        }
        let mut doc = match loaded.scenes[index].doc.take() {
            Some(d) => d,
            None => fs::load_document(&loaded.scenes[index].path)?,
        };
        let notice = fs::open_notice(&doc);
        let disk_step = doc.step;
        doc.step = at.map_or(0, |s| s.min(doc.max_step()));
        let moved = doc.step != disk_step;
        let sync = moved.then(|| doc.sync_code_to_step()).flatten();
        let mut app = App::new(doc, loaded.scenes[index].path.clone());
        if let Some(e) = sync {
            app.error(format!("moves: {e}"));
        }
        app.disk_step = Some(disk_step);
        if at.is_some() {
            app.step_ready_at = None;
        }
        let session = SessionStart {
            step: None,
            focus: None,
            theme_file: None,
            config_warnings: Vec::new(),
            ..start.clone()
        };
        app.talk = Some(Box::new(TalkState {
            loaded,
            index,
            follow_wanted: start.follow,
            start: session,
            parked: HashMap::new(),
        }));
        Ok((app, notice))
    }

    /// Whether `n` at this step leaves the scene: at its last step, in a
    /// talk with a scene after this one. The scene to go to.
    fn scene_after_end(&self) -> Option<usize> {
        let t = self.talk.as_ref()?;
        if self.doc.step < self.doc.max_step() {
            return None;
        }
        t.next_shown()
    }

    /// `n` past the end of a scene: a typing or an effect that runs ends
    /// first (as at the last step of a document), and the next `n` goes
    /// to the next scene. `None` when `n` stays in the scene.
    pub(super) fn next_scene_key(&mut self, force: bool) -> Option<bool> {
        let to = self.scene_after_end()?;
        if self.typing.is_some() || self.effect.is_some() {
            return Some(self.step_to(self.doc.step));
        }
        Some(self.enter_scene(to, Landing::First, force, ":next!"))
    }

    /// The auto-advance of the last step of a scene that does not loop:
    /// to the next scene. `None` when it stays in the scene.
    pub(super) fn next_scene_auto(&mut self) -> Option<bool> {
        if self.doc.next_step_from(self.doc.step).is_some() {
            return None;
        }
        let to = self.scene_after_end()?;
        Some(self.enter_scene(to, Landing::First, false, ":next!"))
    }

    /// `p`, `:prev`, `step.prev` in a talk: to the nearest earlier step
    /// that does not advance by itself; with none (step 0, or an intro that
    /// advances all the way), to the last step of the scene before. In the
    /// first scene there is none: one step back, and that step waits for
    /// the next key (else it would advance again by itself).
    pub(super) fn prev_in_talk(&mut self, force: bool) -> Option<bool> {
        let t = self.talk.as_ref()?;
        let back = (0..self.doc.step)
            .rev()
            .find(|&s| self.doc.auto_advance_ms(s).is_none());
        Some(match (back, t.prev_shown()) {
            (Some(s), _) => self.step_to(s),
            (None, Some(to)) => self.enter_scene(to, Landing::Last, force, ":prev!"),
            (None, None) => {
                let from = self.doc.step;
                let loud = self.step_to(from.saturating_sub(1));
                if self.doc.step != from {
                    self.step_ready_at = None;
                }
                loud
            }
        })
    }

    /// `:scene X [STEP]` (`:scene!` goes on without the changes that are
    /// not saved): to scene X, a position or a name, at STEP (a number or a
    /// named step) or step 0. The scene on screen goes to that step, as
    /// `:goto`.
    pub(super) fn scene_command(&mut self, rest: &str, force: bool) -> Result<String, Error> {
        let Some(t) = self.talk.as_ref() else {
            return Err(Error::Invalid(
                ":scene works in a talk: open a master .tiza that lists the scenes".into(),
            ));
        };
        let mut words = rest.split_whitespace();
        let (Some(spec), step, None) = (words.next(), words.next(), words.next()) else {
            return Err(Error::Invalid(
                "usage: :scene POSITION|NAME [STEP]  (:scene! goes on without unsaved changes)"
                    .into(),
            ));
        };
        let index = t.loaded.resolve(spec)?;
        if index == t.index {
            self.goto_step(step.unwrap_or("0"))?;
            return Ok(self.step_label_line());
        }
        let landing = step.map_or(Landing::First, |s| Landing::Named(s.to_string()));
        let forced = format!(":scene! {spec}");
        if self.enter_scene(index, landing, force, &forced) {
            Ok(String::new())
        } else {
            Ok(self.step_label_line())
        }
    }

    /// [`App::try_enter_scene`], with what stops it on the bar: `true`
    /// when the bar says something (what stops it, or a note or a warning
    /// of the new scene), which a command then does not cover.
    /// `forced` names the command that goes on without the changes that
    /// are not saved.
    pub fn enter_scene(
        &mut self,
        index: usize,
        landing: Landing,
        force: bool,
        forced: &str,
    ) -> bool {
        match self.try_enter_scene(index, landing, force) {
            Ok(()) => self.message.is_some(),
            Err(e) if e.code() == "unsaved_changes" => {
                self.error(format!("{e}; :w saves them, {forced} goes on without them"));
                true
            }
            Err(e) => {
                self.error(e.to_string());
                true
            }
        }
    }

    /// Leaves the scene on screen for scene `index` of the talk, at
    /// `landing`. Changes that could not be saved stop it, unless `force`:
    /// the error has the code `unsaved_changes`. With `--no-save` nothing
    /// stops it: the scene, changes and undo, waits in memory until the
    /// presenter comes back. A scene that does not load, or a named step
    /// that it does not have, is an error that names the scene. On an
    /// error nothing changes.
    ///
    /// The next scene is built first, with `self` as it is; only then do
    /// the talk and the session move to it, in steps that cannot fail. Thus
    /// a panic while the next scene opens leaves the scene on screen whole.
    pub fn try_enter_scene(
        &mut self,
        index: usize,
        landing: Landing,
        force: bool,
    ) -> Result<(), Error> {
        let Some(t) = self.talk.as_ref() else {
            return Err(Error::Invalid("no talk is open".into()));
        };
        let here = scene_label(t.scene());
        // With `--no-save` the presenter chose not to save: the changes
        // wait in memory.
        let keep = self.no_save && self.dirty;
        if !force && !keep && self.dirty && (self.save_error.is_some() || !self.autosave) {
            return Err(Error::Because {
                message: format!("{here}: changes not saved"),
                inner: Box::new(Error::UnsavedChanges),
            });
        }
        let to = t.loaded.scenes.get(index).ok_or_else(|| {
            Error::SceneNotFound(format!("the talk has no scene at index {index}"))
        })?;
        if to.skip {
            return Err(Error::SceneNotFound(format!(
                "scene {} is skipped (`skip` in the talk)",
                to.position
            )));
        }
        let within = |e: Error| Error::Within {
            what: scene_label(to),
            inner: Box::new(e),
        };
        let disk = fs::load_document(&to.path).map_err(within)?;
        let disk_step = disk.step;
        // What waits in memory for this scene: only while its files are
        // the ones it was left with.
        let parked = t.parked.get(&to.path);
        let disk_fingerprint = parked.and_then(|_| fingerprint(&disk, &to.path));
        let fresh = parked.filter(|p| disk_fingerprint.as_ref() == Some(&p.fingerprint));
        let lost = parked.is_some_and(|p| p.doc.is_some()) && fresh.is_none();
        let kept = fresh.and_then(|p| p.doc.clone());
        let fresh = fresh.is_some();
        let disk_json = kept.as_ref().and_then(|_| fs::document_json(&disk).ok());
        let mut doc = kept.unwrap_or(disk);
        let step = match &landing {
            Landing::First => 0,
            Landing::Last => doc.max_step(),
            Landing::At(s) => (*s).min(doc.max_step()),
            Landing::Named(id) => doc.resolve_step(id).map_err(within)?,
        };
        let path = to.path.clone();
        let to_label = scene_label(to);
        let notice = fs::open_notice(&doc);
        let moved = step != doc.step;
        doc.step = step;
        let sync = moved.then(|| doc.sync_code_to_step()).flatten();
        // What the scene on screen leaves behind, said once on the next.
        let mut left = Vec::new();
        if let Some(s) = tiza_core::stale::close_summary(&self.doc) {
            left.push(s);
        }
        if !keep && let Some(p) = self.pause_code_unsaved() {
            left.push(format!("{p} is not kept"));
        }
        // Its history waits while its files stay as they are on disk now.
        let leave_fingerprint = (!self.dirty || keep)
            .then(|| {
                let on_disk = fs::load_document(&self.save_path).ok()?;
                fingerprint(&on_disk, &self.save_path)
            })
            .flatten();
        let follow_wanted = if self.source_file().is_some() {
            self.follow.is_some()
        } else {
            t.follow_wanted
        };

        let mut next = App::new(doc, path);
        next.disk_step = Some(disk_step);
        next.host_dark = self.host_dark;
        next.wide = self.wide;
        next.size = self.size;
        next.supervised = self.supervised;
        next.next_proposal = self.next_proposal;
        next.decided = self.decided;
        next.layouts.set(self.layouts.get());
        next.geometries.set(self.geometries.get());
        next.no_save = self.no_save;
        let mut start = t.start.clone();
        start.present = self.present;
        start.mouse = self.mouse;
        start.padding = self.padding_override;
        start.no_save = self.no_save;
        crate::open_theme(&mut next, &start);
        // Autosave as the presenter left it: a `scene put` of another
        // document held it for that scene only.
        next.autosave = (self.autosave || self.autosave_held) && !next.no_save;
        next.layout_stable = self.layout_stable;
        if let Some(dark) = self.host_dark {
            next.apply_host_dark(dark);
        }
        start.follow = follow_wanted && next.source_file().is_some();
        crate::open_rest(&mut next, &start, notice.as_ref(), &[]);
        if let Some(e) = sync {
            next.error(format!("moves: {e}"));
        }
        if landing != Landing::First {
            next.step_ready_at = None;
        }
        // The session's revision only grows: a reply about the scene of
        // before never names a revision of this one.
        next.revision += self.revision;
        // About the scene on screen first: a narrow bar cuts the end.
        let mut notes = Vec::new();
        if lost {
            notes.push(format!(
                "{to_label}: changes not kept, its files changed on disk"
            ));
        }
        if !left.is_empty() {
            notes.push(format!("left {here}: {}", left.join("; ")));
        }
        if !notes.is_empty() {
            let note = notes.join("; ");
            if next.message.is_some() {
                next.say_first(note);
            } else {
                next.info(note);
            }
        }

        // Test hook, only with the `test-hooks` feature: a panic when the
        // next scene is whole, before anything moves.
        #[cfg(feature = "test-hooks")]
        if crate::test_hook("talk:panic") {
            panic!("TIZA_TEST_HOOK at a scene change");
        }

        // From here nothing fails: the talk and the session move.
        let mut talk = self.talk.take().expect("a talk");
        talk.follow_wanted = follow_wanted;
        let here_path = talk.scene().path.clone();
        let restored = talk.parked.remove(&next.save_path);
        match leave_fingerprint {
            Some(fingerprint) => {
                let history = std::mem::take(&mut self.history);
                let doc = keep.then(|| self.doc.clone());
                talk.parked.insert(
                    here_path,
                    Parked {
                        history,
                        fingerprint,
                        doc,
                    },
                );
            }
            None => {
                talk.parked.remove(&here_path);
            }
        }
        if let Some(p) = restored.filter(|_| fresh) {
            next.history = p.history;
            next.mark_saved();
            if p.doc.is_some() {
                // Not saved: what quitting compares with is the file.
                next.dirty = true;
                next.saved_json = disk_json;
            }
        }
        let gone = "the scene changed before the review: the change was not applied";
        for p in self.proposals.drain(..) {
            let _ = p.reply.send(tiza_core::protocol::Response::err(
                p.req.id,
                "unavailable",
                gone,
            ));
        }
        self.checks
            .drop_all("check: the scene changed before the check ended; ask again");
        next.osc = std::mem::take(&mut self.osc);
        next.socket_path = self.socket_path.take();
        next.image_exporter = self.image_exporter.take();
        next.agent_deferred = std::mem::take(&mut self.agent_deferred);
        crate::note_save_path(&next.save_path);
        talk.index = index;
        next.talk = Some(talk);
        *self = next;
        Ok(())
    }

    /// The other scenes whose changes wait in memory (`--no-save`), in
    /// the order of the talk: `scene 3 (own)`. They are gone when Tiza
    /// closes.
    pub fn parked_changes(&self) -> Vec<String> {
        let Some(t) = self.talk.as_ref() else {
            return Vec::new();
        };
        t.loaded
            .scenes
            .iter()
            .filter(|s| t.parked.get(&s.path).is_some_and(|p| p.doc.is_some()))
            .map(scene_label)
            .collect()
    }

    /// What Tiza says on the terminal as it closes when other scenes have
    /// changes in memory.
    pub fn parked_summary(&self) -> Option<String> {
        let parked = self.parked_changes();
        (!parked.is_empty()).then(|| {
            format!(
                "quit without saving the changes in {} (--no-save)",
                parked.join(", ")
            )
        })
    }

    /// The position to resume at, written to the master once, as Tiza
    /// exits. `None` when nothing is to be said; else why it could not be
    /// written, for stderr.
    pub fn save_talk_position(&self) -> Option<String> {
        let t = self.talk.as_ref()?;
        if t.start.no_save {
            return None;
        }
        let s = t.scene();
        let at = At {
            position: s.position,
            name: s.name.clone(),
            step: self.doc.step.min(self.doc.max_step()),
        };
        if t.loaded.talk.at.as_ref() == Some(&at) {
            return None;
        }
        tiza_core::talk::save_at(&t.loaded.path, &t.loaded.talk.scenes, &at)
            .err()
            .map(|e| {
                format!(
                    "the position in the talk is not saved to {}: {e}",
                    t.loaded.path.display()
                )
            })
    }
}
