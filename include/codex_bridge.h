#ifndef CODEX_BRIDGE_H
#define CODEX_BRIDGE_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef struct LoginSession codex_bridge_login_session_t;

uint32_t codex_bridge_login_abi_version(void);
const char *codex_bridge_codex_commit(void);
const char *codex_bridge_login_last_error(void);

int32_t codex_bridge_login_start(
    const uint8_t *config_json,
    size_t config_len,
    codex_bridge_login_session_t **out_session,
    char **out_start_json);

int32_t codex_bridge_login_wait(
    codex_bridge_login_session_t *session,
    char **out_credentials_json);

int32_t codex_bridge_login_cancel(codex_bridge_login_session_t *session);

int32_t codex_bridge_login_credentials(
    const uint8_t *config_json,
    size_t config_len,
    char **out_credentials_json);

/* Query current Codex rate limits and credit usage for a serialized auth.json payload. */
int32_t codex_bridge_status_query(
    const uint8_t *auth_json,
    size_t auth_len,
    const uint8_t *base_url,
    size_t base_url_len,
    char **out_status_json);

/* Refreshes one stored auth.json payload. Returns OK for both successful and failed
   refreshes; the returned JSON carries refreshed/permanent/message/auth so the caller can
   tell a retryable failure from one that requires a new browser login. */
int32_t codex_bridge_token_refresh(
    const uint8_t *auth_json,
    size_t auth_len,
    char **out_result_json);

/* Returns the JSON Schema for the Codex Responses request body, with `default` values for the
   baseline fields Codex fills in. Static metadata: no credentials, no network call. Callers can
   fill any missing top-level keys of a plain OpenAI-SDK request from these defaults. */
int32_t codex_bridge_responses_schema(char **out_schema_json);

void codex_bridge_login_session_free(codex_bridge_login_session_t *session);
void codex_bridge_string_free(char *value);

#ifdef __cplusplus
}
#endif

#endif
