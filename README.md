# Codex Bridge

Native dynamic/static libraries and test executables built directly against the Codex CLI source
included as `upstream/codex`.

Implemented capabilities are split into separate native libraries:

- `libcodex_bridge`: ChatGPT browser OAuth login and credential export.
- `libcodex_http_bridge`: authenticated Responses HTTP transport, Codex headers, request
  compression, retry handling and SSE parsing.

## Initialize the submodule

```bash
git submodule update --init --recursive
```

## Build

```bash
cargo build --release
```

On macOS this produces:

```text
target/release/libcodex_bridge.dylib
target/release/codex-bridge-login
target/release/libcodex_http_bridge.dylib
target/release/codex-bridge-http
```

Linux produces `libcodex_bridge.so`; Windows produces `codex_bridge.dll`.

## Test browser login

Use a dedicated Codex home while testing:

```bash
target/release/codex-bridge-login login \
  --codex-home "$PWD/.test-codex-home"
```

The command opens the browser and waits for the exact local OAuth callback flow implemented by
`codex-login`. Credentials are persisted to `.test-codex-home/auth.json` by default.

Print a safe credential summary:

```bash
target/release/codex-bridge-login credentials \
  --codex-home "$PWD/.test-codex-home"
```

Print the complete credential payload, including access and refresh tokens:

```bash
target/release/codex-bridge-login credentials \
  --codex-home "$PWD/.test-codex-home" \
  --show-secrets
```

## C ABI

Include `include/codex_bridge.h`. A typical login flow is:

1. `codex_bridge_login_start` returns an opaque session and JSON containing `auth_url`.
2. Present or open `auth_url` and call `codex_bridge_login_wait` on a worker thread.
3. The wait call returns the persisted Codex credential payload as JSON.
4. Release returned strings with `codex_bridge_string_free` and the session with
   `codex_bridge_login_session_free`.

The dynamic library owns a Tokio runtime, so the embedding proxy does not need a Rust async
runtime.

Compile and run the included C ABI smoke test on macOS:

```bash
cc examples/c_abi_smoke.c \
  -I include \
  -L target/release \
  -lcodex_bridge \
  -Wl,-rpath,"$PWD/target/release" \
  -o target/c-abi-smoke
./target/c-abi-smoke
```

## HTTP Responses bridge

First complete login using the same Codex home, then create a Responses body:

```json
{
  "model": "gpt-5.4",
  "instructions": "You are a coding agent.",
  "input": [
    {
      "type": "message",
      "role": "user",
      "content": [{"type": "input_text", "text": "Say hello"}]
    }
  ],
  "reasoning": {"effort": "high", "summary": "auto"}
}
```

Save it as `request.json` and run:

```bash
target/release/codex-bridge-http request \
  --codex-home "$PWD/.test-codex-home" \
  --body request.json
```

The command prints normalized events as NDJSON. The HTTP bridge directly reuses Codex crates for:

- loading and refreshing ChatGPT credentials;
- default User-Agent, originator and Cloudflare cookie behavior;
- `/responses` URL construction and provider retry policy;
- `Authorization`, `ChatGPT-Account-ID`, session/thread and Codex metadata headers;
- optional Zstd request compression;
- SSE framing and typed `ResponseEvent` parsing.

The request envelope accepted by the C ABI is:

```json
{
  "body": {"model": "gpt-5.4", "input": []},
  "session_id": "optional-session-id",
  "thread_id": "optional-thread-id",
  "installation_id": "optional-installation-id",
  "window_id": "optional-window-id",
  "extra_headers": {},
  "normalize": true
}
```

Unknown body fields are preserved. With normalization enabled, the bridge fills current baseline
Codex fields and metadata without replacing caller-provided values. `stream` is always forced to
`true` because the bridge consumes the upstream SSE stream.

The HTTP C interface is in `http-bridge/include/codex_http_bridge.h`. Its lifecycle is:

1. `codex_http_bridge_create` loads ChatGPT auth and constructs the Codex transport.
2. `codex_http_bridge_request_start` starts one streaming request.
3. `codex_http_bridge_event_next` returns normalized JSON events.
4. `codex_http_bridge_request_cancel` cancels an active stream.
5. Free event strings, request handles and bridge handles with their matching functions.

Build and run its C ABI smoke test on macOS:

```bash
cc http-bridge/examples/c_abi_smoke.c \
  -I http-bridge/include \
  -L target/release \
  -lcodex_http_bridge \
  -Wl,-rpath,"$PWD/target/release" \
  -o target/http-c-abi-smoke
./target/http-c-abi-smoke
```

## Update Codex

```bash
git -C upstream/codex fetch origin
git -C upstream/codex checkout origin/main
git add upstream/codex
cargo build --release
```

The parent repository records the exact Codex commit used for each bridge build.
