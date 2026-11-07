//! A talk: a master `.tiza` that plays several `.tiza` documents (its
//! scenes) one after the other. The master has `talk` instead of `code`:
//!
//! ```json
//! {"schema": 2, "talk": {"title": "Ownership",
//!   "scenes": ["scenes/intro.tiza", {"file": "scenes/move.md.tiza", "name": "move", "skip": true}],
//!   "at": {"position": 2, "name": "move", "step": 9}}}
//! ```
//!
//! Each scene is a [`Document`] that does not change: it has its own code,
//! steps and layout, and it opens alone as before. A scene is loaded here
//! exactly as `tiza SCENE.tiza` loads it ([`crate::fs::load_document`]), so
//! a scene in a talk is the scene alone.
//!
//! A scene has a position (its entry in `scenes`, from 1, skipped entries
//! included) and a name: its `name`, else its file name up to the first
//! dot. Names may repeat: a name goes to the first scene with it, and a
//! position reaches any scene.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::Error;
use crate::model::{Document, single_line};
use crate::steps::{WordRules, check_word};

const NAME_RULES: WordRules = WordRules {
    number: "a position in the talk",
    command: "tiza --scene",
    plural: "names",
    example: "shared-borrow",
};

/// The `talk` of a master.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Talk {
    pub title: Option<String>,
    pub scenes: Vec<SceneEntry>,
    /// Where the talk was when it last closed. Tiza writes it.
    pub at: Option<At>,
}

/// One entry of `scenes`: a path, or an object with `file`, `name` and
/// `skip`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SceneEntry {
    /// The path of the scene, relative to the master.
    pub file: String,
    pub name: Option<String>,
    /// A skipped scene is not read and not shown.
    pub skip: bool,
}

/// Where a talk was: the position of the scene (from 1), its name then
/// (`None` when it had no name), and the step.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct At {
    pub position: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub step: u32,
}

/// A scene of a loaded talk.
#[derive(Debug, Clone)]
pub struct TalkScene {
    /// Its entry in `scenes`, from 1.
    pub position: usize,
    /// Its `name`, else its file name up to the first dot. `None` when
    /// that is not a name (all digits, a blank…): the scene is reached by
    /// its position only.
    pub name: Option<String>,
    /// The file, canonical.
    pub path: PathBuf,
    pub skip: bool,
    /// The document as it was read at open. `None` for a skipped scene.
    pub doc: Option<Document>,
}

/// A master and its scenes, every scene that is not skipped read.
#[derive(Debug, Clone)]
pub struct LoadedTalk {
    pub path: PathBuf,
    pub talk: Talk,
    pub scenes: Vec<TalkScene>,
}

/// `true` when a `.tiza` (as JSON) is a master: it has `talk`.
pub fn is_talk(raw: &Value) -> bool {
    raw.get("talk").is_some()
}

/// The message for a master given where a document is needed.
pub fn not_a_document() -> Error {
    Error::Invalid(
        "this file is a talk (it has `talk`), not a document: open it with `tiza FILE` or `tiza FILE --present`"
            .into(),
    )
}

