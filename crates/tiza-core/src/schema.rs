//! JSON Schemas of the `.tiza` document and of the local API, generated
//! from the Rust types (schemars). `tiza schema` prints them; the copies
//! committed under `schema/` are checked by a test so they never go stale.

use schemars::{JsonSchema, schema_for};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::model::{Document, MarkStyle};
use crate::protocol::{
    AddParams, AnnotationMutated, IdParams, METHODS, MoveIndexParams, MoveList, MoveMutated,
    Mutated, PatchParams, PutParams, Scene, SceneChanged, SceneGetParams, Status, StepGotoParams,
    StepParams, StepReply, UpdateParams,
};

/// Parameters of `supervise.set`.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SuperviseParams {
    /// `true`: every mutating request waits for the presenter's y/n.
    pub enabled: bool,
}

/// Result of `supervise.set` / `supervise.status`.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct SuperviseStatus {
    pub supervised: bool,
    /// Proposals waiting for a decision: `{id, method, summary, age_ms}`.
    pub pending: Vec<PendingProposal>,
    pub accepted: u64,
    pub rejected: u64,
    pub timeout_secs: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PendingProposal {
    pub id: u64,
    pub method: String,
    pub summary: String,
    pub age_ms: u64,
}

/// Parameters of `style.set`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct StyleSetParams {
    pub name: String,
    #[serde(flatten)]
    pub style: MarkStyle,
}

/// Parameters of `scene.clear`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ClearParams {
    /// Only this step; omit to clear everything.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "u32")]
    pub step: Option<u32>,
}

/// Parameters of `render.text`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct RenderParams {
    /// Columns of the take (the layout of presentation mode, as `tiza render`
    /// writes it). Without it, the width of the screen's code area, as
    /// `visible` judges it (`check` at the same width agrees); the reply's
    /// `width` says which.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<usize>,
}

/// Parameters of `check`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CheckParams {
    /// Columns to check the labels and the code lines at; defaults to the
    /// `width` of the config file of the instance, else 100.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<usize>,
}

/// Parameters of `focus.set`: omit both to clear the focus.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct FocusParams {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from_line: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub to_line: Option<u32>,
}

/// Parameters of `theme.set`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ThemeParams {
    /// `dark`, `light`, `terminal`, `auto` (follow the terminal) or `toggle`
    /// (the default when omitted).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub theme: Option<String>,
}

/// Parameters of `follow.set`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct FollowParams {
    /// `true` to reload when the source file is saved, `false` to stop.
    pub enabled: bool,
}

/// Result of `follow.status` and `follow.set`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct FollowStatus {
    pub enabled: bool,
    /// Absolute path of the followed (or followable) file; null when embedded.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reloads: Option<u64>,
    /// With follow on: how many times Tiza read the file to load it (after
    /// a save, when follow starts, at the resume step), also the reads that
    /// load nothing. Saves that come close together can be one read.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reads: Option<u64>,
    /// With follow on: the sha256 of the file as Tiza last read it, in
    /// lowercase hex (what `shasum -a 256 FILE` prints). It is the hash of
    /// the whole file, also when `code` is a range of it. Null when Tiza
    /// could not read the file, for example when the file is removed. When
    /// it is the hash of the file you saved, Tiza read your save. A save of
    /// the same bytes does not change it: then `reads` increases.
    #[serde(default)]
    pub hash: Option<String>,
    /// Time of the last reload, `HH:MM:SSZ`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_reload: Option<String>,
    /// The file is currently missing on disk (waiting for it to come back).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub missing: Option<bool>,
    /// A `code.patch` is applied at the current step: a save does not load
    /// until the presenter is at `resume_step` or earlier.
    pub paused: bool,
    /// Paused, and the file on disk holds other code than the screen: a
    /// change waits to load.
    pub pending: bool,
    /// While paused: the last step before the first patch, where a change
    /// that waits loads (an earlier step loads it too).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resume_step: Option<u32>,
    /// Why follow is off, when something other than `follow.set` turned it
    /// off: a paste (`the code is pasted, not main.rs; :follow on reloads
    /// main.rs`), or a file watcher that could not start (`cannot watch
    /// /home/me/demo: …`). With follow on, why a save does not load now:
    /// a patch is applied (`paused; loads at step 1 or earlier`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// What `follow.set` did, in words.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

