use codex_api::ResponseEvent;
use serde_json::Value;
use serde_json::json;

pub(crate) fn event_to_json(event: ResponseEvent) -> Value {
    match event {
        ResponseEvent::Created => json!({"type":"response.created"}),
        ResponseEvent::SafetyBuffering(value) => json!({
            "type":"response.safety_buffering",
            "use_cases":value.use_cases,
            "reasons":value.reasons,
            "show_buffering_ui":value.show_buffering_ui,
            "faster_model":value.faster_model,
        }),
        ResponseEvent::OutputItemDone(item) => {
            json!({"type":"response.output_item.done","item":item})
        }
        ResponseEvent::OutputItemAdded(item) => {
            json!({"type":"response.output_item.added","item":item})
        }
        ResponseEvent::ServerModel(model) => {
            json!({"type":"response.server_model","model":model})
        }
        ResponseEvent::ModelVerifications(verifications) => {
            json!({"type":"response.model_verifications","verifications":verifications})
        }
        ResponseEvent::TurnModerationMetadata(metadata) => {
            json!({"type":"response.turn_moderation_metadata","metadata":metadata.metadata})
        }
        ResponseEvent::ServerReasoningIncluded(included) => {
            json!({"type":"response.server_reasoning_included","included":included})
        }
        ResponseEvent::Completed {
            response_id,
            token_usage,
            end_turn,
        } => json!({
            "type":"response.completed",
            "response_id":response_id,
            "token_usage":token_usage,
            "end_turn":end_turn,
        }),
        ResponseEvent::OutputTextDelta(delta) => {
            json!({"type":"response.output_text.delta","delta":delta})
        }
        ResponseEvent::ToolCallInputDelta {
            item_id,
            call_id,
            delta,
        } => json!({
            "type":"response.function_call_arguments.delta",
            "item_id":item_id,
            "call_id":call_id,
            "delta":delta,
        }),
        ResponseEvent::ReasoningSummaryDelta {
            delta,
            summary_index,
        } => json!({
            "type":"response.reasoning_summary_text.delta",
            "delta":delta,
            "summary_index":summary_index,
        }),
        ResponseEvent::ReasoningSummaryDone {
            item_id,
            text,
            summary_index,
        } => json!({
            "type":"response.reasoning_summary_text.done",
            "item_id":item_id,
            "text":text,
            "summary_index":summary_index,
        }),
        ResponseEvent::ReasoningContentDelta {
            delta,
            content_index,
        } => json!({
            "type":"response.reasoning_text.delta",
            "delta":delta,
            "content_index":content_index,
        }),
        ResponseEvent::ReasoningSummaryPartAdded { summary_index } => json!({
            "type":"response.reasoning_summary_part.added",
            "summary_index":summary_index,
        }),
        ResponseEvent::RateLimits(rate_limits) => {
            json!({"type":"response.rate_limits","rate_limits":rate_limits})
        }
        ResponseEvent::ModelsEtag(etag) => {
            json!({"type":"response.models_etag","etag":etag})
        }
    }
}