impl Talk {
    /// Reads the `talk` of a master, already migrated. The errors name the
    /// key or the entry.
    pub fn from_value(raw: &Value) -> Result<Talk, Error> {
        if raw.get("code").is_some() {
            return Err(Error::Invalid(
                "the file has both `code` and `talk`: a document has `code`, a talk has `talk`; keep one"
                    .into(),
            ));
        }
        let Some(talk) = raw.get("talk").and_then(Value::as_object) else {
            return Err(Error::Invalid(
                "`talk` must be an object with `scenes`".into(),
            ));
        };
        for key in talk.keys() {
            if !matches!(key.as_str(), "title" | "scenes" | "at") {
                return Err(Error::Invalid(format!(
                    "talk: unknown key `{key}` (a talk has `title`, `scenes` and `at`)"
                )));
            }
        }
        let title = match talk.get("title") {
            None | Some(Value::Null) => None,
            Some(Value::String(t)) => {
                Some(single_line(t).trim().to_string()).filter(|t| !t.is_empty())
            }
            Some(_) => return Err(Error::Invalid("talk: `title` must be a text".into())),
        };
        let Some(list) = talk.get("scenes").and_then(Value::as_array) else {
            return Err(Error::Invalid(
                "talk: `scenes` must be a list of .tiza files".into(),
            ));
        };
        let scenes = list
            .iter()
            .enumerate()
            .map(|(i, v)| SceneEntry::from_value(v, i + 1))
            .collect::<Result<Vec<_>, _>>()?;
        if scenes.iter().all(|s| s.skip) {
            return Err(Error::Invalid(if scenes.is_empty() {
                "talk: `scenes` is empty; give at least one .tiza file".into()
            } else {
                "talk: every scene has `skip`; at least one scene must show".into()
            }));
        }
        let at = match talk.get("at") {
            None | Some(Value::Null) => None,
            Some(v) => {
                Some(At::deserialize(v).map_err(|e| Error::Invalid(format!("talk: `at`: {e}")))?)
            }
        };
        Ok(Talk { title, scenes, at })
    }
}

impl SceneEntry {
    fn from_value(v: &Value, position: usize) -> Result<SceneEntry, Error> {
        let what = format!("talk: scene {position}");
        let (file, name, skip) = match v {
            Value::String(f) => (f.clone(), None, false),
            Value::Object(o) => {
                for key in o.keys() {
                    if !matches!(key.as_str(), "file" | "name" | "skip") {
                        return Err(Error::Invalid(format!(
                            "{what}: unknown key `{key}` (a scene has `file`, `name` and `skip`)"
                        )));
                    }
                }
                let file = match o.get("file") {
                    Some(Value::String(f)) => f.clone(),
                    _ => {
                        return Err(Error::Invalid(format!(
                            "{what}: `file` must be the path of a .tiza file"
                        )));
                    }
                };
                let name = match o.get("name") {
                    None | Some(Value::Null) => None,
                    Some(Value::String(n)) => Some(n.trim().to_string()),
                    Some(_) => {
                        return Err(Error::Invalid(format!("{what}: `name` must be a text")));
                    }
                };
                let skip = match o.get("skip") {
                    None | Some(Value::Null) => false,
                    Some(Value::Bool(b)) => *b,
                    Some(_) => {
                        return Err(Error::Invalid(format!(
                            "{what}: `skip` must be true or false"
                        )));
                    }
                };
                (file, name, skip)
            }
            _ => {
                return Err(Error::Invalid(format!(
                    "{what} must be the path of a .tiza file, or an object with `file`"
                )));
            }
        };
        if file.trim().is_empty() {
            return Err(Error::Invalid(format!("{what}: the path is empty")));
        }
        if !file.ends_with(&format!(".{}", crate::fs::EXT)) {
            return Err(Error::Invalid(format!(
                "{what}: {file} is not a .tiza file; a talk plays .tiza documents"
            )));
        }
        if let Some(n) = &name {
            if n.is_empty() {
                return Err(Error::Invalid(format!(
                    "{what} has an empty `name` (leave it out instead)"
                )));
            }
            check_word(n, &format!("{what}: name {n:?}"), &NAME_RULES)?;
        }
        Ok(SceneEntry { file, name, skip })
    }

    /// The name of the scene: its `name`, else its file name up to the
    /// first dot, when that obeys the rules of a name.
    pub fn name(&self) -> Option<String> {
        if let Some(n) = &self.name {
            return Some(n.clone());
        }
        let file = Path::new(&self.file).file_name()?.to_string_lossy();
        let stem = file.split('.').next()?.trim();
        (!stem.is_empty() && check_word(stem, "", &NAME_RULES).is_ok()).then(|| stem.to_string())
    }
}

impl LoadedTalk {
    /// The scenes that show, in order.
    pub fn shown(&self) -> impl Iterator<Item = &TalkScene> {
        self.scenes.iter().filter(|s| !s.skip)
    }

