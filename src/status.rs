use serde_json::Value;
use serde_json::json;

const DEFAULT_BASE_URL: &str = "https://chatgpt.com/backend-api";

/// Fetches the current Codex usage and rate-limit payload for one serialized `auth.json` value.
pub fn query_account_status(auth_json: &[u8], base_url: Option<&str>) -> Result<Value, String> {
    let auth: Value =
        serde_json::from_slice(auth_json).map_err(|error| format!("invalid auth JSON: {error}"))?;
    let access_token = auth
        .pointer("/tokens/access_token")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "auth JSON does not contain an access token".to_string())?;
    let account_id = auth
        .pointer("/tokens/account_id")
        .and_then(Value::as_str)
        .or_else(|| {
            auth.pointer("/tokens/id_token/chatgpt_account_id")
                .and_then(Value::as_str)
        });
    let base_url = base_url.unwrap_or(DEFAULT_BASE_URL).trim_end_matches('/');
    let runtime = tokio::runtime::Runtime::new().map_err(|error| error.to_string())?;
    runtime.block_on(async move {
        let client = reqwest::Client::builder()
            .user_agent("codex-cli")
            .build()
            .map_err(|error| format!("create status client: {error}"))?;
        let mut request = client
            .get(format!("{base_url}/wham/usage"))
            .bearer_auth(access_token);
        if let Some(account_id) = account_id {
            request = request.header("ChatGPT-Account-ID", account_id);
        }
        let response = request
            .send()
            .await
            .map_err(|error| format!("fetch rate limits: {error}"))?;
        let status = response.status();
        let usage: Value = response
            .json()
            .await
            .map_err(|error| format!("decode rate limits: {error}"))?;
        if !status.is_success() {
            return Err(format!("rate limits request failed: {status}: {usage}"));
        }

        let reset_credits = match client
            .get(format!("{base_url}/wham/rate-limit-reset-credits"))
            .bearer_auth(access_token)
            .send()
            .await
        {
            Ok(response) => response.json::<Value>().await.ok(),
            Err(_) => None,
        };
        Ok(json!({
            "usage": usage,
            "rate_limit_reset_credit_details": reset_credits,
        }))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_auth_without_access_token_before_network() {
        let error = query_account_status(br#"{}"#, None).expect_err("missing token must fail");
        assert!(error.contains("access token"));
    }
}
