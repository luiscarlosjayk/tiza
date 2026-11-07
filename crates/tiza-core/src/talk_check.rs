//! `tiza check` of a talk: the check of each scene, as `tiza check SCENE`
//! gives it, and what is wrong with the master itself. One bad entry does
//! not hide the others: each one gives its own finding.

use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::Error;
use crate::check::{CheckOptions, Finding, Level, Report, check_path};

use crate::layout::MAX_WIDTH;
use crate::talk::{SceneEntry, Talk, TalkScene, entry_label, nested_talk, resolve_in};

/// What happened to a scene of the talk.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, schemars::JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum SceneStatus {
    /// Its check is in `report`.
    Checked,
    /// It has `skip`: not read.
    Skipped,
    /// Its file is not there (or cannot be read).
    Missing,
    /// Its file is a master too.
    Talk,
    /// An earlier scene that shows has the same file.
    Duplicate,
    /// Its entry in `scenes` is wrong (a `skip` that is not true or false,
    /// a file that is not a `.tiza`…).
    Invalid,
}

/// One scene of a [`TalkReport`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, schemars::JsonSchema)]
pub struct SceneCheck {
    /// Its entry in `scenes`, from 1.
    pub position: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// The path as the master gives it (relative to the master). Absent
    /// when the entry does not give one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    pub status: SceneStatus,
    /// The report of `tiza check FILE` on the scene, when `status` is
    /// `checked`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub report: Option<Report>,
}

/// A finding of a talk: the finding of a scene, with that scene, or one of
/// the master (without `scene`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, schemars::JsonSchema)]
pub struct TalkFinding {
    /// The position of the scene, from 1.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scene: Option<usize>,
    /// The file of the scene, as the master gives it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scene_file: Option<String>,
    #[serde(flatten)]
    pub finding: Finding,
}

/// Everything `tiza check` found about a talk.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, schemars::JsonSchema)]
pub struct TalkReport {
    /// No errors in the master or in a scene.
    pub ok: bool,
    pub errors: usize,
    pub warnings: usize,
    /// The narrowest terminal at which every scene shows its labels whole:
    /// the widest of the scenes. `None` when a scene was not checked, or
    /// when a scene clips a label at every width.
    pub clean_terminal_width: Option<usize>,
    /// The same, in columns of code (`tiza render --width`, which has no
    /// padding).
    pub clean_code_width: Option<usize>,
    /// Every finding: the master's first, then each scene's.
    pub findings: Vec<TalkFinding>,
    pub scenes: Vec<SceneCheck>,
}

/// Checks the master at `path` and each of its scenes with `opts`.
pub fn check_talk(path: &Path, opts: &CheckOptions) -> Result<TalkReport, Error> {
    let text = std::fs::read_to_string(path)?;
    let mut master: Vec<Finding> = Vec::new();
    let mut scenes: Vec<(SceneCheck, Option<Finding>)> = Vec::new();
    match read_master(&text) {
        Err(f) => master.push(*f),
        Ok(talk) => {
            if let Err(e) = talk.at {
                master.push(Finding::new(Level::Error, "talk", e.to_string()));
            }
            let base = crate::fs::document_dir(path);
            // The canonical files of the scenes that show so far.
            let mut shown: Vec<(usize, PathBuf)> = Vec::new();
            for (i, entry) in talk.entries.into_iter().enumerate() {
                scenes.push(check_entry(&base, entry, i + 1, opts, &mut shown));
            }
        }
    }
    Ok(TalkReport::new(master, scenes))
}

/// The master, read as [`Talk::entries_from_value`] reads it; a finding
/// when the master itself cannot be read.
fn read_master(text: &str) -> Result<crate::talk::TalkEntries, Box<Finding>> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut raw: serde_json::Value = serde_json::from_str(text).map_err(|e| {
        Box::new(
            Finding::new(Level::Error, "schema", format!("not valid JSON: {e}"))
                .line(Some(e.line() as u32)),
        )
    })?;
    crate::fs::migrate_document_value(&mut raw)
        .map_err(|e| Box::new(Finding::new(Level::Error, "schema", e.to_string())))?;
    Talk::entries_from_value(&raw)
        .map_err(|e| Box::new(Finding::new(Level::Error, "talk", e.to_string())))
}

