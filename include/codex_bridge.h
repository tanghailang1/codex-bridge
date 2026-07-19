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

void codex_bridge_login_session_free(codex_bridge_login_session_t *session);
void codex_bridge_string_free(char *value);

#ifdef __cplusplus
}
#endif

#endif
