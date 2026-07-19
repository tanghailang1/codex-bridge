#ifndef CODEX_HTTP_BRIDGE_H
#define CODEX_HTTP_BRIDGE_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

#define CODEX_HTTP_BRIDGE_OK 0
#define CODEX_HTTP_BRIDGE_ERROR 1
#define CODEX_HTTP_BRIDGE_PANIC 2
#define CODEX_HTTP_BRIDGE_TIMEOUT 3
#define CODEX_HTTP_BRIDGE_CLOSED 4

typedef struct HttpBridge codex_http_bridge_t;
typedef struct HttpRequestHandle codex_http_request_t;

uint32_t codex_http_bridge_abi_version(void);
const char *codex_http_bridge_codex_commit(void);
const char *codex_http_bridge_last_error(void);

int32_t codex_http_bridge_create(
    const uint8_t *config_json,
    size_t config_len,
    codex_http_bridge_t **out_bridge);

int32_t codex_http_bridge_request_start(
    codex_http_bridge_t *bridge,
    const uint8_t *request_json,
    size_t request_len,
    codex_http_request_t **out_request);

int32_t codex_http_bridge_event_next(
    codex_http_request_t *request,
    int64_t timeout_ms,
    char **out_event_json);

int32_t codex_http_bridge_request_cancel(codex_http_request_t *request);
void codex_http_bridge_request_free(codex_http_request_t *request);
void codex_http_bridge_free(codex_http_bridge_t *bridge);
void codex_http_bridge_string_free(char *value);

#ifdef __cplusplus
}
#endif

#endif