/// One entry of `scenes`, at `position`.
fn check_entry(
    base: &Path,
    entry: Result<SceneEntry, Error>,
    position: usize,
    opts: &CheckOptions,
    shown: &mut Vec<(usize, PathBuf)>,
) -> (SceneCheck, Option<Finding>) {
    let failed = |message: String| Some(Finding::new(Level::Error, "talk", message));
    let entry = match entry {
        Ok(e) => e,
        Err(e) => {
            let scene = SceneCheck {
                position,
                name: None,
                file: None,
                status: SceneStatus::Invalid,
                report: None,
            };
            return (scene, failed(e.to_string()));
        }
    };
    let mut scene = SceneCheck {
        position,
        name: entry.name(),
        file: Some(entry.file.clone()),
        status: SceneStatus::Skipped,
        report: None,
    };
    if entry.skip {
        return (scene, None);
    }
    let joined = base.join(&entry.file);
    let canonical = match std::fs::canonicalize(&joined) {
        Ok(c) => c,
        Err(e) => {
            scene.status = SceneStatus::Missing;
            return (
                scene,
                failed(format!("{}: {e}", entry_label(&entry, position))),
            );
        }
    };
    if crate::fs::file_is_talk(&canonical) {
        scene.status = SceneStatus::Talk;
        return (scene, failed(nested_talk(&entry, position).to_string()));
    }
    if let Some((other, _)) = shown
        .iter()
        .find(|(_, p)| crate::talk::same_file(p, &canonical))
    {
        scene.status = SceneStatus::Duplicate;
        let message = crate::talk::same_file_twice(&entry, position, *other).to_string();
        return (scene, failed(message));
    }
    shown.push((position, canonical.clone()));
    match check_path(&joined, opts) {
        Ok(report) => {
            scene.status = SceneStatus::Checked;
            scene.report = Some(report);
            (scene, None)
        }
        Err(e) => {
            scene.status = SceneStatus::Missing;
            (
                scene,
                failed(format!("{}: {e}", entry_label(&entry, position))),
            )
        }
    }
}

impl TalkReport {
    fn new(master: Vec<Finding>, checked: Vec<(SceneCheck, Option<Finding>)>) -> TalkReport {
        let mut findings: Vec<TalkFinding> = master
            .into_iter()
            .map(|finding| TalkFinding {
                scene: None,
                scene_file: None,
                finding,
            })
            .collect();
        let mut scenes = Vec::with_capacity(checked.len());
        for (s, own) in checked {
            let of_scene = s.report.iter().flat_map(|r| r.findings.iter().cloned());
            for finding in own.into_iter().chain(of_scene) {
                findings.push(TalkFinding {
                    scene: Some(s.position),
                    scene_file: s.file.clone(),
                    finding,
                });
            }
            scenes.push(s);
        }
        let errors = findings
            .iter()
            .filter(|f| f.finding.level == Level::Error)
            .count();
        let warnings = findings.len() - errors;
        let (clean_terminal_width, clean_code_width) = clean_widths(&scenes);
        TalkReport {
            ok: errors == 0,
            errors,
            warnings,
            clean_terminal_width,
            clean_code_width,
            findings,
            scenes,
        }
    }

    /// The scene that clips a label at every width, when one does.
    pub fn never_clean(&self) -> Option<&SceneCheck> {
        self.scenes.iter().find(|s| {
            s.report
                .as_ref()
                .is_some_and(|r| r.max_step.is_some() && r.min_clean_width.is_none())
        })
    }

    /// The report for a terminal: each scene's lines with its file in
    /// front, then the summary of the talk. `name` is the master as the
    /// command line gave it.
    pub fn to_text(&self, name: &str) -> String {
        let mut out = String::new();
        for f in self.findings.iter().filter(|f| f.scene.is_none()) {
            out.push_str(&format!("{name}: {}\n", f.finding.to_line()));
        }
        // A scene file as the command line names it: next to the master.
        let dir = Path::new(name)
            .parent()
            .filter(|d| !d.as_os_str().is_empty());
        for s in &self.scenes {
            let label = scene_label(s, dir);
            match (&s.status, &s.report) {
                (SceneStatus::Checked, Some(r)) => {
                    for f in &r.findings {
                        out.push_str(&format!("{label}: {}\n", f.to_line()));
                    }
                    // The summary starts with the name already; each line
                    // after it gets the name in front.
                    for (i, line) in r.summary_text(&label).lines().enumerate() {
                        if i == 0 {
                            out.push_str(line);
                        } else {
                            out.push_str(&format!("{label}: {line}"));
                        }
                        out.push('\n');
                    }
                }
                (SceneStatus::Skipped, _) => {
                    out.push_str(&format!("{label}: skipped (`skip` in the talk)\n"));
                }
                _ => {
                    for f in self.findings.iter().filter(|f| f.scene == Some(s.position)) {
                        out.push_str(&format!("{label}: {}\n", f.finding.to_line()));
                    }
                }
            }
        }
        let plural = |n: usize, w: &str| format!("{n} {w}{}", if n == 1 { "" } else { "s" });
        let skipped = self
            .scenes
            .iter()
            .filter(|s| s.status == SceneStatus::Skipped)
            .count();
        out.push_str(&format!(
            "{name}: {}{}, {}, {}\n",
            plural(self.scenes.len(), "scene"),
            if skipped > 0 {
                format!(" ({skipped} skipped)")
            } else {
                String::new()
            },
            plural(self.errors, "error"),
            plural(self.warnings, "warning"),
        ));
        if let Some(s) = self.never_clean() {
            out.push_str(&format!(
                "{name}: {} clips a label at every width up to {MAX_WIDTH}: shorten its labels\n",
                scene_label(s, dir)
            ));
        } else if let (Some(t), Some(c)) = (self.clean_terminal_width, self.clean_code_width) {
            out.push_str(&format!(
                "{name}: every scene shows its labels whole at {t} or wider ({c} columns of code: `tiza render --width {c}`)\n"
            ));
        }
        out
    }
}