/// Parameters of `code.get` (none).
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct NoParams {}

/// One method of the local API with the schema of its params and result.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct MethodSchema {
    pub method: String,
    pub description: String,
    pub params: Value,
    pub result: Value,
}

/// The whole API contract, as `api.schema` and `tiza schema api` return it.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ApiSchema {
    pub protocol: u32,
    /// Shape of every request and response line.
    pub request: Value,
    pub response: Value,
    pub methods: Vec<MethodSchema>,
}

fn schema<T: JsonSchema>() -> Value {
    serde_json::to_value(schema_for!(T)).unwrap_or(Value::Null)
}

/// JSON Schema of the `.tiza` document.
pub fn document_schema() -> Value {
    let mut v = schema::<Document>();
    if let Value::Object(m) = &mut v {
        m.insert(
            "$id".into(),
            Value::String("https://github.com/luiscarlosjayk/tiza/schema/tiza.schema.json".into()),
        );
        m.insert(
            "title".into(),
            Value::String("Tiza document (.tiza)".into()),
        );
    }
    v
}

/// JSON Schema of the local API: request/response envelopes and each method.
pub fn api_schema() -> ApiSchema {
    let string = || serde_json::json!({"type": "string"});
    let step_result = schema::<StepReply>;
    let methods = METHODS
        .iter()
        .map(|(m, d)| {
            let (params, result) = match *m {
                "ping" => (schema::<NoParams>(), serde_json::json!({"type": "string", "const": "pong"})),
                "status" => (schema::<NoParams>(), schema::<Status>()),
                "api.methods" => (schema::<NoParams>(), serde_json::json!({"type": "array", "items": {"type": "object", "properties": {"method": string(), "description": string()}}})),
                "api.schema" => (schema::<NoParams>(), schema::<ApiSchema>()),
                "code.get" => (schema::<NoParams>(), serde_json::json!({"type": "object", "properties": {"language": string(), "file": {"type": ["string", "null"]}, "view": {"type": ["string", "null"]}, "first_line": {"type": "integer"}, "last_line": {"type": "integer"}, "text": string(), "invisible": {"type": "array", "description": "Each character of the code that the screen does not show as it is (an invisible character or a control character), in order. `text` holds the real character.", "items": {"type": "object", "properties": {"line": {"type": "integer"}, "col": {"type": "integer", "description": "The grapheme, from 0, as the `col` of an anchor."}, "code_point": {"type": "string", "description": "`U+200B`"}, "name": {"type": "string", "description": "`zero width space`"}}}}}})),
                "scene.get" => (schema::<SceneGetParams>(), schema::<Scene>()),
                "scene.list" => (schema::<NoParams>(), serde_json::json!({"type": "array", "items": string()})),
                "annotation.add" => (schema::<AddParams>(), schema::<AnnotationMutated>()),
                "annotation.update" => (schema::<UpdateParams>(), schema::<AnnotationMutated>()),
                "annotation.delete" => (schema::<IdParams>(), schema::<Mutated>()),
                "style.set" => (schema::<StyleSetParams>(), serde_json::json!({"type": "object", "properties": {"name": string(), "style": {"type": "object"}, "revision": {"type": "integer"}}})),
                "style.list" => (schema::<NoParams>(), serde_json::json!({"type": "object", "properties": {"document": {"type": "object"}, "builtin": {"type": "array"}, "aliases": {"type": "array"}}})),
                "scene.put" => (schema::<PutParams>(), schema::<SceneChanged>()),
                "scene.patch" => (schema::<PatchParams>(), schema::<SceneChanged>()),
                "scene.clear" => (schema::<ClearParams>(), serde_json::json!({"type": "object", "properties": {"removed": {"type": "integer"}, "remaining": {"type": "integer"}, "revision": {"type": "integer"}}})),
                "move.add" => (schema::<crate::moves::Move>(), schema::<MoveMutated>()),
                "move.delete" => (schema::<MoveIndexParams>(), schema::<MoveMutated>()),
                "move.list" => (schema::<NoParams>(), schema::<MoveList>()),
                "step.set" => (schema::<StepParams>(), step_result()),
                "step.next" | "step.prev" => (schema::<NoParams>(), step_result()),
                "step.goto" => (schema::<StepGotoParams>(), step_result()),
                "focus.set" => (schema::<FocusParams>(), serde_json::json!({"type": "object", "properties": {"focus": {}, "revision": {"type": "integer"}}})),
                "pointer.set" => (schema::<crate::protocol::PointerParams>(), schema::<crate::protocol::PointerReply>()),
                "pointer.clear" => (schema::<NoParams>(), schema::<crate::protocol::PointerReply>()),
                "theme.set" => (schema::<ThemeParams>(), serde_json::json!({"type": "object", "properties": {"theme": string(), "auto": {"type": "boolean"}, "revision": {"type": "integer"}}})),
                "render.text" => (
                    schema::<RenderParams>(),
                    serde_json::json!({"type": "object", "properties": {"text": {"type": "string"}, "width": {"type": "integer", "description": "The width it was laid out at"}}}),
                ),
                "check" => (schema::<CheckParams>(), schema::<crate::protocol::LiveCheck>()),
                "follow.set" => (schema::<FollowParams>(), schema::<FollowStatus>()),
                "follow.status" => (schema::<NoParams>(), schema::<FollowStatus>()),
                "supervise.set" => (schema::<SuperviseParams>(), schema::<SuperviseStatus>()),
                "supervise.status" => (schema::<NoParams>(), schema::<SuperviseStatus>()),
                _ => (Value::Object(Default::default()), Value::Object(Default::default())),
            };
            MethodSchema {
                method: m.to_string(),
                description: d.to_string(),
                params,
                result,
            }
        })
        .collect();
    ApiSchema {
        protocol: crate::protocol::PROTOCOL_VERSION,
        request: schema::<crate::protocol::Request>(),
        response: schema::<crate::protocol::Response>(),
        methods,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn document_schema_mentions_every_shape() {
        let s = serde_json::to_string(&document_schema()).unwrap();
        for t in [
            "highlight",
            "mark",
            "box",
            "arrow",
            "note",
            "callout",
            "spotlight",
        ] {
            assert!(
                s.contains(&format!("\"{t}\"")),
                "{t} missing from the schema"
            );
        }
        assert!(
            s.contains("\"stroke\"") && s.contains("\"border\"") && s.contains("\"placement\"")
        );
    }

    #[test]
    fn api_schema_covers_every_method() {
        let api = api_schema();
        assert_eq!(api.methods.len(), METHODS.len());
        assert!(
            api.methods
                .iter()
                .all(|m| m.params.is_object() && m.result.is_object())
        );
    }

    /// The copies committed under `schema/` must match the types. Regenerate
    /// with `tiza schema document > schema/tiza.schema.json` and
    /// `tiza schema api > schema/api.schema.json`.
    #[test]
    fn committed_schemas_are_current() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schema");
        for (file, value) in [
            ("tiza.schema.json", document_schema()),
            (
                "api.schema.json",
                serde_json::to_value(api_schema()).unwrap(),
            ),
        ] {
            let path = root.join(file);
            let on_disk: Value = serde_json::from_str(
                &std::fs::read_to_string(&path)
                    .unwrap_or_else(|e| panic!("{}: {e}", path.display())),
            )
            .unwrap();
            assert_eq!(
                on_disk,
                value,
                "{file} is stale: run `cargo run -- schema {} > schema/{file}`",
                if file.starts_with("api") {
                    "api"
                } else {
                    "document"
                }
            );
        }
    }
}
