//! JSON Schemas of the `.codedraw` document and of the local API, generated
//! from the Rust types (schemars). `codedraw schema` prints them; the copies
//! committed under `schema/` are checked by a test so they never go stale.

use schemars::{JsonSchema, schema_for};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::model::{Document, MarkStyle};
use crate::protocol::{
    AddParams, IdParams, METHODS, Mutated, PatchParams, PutParams, Scene, SceneChanged, Status,
    StepParams, UpdateParams,
};

/// Parameters of `supervise.set`.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
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
pub struct ClearParams {
    /// Only this step; omit to clear everything.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub step: Option<u32>,
}

/// Parameters of `render.text`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct RenderParams {
    /// Columns; defaults to the terminal's width.
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
    /// `dark`, `light` or `toggle` (the default when omitted).
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
    /// Time of the last reload, `HH:MM:SSZ`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_reload: Option<String>,
    /// The file is currently missing on disk (waiting for it to come back).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub missing: Option<bool>,
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

/// The whole API contract, as `api.schema` and `codedraw schema api` return it.
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

/// JSON Schema of the `.codedraw` document.
pub fn document_schema() -> Value {
    let mut v = schema::<Document>();
    if let Value::Object(m) = &mut v {
        m.insert(
            "$id".into(),
            Value::String(
                "https://github.com/luiscarlosjayk/codedraw/schema/codedraw.schema.json".into(),
            ),
        );
        m.insert(
            "title".into(),
            Value::String("CodeDraw document (.codedraw)".into()),
        );
    }
    v
}

/// JSON Schema of the local API: request/response envelopes and each method.
pub fn api_schema() -> ApiSchema {
    let string = || serde_json::json!({"type": "string"});
    let text = || serde_json::json!({"type": "object", "properties": {"text": {"type": "string"}}});
    let step_result = || serde_json::json!({"type": "object", "properties": {"step": {"type": "integer"}, "max_step": {"type": "integer"}, "revision": {"type": "integer"}}});
    let methods = METHODS
        .iter()
        .map(|(m, d)| {
            let (params, result) = match *m {
                "ping" => (schema::<NoParams>(), serde_json::json!({"type": "object", "properties": {"type": {"const": "pong"}, "protocol": {"type": "integer"}}})),
                "status" => (schema::<NoParams>(), schema::<Status>()),
                "api.methods" => (schema::<NoParams>(), serde_json::json!({"type": "array", "items": {"type": "object", "properties": {"method": string(), "description": string()}}})),
                "api.schema" => (schema::<NoParams>(), schema::<ApiSchema>()),
                "code.get" => (schema::<NoParams>(), serde_json::json!({"type": "object", "properties": {"language": string(), "first_line": {"type": "integer"}, "last_line": {"type": "integer"}, "text": string()}})),
                "scene.get" => (schema::<NoParams>(), schema::<Scene>()),
                "scene.list" => (schema::<NoParams>(), serde_json::json!({"type": "array", "items": string()})),
                "annotation.add" => (schema::<AddParams>(), schema::<Mutated>()),
                "annotation.update" => (schema::<UpdateParams>(), schema::<Mutated>()),
                "annotation.delete" => (schema::<IdParams>(), schema::<Mutated>()),
                "style.set" => (schema::<StyleSetParams>(), serde_json::json!({"type": "object", "properties": {"name": string(), "revision": {"type": "integer"}}})),
                "style.list" => (schema::<NoParams>(), serde_json::json!({"type": "object", "properties": {"document": {"type": "object"}, "builtin": {"type": "array"}, "aliases": {"type": "array"}}})),
                "scene.put" => (schema::<PutParams>(), schema::<SceneChanged>()),
                "scene.patch" => (schema::<PatchParams>(), schema::<SceneChanged>()),
                "scene.clear" => (schema::<ClearParams>(), serde_json::json!({"type": "object", "properties": {"removed": {"type": "integer"}, "revision": {"type": "integer"}}})),
                "step.set" => (schema::<StepParams>(), step_result()),
                "step.next" | "step.prev" => (schema::<NoParams>(), step_result()),
                "focus.set" => (schema::<FocusParams>(), serde_json::json!({"type": "object", "properties": {"focus": {}, "revision": {"type": "integer"}}})),
                "theme.set" => (schema::<ThemeParams>(), serde_json::json!({"type": "object", "properties": {"theme": string(), "revision": {"type": "integer"}}})),
                "render.text" => (schema::<RenderParams>(), text()),
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
    /// with `codedraw schema document > schema/codedraw.schema.json` and
    /// `codedraw schema api > schema/api.schema.json`.
    #[test]
    fn committed_schemas_are_current() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schema");
        for (file, value) in [
            ("codedraw.schema.json", document_schema()),
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