/// How the text names a scene: its file, else its position.
fn scene_label(s: &SceneCheck, dir: Option<&Path>) -> String {
    match &s.file {
        Some(f) => dir.map_or_else(|| f.clone(), |d| d.join(f).display().to_string()),
        None => format!("scene {}", s.position),
    }
}

/// The widest clean width of the scenes, in columns of the terminal and
/// in columns of code (each scene has its own padding, so the two may come
/// from two scenes). `None` when a scene that shows was not checked or
/// has no clean width.
fn clean_widths(scenes: &[SceneCheck]) -> (Option<usize>, Option<usize>) {
    let mut terminal: Option<usize> = Some(0);
    let mut code: Option<usize> = Some(0);
    for s in scenes {
        match (&s.status, &s.report) {
            (SceneStatus::Skipped, _) => {}
            (SceneStatus::Checked, Some(r)) => {
                // A scene that did not load has no width.
                if r.max_step.is_none() {
                    return (None, None);
                }
                let t = r
                    .terminal
                    .as_ref()
                    .map_or(r.min_clean_width, |t| t.min_clean_width);
                terminal = terminal.zip(t).map(|(a, b)| a.max(b));
                code = code.zip(r.min_clean_width).map(|(a, b)| a.max(b));
            }
            _ => return (None, None),
        }
    }
    (terminal.filter(|w| *w > 0), code.filter(|w| *w > 0))
}

/// The scene of `tiza check MASTER --scene SPEC` (or of `tiza render`, the
/// `command`): the index of its entry and its path. The other entries are
/// not read, so a scene whose file is missing can be named too, and the
/// scene does not need the others. A skipped scene is refused with how to
/// use it anyway, a scene that is a master as in the talk.
/// It returns the index, the path of the scene next to the master and the
/// path as the master gives it.
pub fn resolve_scene(
    path: &Path,
    spec: &str,
    command: &str,
) -> Result<(usize, PathBuf, String), Error> {
    let text = std::fs::read_to_string(path)?;
    let text = text.strip_prefix('\u{feff}').unwrap_or(&text);
    let mut raw: serde_json::Value = serde_json::from_str(text)?;
    crate::fs::migrate_document_value(&mut raw)?;
    let talk = Talk::entries_from_value(&raw)?;
    let base = crate::fs::document_dir(path);
    let scenes: Vec<TalkScene> = talk
        .entries
        .iter()
        .enumerate()
        .map(|(i, e)| TalkScene {
            position: i + 1,
            name: e.as_ref().ok().and_then(SceneEntry::name),
            path: e.as_ref().map(|e| base.join(&e.file)).unwrap_or_default(),
            skip: e.as_ref().is_ok_and(|e| e.skip),
            doc: None,
        })
        .collect();
    match resolve_in(&scenes, spec) {
        Ok(i) => match &talk.entries[i] {
            Ok(e) if crate::fs::file_is_talk(&scenes[i].path) => Err(nested_talk(e, i + 1)),
            Ok(e) => Ok((i, scenes[i].path.clone(), e.file.clone())),
            Err(e) => Err(Error::Invalid(e.to_string())),
        },
        Err(e) => {
            // A skipped scene: the error says so; add how to check it.
            let spec = spec.trim();
            let skipped = scenes.iter().find(|s| {
                s.skip && (s.position.to_string() == spec || s.name.as_deref() == Some(spec))
            });
            match (skipped, &talk.entries) {
                (Some(s), entries) => {
                    let file = entries[s.position - 1]
                        .as_ref()
                        .map(|e| e.file.clone())
                        .unwrap_or_default();
                    // The path as the command line gave the master.
                    let shown = match path.parent().filter(|d| !d.as_os_str().is_empty()) {
                        Some(d) => d.join(&file),
                        None => PathBuf::from(&file),
                    };
                    let verb = if command == "check" {
                        "check"
                    } else {
                        "export"
                    };
                    Err(Error::SceneNotFound(format!(
                        "{e}; to {verb} it anyway: `tiza {command} {}`",
                        shown.display()
                    )))
                }
                (None, _) => Err(e),
            }
        }
    }
}