    /// The scene that `spec` names (an index into `scenes`): digits are a
    /// position, anything else the first scene with that name.
    pub fn resolve(&self, spec: &str) -> Result<usize, Error> {
        resolve_in(&self.scenes, spec)
    }

    /// Where the talk opens from its `at`: the scene at that position when
    /// it still has that name and shows; else the first scene that shows
    /// with that name; else the first scene that shows, at step 0. Returns
    /// the index and the step.
    pub fn resume(&self) -> (usize, u32) {
        resume_in(&self.scenes, self.talk.at.as_ref())
    }
}

/// [`LoadedTalk::resolve`] over a list of scenes.
pub fn resolve_in(scenes: &[TalkScene], spec: &str) -> Result<usize, Error> {
    let spec = spec.trim();
    let skipped: Vec<String> = scenes
        .iter()
        .filter(|s| s.skip)
        .map(|s| s.position.to_string())
        .collect();
    if !spec.is_empty() && spec.chars().all(|c| c.is_ascii_digit()) {
        let n: usize = spec.parse().unwrap_or(0);
        let skipped_note = if skipped.is_empty() {
            String::new()
        } else {
            format!(", and {} skipped", skipped.join(", "))
        };
        return match scenes.iter().position(|s| s.position == n) {
            Some(i) if !scenes[i].skip => Ok(i),
            Some(_) => Err(Error::SceneNotFound(format!(
                "scene {n} is skipped (`skip` in the talk); the talk has scenes 1 to {}{skipped_note}",
                scenes.len()
            ))),
            None => Err(Error::SceneNotFound(format!(
                "the talk has no scene {spec}: it has scenes 1 to {}{skipped_note}",
                scenes.len()
            ))),
        };
    }
    if let Some(i) = scenes
        .iter()
        .position(|s| !s.skip && s.name.as_deref() == Some(spec))
    {
        return Ok(i);
    }
    let mut names: Vec<&str> = Vec::new();
    for s in scenes.iter().filter(|s| !s.skip) {
        if let Some(n) = s.name.as_deref()
            && !names.contains(&n)
        {
            names.push(n);
        }
    }
    let skipped_here = scenes
        .iter()
        .any(|s| s.skip && s.name.as_deref() == Some(spec));
    Err(Error::SceneNotFound(if skipped_here {
        format!("scene {spec:?} is skipped (`skip` in the talk)")
    } else if names.is_empty() {
        format!(
            "no scene is named {spec:?}; give a position (1 to {})",
            scenes.len()
        )
    } else {
        format!(
            "no scene is named {spec:?}; the names are {}, or give a position (1 to {})",
            names.join(", "),
            scenes.len()
        )
    }))
}

/// [`LoadedTalk::resume`] over a list of scenes.
pub fn resume_in(scenes: &[TalkScene], at: Option<&At>) -> (usize, u32) {
    let first = scenes.iter().position(|s| !s.skip).unwrap_or(0);
    let Some(at) = at else {
        return (first, 0);
    };
    if let Some(i) = scenes.iter().position(|s| s.position == at.position)
        && !scenes[i].skip
        && at.name.is_some()
        && scenes[i].name == at.name
    {
        return (i, at.step);
    }
    // No name then: only the exact position, and only when that scene has
    // no name either (a file name that is all digits, at the same place).
    if at.name.is_none()
        && let Some(i) = scenes.iter().position(|s| s.position == at.position)
        && !scenes[i].skip
        && scenes[i].name.is_none()
    {
        return (i, at.step);
    }
    if let Some(name) = &at.name
        && let Some(i) = scenes
            .iter()
            .position(|s| !s.skip && s.name.as_ref() == Some(name))
    {
        return (i, at.step);
    }
    (first, 0)
}

