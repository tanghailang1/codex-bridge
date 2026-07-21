use serde_json::Value;
use serde_json::json;

/// JSON Schema for the Responses request body that the Codex CLI sends.
///
/// Each baseline field that Codex fills in carries a `default`, so a proxy that receives a plain
/// OpenAI-SDK request (which omits them) can read the schema and fill any missing top-level keys
/// before forwarding to the ChatGPT Codex backend.
///
/// The defaults mirror `codex-http-bridge`'s `normalize_body`, the tested source of truth for what
/// Codex adds. `store` is the only field the ChatGPT Codex backend hard-requires (`false`); the
/// rest are functional or affect cost/latency, not whether the request is accepted.
///
/// This is static metadata: the call takes no credentials and makes no network request.
pub fn responses_request_schema() -> Value {
    json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "title": "Codex Responses request",
        "type": "object",
        "required": ["model", "input"],
        "additionalProperties": true,
        "properties": {
            "model": {
                "type": "string",
                "description": "Model slug, e.g. gpt-5.6-sol. Provided by the caller."
            },
            "input": {
                "type": "array",
                "description": "Responses input items. Provided by the caller."
            },
            "instructions": {
                "type": "string",
                "default": "",
                "description": "System/developer instructions."
            },
            "tools": {
                "type": "array",
                "default": [],
                "description": "Tool definitions. Empty means a plain chat turn."
            },
            "tool_choice": {
                "type": ["string", "object"],
                "default": "auto"
            },
            "parallel_tool_calls": {
                "type": "boolean",
                "default": false
            },
            "store": {
                "type": "boolean",
                "default": false,
                "description": "The ChatGPT Codex backend rejects the request unless this is false."
            },
            "stream": {
                "type": "boolean",
                "default": true,
                "description": "Codex streams. A passthrough proxy should keep the client's own choice rather than force this, or it will break non-streaming clients."
            },
            "include": {
                "type": "array",
                "items": { "type": "string" },
                "default": ["reasoning.encrypted_content"],
                "description": "Lets the backend return encrypted reasoning content for reasoning models."
            },
            "reasoning": {
                "type": "object",
                "description": "Reasoning controls, e.g. {\"effort\": \"medium\"}. Provided by the caller."
            },
            "prompt_cache_key": {
                "type": "string",
                "description": "Session id used for prompt caching. Set per session by the caller."
            },
            "client_metadata": {
                "type": "object",
                "description": "session_id / turn_id / installation / window ids. Set by the caller."
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn store_default_is_false() {
        let schema = responses_request_schema();
        assert_eq!(schema["properties"]["store"]["default"], false);
        assert_eq!(
            schema["properties"]["include"]["default"][0],
            "reasoning.encrypted_content"
        );
    }
}
