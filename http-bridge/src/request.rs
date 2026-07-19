use crate::HttpBridgeError;
use serde::Deserialize;
use serde::Serialize;
use serde_json::Map;
use serde_json::Value;
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct HttpRequest {
    /// Responses request body. The bridge preserves unknown fields for forward compatibility.
    pub body: Value,
    pub session_id: Option<String>,
    pub thread_id: Option<String>,
    pub installation_id: Option<String>,
    pub window_id: Option<String>,
    pub extra_headers: HashMap<String, String>,
    /// Fill current Codex-required defaults and metadata without overwriting caller values.
    pub normalize: bool,
}

impl Default for HttpRequest {
    fn default() -> Self {
        Self {
            body: Value::Object(Map::new()),
            session_id: None,
            thread_id: None,
            installation_id: None,
            window_id: None,
            extra_headers: HashMap::new(),
            normalize: true,
        }
    }
}

impl HttpRequest {
    pub fn from_json(json: &[u8]) -> Result<Self, HttpBridgeError> {
        let request: Self = serde_json::from_slice(json)?;
        if !request.body.is_object() {
            return Err(HttpBridgeError::InvalidRequest(
                "body must be a JSON object".to_string(),
            ));
        }
        Ok(request)
    }

    pub(crate) fn normalize_body(
        &mut self,
        session_id: &str,
        thread_id: &str,
        installation_id: &str,
        window_id: &str,
    ) -> Result<(), HttpBridgeError> {
        let body = self.body.as_object_mut().ok_or_else(|| {
            HttpBridgeError::InvalidRequest("body must be a JSON object".to_string())
        })?;
        if !body.get("model").is_some_and(Value::is_string) {
            return Err(HttpBridgeError::InvalidRequest(
                "body.model must be a string".to_string(),
            ));
        }
        if !body.get("input").is_some_and(Value::is_array) {
            return Err(HttpBridgeError::InvalidRequest(
                "body.input must be an array of Responses input items".to_string(),
            ));
        }

        body.insert("stream".to_string(), Value::Bool(true));
        if !self.normalize {
            return Ok(());
        }

        body.entry("instructions")
            .or_insert(Value::String(String::new()));
        body.entry("tools")
            .or_insert_with(|| Value::Array(Vec::new()));
        body.entry("tool_choice")
            .or_insert_with(|| Value::String("auto".to_string()));
        body.entry("parallel_tool_calls")
            .or_insert(Value::Bool(false));
        body.entry("store").or_insert(Value::Bool(false));
        body.entry("include").or_insert_with(|| {
            Value::Array(vec![Value::String(
                "reasoning.encrypted_content".to_string(),
            )])
        });
        body.entry("prompt_cache_key")
            .or_insert_with(|| Value::String(session_id.to_string()));

        let metadata = body
            .entry("client_metadata")
            .or_insert_with(|| Value::Object(Map::new()));
        let metadata = metadata.as_object_mut().ok_or_else(|| {
            HttpBridgeError::InvalidRequest("body.client_metadata must be an object".to_string())
        })?;
        metadata
            .entry("x-codex-installation-id")
            .or_insert_with(|| Value::String(installation_id.to_string()));
        metadata
            .entry("session_id")
            .or_insert_with(|| Value::String(session_id.to_string()));
        metadata
            .entry("thread_id")
            .or_insert_with(|| Value::String(thread_id.to_string()));
        metadata
            .entry("x-codex-window-id")
            .or_insert_with(|| Value::String(window_id.to_string()));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn normalization_preserves_unknown_fields_and_adds_codex_metadata() {
        let mut request = HttpRequest {
            body: json!({"model":"gpt-test","input":[],"future_field":{"x":1}}),
            ..HttpRequest::default()
        };
        request
            .normalize_body("session", "thread", "install", "thread:0")
            .expect("normalize");
        assert_eq!(request.body["stream"], true);
        assert_eq!(request.body["future_field"]["x"], 1);
        assert_eq!(request.body["client_metadata"]["thread_id"], "thread");
        assert_eq!(request.body["include"][0], "reasoning.encrypted_content");
    }
}