/// Reads a master and every scene that shows. An error names the scene
/// (its position and name) and its path.
pub fn load_talk(path: &Path) -> Result<LoadedTalk, Error> {
    let text = std::fs::read_to_string(path)?;
    let text = text.strip_prefix('\u{feff}').unwrap_or(&text);
    let mut raw: Value = serde_json::from_str(text)?;
    crate::fs::migrate_document_value(&mut raw)?;
    let talk = Talk::from_value(&raw)?;
    let base = crate::fs::document_dir(path);
    let mut scenes: Vec<TalkScene> = Vec::with_capacity(talk.scenes.len());
    for (i, entry) in talk.scenes.iter().enumerate() {
        let scene = load_entry(&base, entry, i + 1)?;
        if !scene.skip
            && let Some(other) = scenes
                .iter()
                .find(|o| !o.skip && same_file(&o.path, &scene.path))
        {
            return Err(Error::Invalid(format!(
                "{}: the same file as scene {}; a talk shows a file once (copy it to show it again)",
                entry_label(entry, scene.position),
                other.position
            )));
        }
        scenes.push(scene);
    }
    Ok(LoadedTalk {
        path: path.to_path_buf(),
        talk,
        scenes,
    })
}

/// `scene 3 (borrow, borrow.tiza)`: how an error names an entry.
fn entry_label(entry: &SceneEntry, position: usize) -> String {
    match entry.name() {
        Some(n) => format!("scene {position} ({n}, {})", entry.file),
        None => format!("scene {position} ({})", entry.file),
    }
}

/// Reads one entry of a talk at `position` (from 1), with its path
/// relative to `base` (the folder of the master): its document, or nothing
/// for an entry that `skip` leaves out. A scene that shows has its
/// canonical path. An error names the entry. It does not compare the file
/// with the other scenes: see [`same_file`].
pub fn load_entry(base: &Path, entry: &SceneEntry, position: usize) -> Result<TalkScene, Error> {
    let name = entry.name();
    let joined = base.join(&entry.file);
    if entry.skip {
        return Ok(TalkScene {
            position,
            name,
            path: joined,
            skip: true,
            doc: None,
        });
    }
    let within = |e: Error| Error::Within {
        what: entry_label(entry, position),
        inner: Box::new(e),
    };
    let canonical = std::fs::canonicalize(&joined).map_err(|e| within(Error::Io(e)))?;
    let doc = crate::fs::load_document(&canonical).map_err(within)?;
    Ok(TalkScene {
        position,
        name,
        path: canonical,
        skip: false,
        doc: Some(doc),
    })
}

/// Two canonical paths name the same file: equal, or the same file on a
/// file system that ignores case (APFS by default).
pub fn same_file(a: &Path, b: &Path) -> bool {
    if a == b {
        return true;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if let (Ok(x), Ok(y)) = (std::fs::metadata(a), std::fs::metadata(b)) {
            return x.dev() == y.dev() && x.ino() == y.ino();
        }
    }
    false
}

/// Writes `at` into the master on disk and nothing else: see
/// [`update_talk`].
pub fn save_at(path: &Path, scenes: &[SceneEntry], at: &At) -> Result<(), Error> {
    update_talk(path, scenes, "at", serde_json::to_value(at)?)
}

/// The one writer of a master: the file is read again, and only `key` of
/// `talk` (`title`, `scenes` or `at`) changes to `value`. The other keys,
/// the form of each entry and the keys that this version does not know
/// stay; the file is written in the layout of Tiza: `schema`, then `talk`
/// (`title`, `scenes`, `at`), then the other keys. When the `scenes` on
/// disk are not `scenes` any more (the master was edited while the talk
/// ran), nothing is written: the error says so.
pub fn update_talk(
    path: &Path,
    scenes: &[SceneEntry],
    key: &str,
    value: Value,
) -> Result<(), Error> {
    let text = std::fs::read_to_string(path)?;
    let text = text.strip_prefix('\u{feff}').unwrap_or(&text);
    let mut raw: Value = serde_json::from_str(text)?;
    let on_disk = Talk::from_value(&raw)?;
    if on_disk.scenes != scenes {
        return Err(Error::Invalid(
            "its list of scenes changed while the talk ran".into(),
        ));
    }
    let Value::Object(mut top) = raw else {
        return Err(Error::Invalid("a master must be a JSON object".into()));
    };
    let Some(Value::Object(mut talk)) = top.remove("talk") else {
        return Err(Error::Invalid(
            "`talk` must be an object with `scenes`".into(),
        ));
    };
    if !matches!(key, "title" | "scenes" | "at") {
        return Err(Error::Invalid(format!("talk: unknown key `{key}`")));
    }
    talk.insert(key.into(), value);
    let at = talk.remove("at").map(|v| match At::deserialize(&v) {
        Ok(at) => AtOut::Typed(at),
        Err(_) => AtOut::Raw(v),
    });
    let out = MasterOut {
        schema: top.remove("schema"),
        talk: TalkOut {
            title: talk.remove("title"),
            scenes: talk.remove("scenes").unwrap_or(Value::Null),
            at,
        },
        rest: top,
    };
    let out = serde_json::to_string_pretty(&out)?;
    crate::fs::write_atomically(path, out.as_bytes())
}

/// A master as [`update_talk`] writes it: its keys in this order, then the
/// keys that this version does not know.
#[derive(Serialize)]
struct MasterOut {
    #[serde(skip_serializing_if = "Option::is_none")]
    schema: Option<Value>,
    talk: TalkOut,
    #[serde(flatten)]
    rest: serde_json::Map<String, Value>,
}

#[derive(Serialize)]
struct TalkOut {
    #[serde(skip_serializing_if = "Option::is_none")]
    title: Option<Value>,
    scenes: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    at: Option<AtOut>,
}

/// `at` with its fields in their order (`position`, `name`, `step`).
#[derive(Serialize)]
#[serde(untagged)]
enum AtOut {
    Typed(At),
    Raw(Value),
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn scene(position: usize, name: Option<&str>, skip: bool) -> TalkScene {
        TalkScene {
            position,
            name: name.map(str::to_string),
            path: PathBuf::from(format!("s{position}.tiza")),
            skip,
            doc: None,
        }
    }

    fn entry(file: &str) -> SceneEntry {
        SceneEntry {
            file: file.into(),
            name: None,
            skip: false,
        }
    }

    #[test]
    fn a_name_comes_from_the_entry_or_from_the_file_up_to_the_first_dot() {
        assert_eq!(
            entry("scenes/06-move.md.tiza").name().as_deref(),
            Some("06-move")
        );
        assert_eq!(entry("intro.tiza").name().as_deref(), Some("intro"));
        let named = SceneEntry {
            name: Some("move".into()),
            ..entry("scenes/06-move.md.tiza")
        };
        assert_eq!(named.name().as_deref(), Some("move"));
        // Not a name: all digits, a blank, a leading dash, nothing before the dot.
        for file in ["07.tiza", "my scene.tiza", "-x.tiza", ".tiza"] {
            assert_eq!(entry(file).name(), None, "{file}");
        }
    }

    #[test]
    fn digits_are_a_position_and_a_name_goes_to_the_first_scene_with_it() {
        let scenes = [
            scene(1, Some("intro"), false),
            scene(2, Some("move"), true),
            scene(3, Some("move"), false),
            scene(4, Some("move"), false),
            scene(5, None, false),
        ];
        assert_eq!(
            resolve_in(&scenes, "move").unwrap(),
            2,
            "the first that shows"
        );
        assert_eq!(
            resolve_in(&scenes, "4").unwrap(),
            3,
            "a position reaches the second"
        );
        assert_eq!(
            resolve_in(&scenes, "5").unwrap(),
            4,
            "a scene without a name"
        );
        assert_eq!(resolve_in(&scenes, " intro ").unwrap(), 0);
        let err = |s: &str| resolve_in(&scenes, s).unwrap_err();
        assert!(
            err("2").to_string().contains("scene 2 is skipped"),
            "{}",
            err("2")
        );
        assert!(err("0").to_string().contains("no scene 0"), "{}", err("0"));
        assert!(
            err("6")
                .to_string()
                .contains("scenes 1 to 5, and 2 skipped"),
            "{}",
            err("6")
        );
        assert!(
            err("07").to_string().contains("no scene 07"),
            "{}",
            err("07")
        );
        let unknown = err("borrow").to_string();
        assert!(unknown.contains("the names are intro, move"), "{unknown}");
        assert_eq!(err("borrow").code(), "not_found");
    }

    #[test]
    fn at_opens_the_exact_scene_else_the_first_with_its_name_else_the_first() {
        let at = |position, name: Option<&str>, step| At {
            position,
            name: name.map(str::to_string),
            step,
        };
        let scenes = [
            scene(1, Some("a"), true),
            scene(2, Some("move"), false),
            scene(3, Some("move"), false),
            scene(4, None, false),
        ];
        assert_eq!(resume_in(&scenes, None), (1, 0), "the first that shows");
        assert_eq!(
            resume_in(&scenes, Some(&at(3, Some("move"), 9))),
            (2, 9),
            "exact, names repeat"
        );
        assert_eq!(
            resume_in(&scenes, Some(&at(4, Some("move"), 9))),
            (1, 9),
            "moved: the first move"
        );
        assert_eq!(
            resume_in(&scenes, Some(&at(1, Some("a"), 2))),
            (1, 0),
            "skipped now: the first"
        );
        assert_eq!(
            resume_in(&scenes, Some(&at(3, Some("gone"), 2))),
            (1, 0),
            "renamed: the first"
        );
        assert_eq!(
            resume_in(&scenes, Some(&at(4, None, 5))),
            (3, 5),
            "no name, same place"
        );
        // No name: never another scene without a name at another place.
        assert_eq!(resume_in(&scenes, Some(&at(2, None, 5))), (1, 0));
    }

    #[test]
    fn the_talk_names_what_is_wrong() {
        let err = |v: Value| Talk::from_value(&v).unwrap_err().to_string();
        assert!(
            err(json!({"code": [], "talk": {"scenes": ["a.tiza"]}}))
                .contains("both `code` and `talk`")
        );
        assert!(err(json!({"talk": {"scenes": []}})).contains("`scenes` is empty"));
        assert!(
            err(json!({"talk": {"scenes": [{"file": "a.tiza", "skip": true}]}}))
                .contains("every scene has `skip`")
        );
        assert!(
            err(json!({"talk": {"scenes": [{"file": "a.tiza", "skip": "yes"}]}}))
                .contains("scene 1: `skip` must be true or false")
        );
        assert!(err(json!({"talk": {"scenes": ["a.md"]}})).contains("a.md is not a .tiza file"));
        assert!(
            err(json!({"talk": {"scenes": [{"file": "a.tiza", "nmae": "x"}]}}))
                .contains("unknown key `nmae`")
        );
        assert!(
            err(json!({"talk": {"scenes": ["a.tiza"], "titel": "x"}}))
                .contains("unknown key `titel`")
        );
        assert!(
            err(json!({"talk": {"scenes": [{"file": "a.tiza", "name": "12"}]}}))
                .contains("is all digits; a number means a position in the talk")
        );
        assert!(
            err(json!({"talk": {"scenes": [{"file": "a.tiza", "name": "a b"}]}}))
                .contains("has blanks")
        );
        assert!(
            err(json!({"talk": {"scenes": [{"file": "a.tiza", "name": " "}]}}))
                .contains("empty `name`")
        );
        assert!(
            err(json!({"talk": {"scenes": ["a.tiza"], "at": {"position": 1}}}))
                .contains("talk: `at`")
        );
        let ok = Talk::from_value(&json!({"talk": {"title": " T\n", "scenes": ["a.tiza", {"file": "b.tiza", "name": "b", "skip": false}]}})).unwrap();
        assert_eq!(ok.title.as_deref(), Some("T"));
        assert_eq!(ok.scenes[1].name.as_deref(), Some("b"));
    }
}
