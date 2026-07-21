# Codex Bridge 项目归档

## 项目目标

本仓库是 Codex CLI 的独立 Bridge 项目，通过 Git submodule 引用官方 Codex
源码，将可复用能力封装成稳定的 Rust API、C ABI、动态库和可直接运行的测试程序。

分成两个相互独立、可单独分发的库：

**`libcodex_bridge`（登录库，`src/`）** —— 账号与元数据：

1. ChatGPT 浏览器 OAuth 登录和凭证导出（`login.rs`）。
2. 账号额度 / 限流查询（`status.rs`，`/wham/usage`）。
3. `access_token` 刷新（`refresh.rs`，复用上游 `AuthManager`，会轮换并持久化 refresh_token）。
4. 可用模型目录（`models.rs`，等价 CLI 的 `/model`：按 priority 排序、按登录态过滤）。
5. Codex Responses 请求体的 JSON Schema（`template.rs`，各基线字段带 `default`，静态元数据无需凭证）。

**`libcodex_http_bridge`（HTTP 库，`http-bridge/`）** —— 使用 ChatGPT 凭证调用 Codex Responses HTTP/SSE 协议。

两个库运行时的唯一交界是 Codex 格式的 `auth.json`；HTTP 库不链接登录库。

项目根目录：

```text
/Users/chenxy/workspace/codex/codex-bridge
```

## 上游 Codex

- Submodule 路径：`upstream/codex`
- Submodule 地址：`https://github.com/openai/codex.git`
- 当前已验证 commit：`cbc83d961e8132bfff4d340ab8342d181b79e95e`
- Bridge 的 `build.rs` 会将编译时的 Codex commit 写入动态库，可通过 C ABI
  查询。
- 不要在 `upstream/codex` 里添加 Bridge 业务代码，避免影响后续上游更新。

更新上游：

```bash
git -C upstream/codex fetch origin
git -C upstream/codex checkout origin/main
git add upstream/codex
cargo build --release
cargo test --release
```

Codex 公开 API 未变更时可直接重新编译。类型、构造函数或 SSE 事件枚举变更时，
应修复 Bridge 适配层，不要用忽略未知事件的方式绕过编译错误。

## 仓库结构

```text
codex-bridge/
├── Cargo.toml                  # 根 package + workspace 配置
├── Cargo.lock
├── build.rs                   # 登录库 install name 和 Codex commit
├── src/                       # 登录 Bridge
├── include/codex_bridge.h     # 登录 C ABI
├── examples/                  # 登录 C ABI 示例
├── http-bridge/               # HTTP Bridge 独立 crate
│   ├── Cargo.toml
│   ├── build.rs
│   ├── src/
│   ├── include/codex_http_bridge.h
│   └── examples/c_abi_smoke.c
├── upstream/codex/             # 官方 Codex Git submodule
└── target/                     # 共享 Cargo 产物，不提交
```

## 登录 Bridge

源码：

```text
src/config.rs       登录配置(codex_home/端口/issuer/凭证存储后端)
src/login.rs        浏览器 OAuth 流程(复用上游 codex-login)
src/status.rs       额度/限流查询
src/refresh.rs      access_token 刷新
src/models.rs       可用模型目录(/model)
src/template.rs     Responses 请求体 JSON Schema
src/error.rs        BridgeError
src/ffi.rs          稳定 C ABI
src/lib.rs
src/bin/codex-bridge-login.rs   命令行测试程序
```

`login.rs` 直接复用上游 `codex-login` 的 ChatGPT browser OAuth 流程，包括本地 callback
server、浏览器登录、凭证持久化和凭证导出。

`refresh.rs` / `models.rs` 采用「凭证 → 临时 Codex home → 复用上游逻辑 → 读回结果」的方式：
调用方把凭证存在自己的数据库里而不是 Codex home，所以先把 `auth.json` 落到临时目录交给上游
`AuthManager` / models manager，再读回结果，既不重复实现刷新/目录语义，也不污染宿主的 Codex 配置。

产物：

```text
target/release/libcodex_bridge.dylib
target/release/libcodex_bridge.a
target/release/codex-bridge-login
```

测试登录：

```bash
target/release/codex-bridge-login login \
  --codex-home "$PWD/.test-codex-home"

target/release/codex-bridge-login credentials \
  --codex-home "$PWD/.test-codex-home"
```

代理进程推荐直接使用 `include/codex_bridge.h` 和 `libcodex_bridge` 动态库。登录 ABI
返回 opaque session，登录完成后返回完整鉴权 JSON。

登录库当前导出的 C ABI（返回码 `0 OK / 1 ERROR / 2 PANIC`）：

- `codex_bridge_login_start` / `_wait` / `_cancel`：浏览器登录三步。
- `codex_bridge_login_credentials`：读已持久化的凭证，不发起新登录。
- `codex_bridge_status_query`：用 `auth.json` 查 `/wham/usage` 和 reset credit 详情。
- `codex_bridge_token_refresh`：刷新 `access_token`。**成功与失败都返回 OK**，结果在 JSON 里区分：
  `{"refreshed":true,"permanent":false,"auth":{...}}` 表示成功并回传轮换后的完整 `auth.json`；
  `{"refreshed":false,"permanent":true,"message":"..."}` 表示 refresh token 已失效，**必须重新登录**。
- `codex_bridge_models_list`：列出该账号可用模型（每个模型自带推理强度 `supported_reasoning_efforts`
  与速度档位 `service_tiers`），等价 CLI 的 `/model`，目录按账号套餐过滤。
- `codex_bridge_responses_schema`：返回 Codex Responses 请求体的 JSON Schema，每个基线字段带
  `default`（`store:false`、`stream:true`、`include` 等）。**静态元数据，无需凭证、不发网络请求**，
  供代理补齐原生 SDK 请求缺失的字段。
- `codex_bridge_codex_commit`：编译时的上游 commit。
- `codex_bridge_login_session_free` / `_string_free`：释放 session / 返回的字符串。

旧的进程集成测试仍可使用 `login-json`。它先输出 `login.started` NDJSON 事件，浏览器回调
完成后再输出包含账号摘要和完整鉴权内容的 `login.completed` 事件：

```bash
target/release/codex-bridge-login login-json \
  --codex-home "$PWD/.test-codex-home" --no-open-browser --port 0
```

列出该账号可用模型 / 推理强度 / 速度档位：

```bash
target/release/codex-bridge-login models --codex-home "$PWD/.test-codex-home"
```

`.test-codex-home/auth.json` 是本地敏感凭证，已被 `.gitignore` 忽略。不要将其内容、
access token 或 refresh token 写入文档和测试输出。

## HTTP Responses Bridge

位于独立 crate：`http-bridge/`。

源码职责：

- `config.rs`：`CODEX_HOME`、base URL、凭证存储、请求压缩和 beta 配置。
- `request.rs`：Responses JSON 信封解析和规范化；保留未知字段。
- `engine.rs`：读取/刷新 ChatGPT 凭证、构造请求头、发送请求、重试、SSE 流和取消。
- `event.rs`：将上游 `ResponseEvent` 穷尽映射为稳定 JSON 事件。
- `ffi.rs`：稳定 C ABI，包含 Bridge/request handle、轮询、超时、取消和内存释放。
- `bin/codex-bridge-http.rs`：可直接运行的 NDJSON 流式测试程序。

主要上游依赖：

- `codex-api::ResponsesClient`
- `codex-http-client::ReqwestTransport`
- `codex-login::AuthManager`
- `codex-model-provider-info`
- `codex-protocol`

设计说明：没有直接包装 `codex-core::ModelClient`，因为当前上游版本的部分必需
构造器/字段为 `pub(crate)`，仓库外 crate 无法稳定构造。当前实现是面向代理接入的
Responses wire bridge，直接复用 Codex 的认证、HTTP transport、provider retry 和 SSE
parser。

产物：

```text
target/release/libcodex_http_bridge.dylib
target/release/libcodex_http_bridge.a
target/release/codex-bridge-http
```

C 头文件：

```text
http-bridge/include/codex_http_bridge.h
```

HTTP 库不链接 `libcodex_bridge.dylib`。它会自己使用上游 `codex-login`
读取 `CODEX_HOME/auth.json` 和刷新 token。因此：

- 已有凭证时，其他代理只需分发 HTTP 库和 HTTP 头文件。
- 需要在代理内发起登录时，再额外分发登录库和对应头文件。
- 两个库的运行时数据交界只是 Codex 格式的 `auth.json`。

## HTTP C ABI 生命周期

1. `codex_http_bridge_create`：读取配置和 ChatGPT 凭证。
2. `codex_http_bridge_request_start`：传入 Responses 请求信封，返回 request handle。
3. `codex_http_bridge_event_next`：阻塞或带超时地取得一条 JSON 事件。
4. `codex_http_bridge_request_cancel`：客户端断开时取消上游请求。
5. 用 `codex_http_bridge_string_free` 释放事件字符串。
6. 请求结束后释放 request handle，所有 request 结束后再释放 bridge。

返回码：

```text
0 OK
1 ERROR
2 PANIC
3 TIMEOUT
4 CLOSED
```

`codex_http_bridge_event_next` 是阻塞 API，代理应将它放在 worker thread 上。同一
request handle 同时只应有一个事件消费者。

## HTTP 请求格式

C ABI 接受信封，而命令行程序的 `--body` 只接受信封内的 Responses body：

```json
{
  "body": {
    "model": "gpt-5.4-mini",
    "input": [
      {
        "type": "message",
        "role": "user",
        "content": [{"type": "input_text", "text": "你好"}]
      }
    ]
  },
  "session_id": "optional-session-id",
  "thread_id": "optional-thread-id",
  "installation_id": "optional-installation-id",
  "window_id": "optional-window-id",
  "extra_headers": {},
  "normalize": true
}
```

`normalize=true` 时补全当前 Codex 基线字段，不覆盖调用方已给字段；`stream`
始终强制为 `true`。不认识的 body 字段会保留，便于将来协议扩展。

当前真实账号测试中，`gpt-5.4-mini` 请求成功；模型可用性与账号和服务端当前配置
有关，不要将该模型硬编码为永久默认值。

## 构建与验证

由于 Codex 依赖树较大，根 `Cargo.toml` 已设置不产生调试信息并禁用 incremental。
本机磁盘空间曾较紧张，优先使用定向 release 构建，不要随意清理已生成的 release
cache 后重编整个上游。

```bash
cargo build --release -p codex-bridge -j 2
cargo build --release -p codex-http-bridge -j 2
cargo test --release -p codex-bridge --lib -j 2
cargo test --release -p codex-http-bridge --lib -j 2
cargo fmt --all -- --check
git diff --check
```

已完成的验证：

- 登录 Bridge Rust 测试和 C ABI smoke test 通过。
- HTTP Bridge Rust 请求规范化测试通过。
- HTTP Bridge 本地 TCP/SSE 集成测试通过，验证 Authorization、account、session、
  thread header 和事件解析。
- HTTP C ABI smoke test 通过。
- macOS HTTP dylib install name 已验证为 `@rpath/libcodex_http_bridge.dylib`。
- 使用本地 ChatGPT 凭证发起过真实 `gpt-5.4-mini` 请求，SSE 和 completed
  token usage 事件均正常。

## 代理接入边界

HTTP Bridge 是上游通信库，不是完整代理服务器。调用方仍需负责：

- 对外 HTTP server 和 SSE 响应。
- Chat Completions/messages 与 Responses input/event 的转换。
- 模型选择、会话映射和多账号调度。
- 下游客户端断开后调用 cancel。

动态库目前仅实现 HTTP Responses/SSE，尚未实现 WebSocket Bridge、完整 Agent
执行器、MCP 代理、多账号凭证池或独立 HTTP server。

## Git 状态提醒

本 Bridge 仓库是独立 Git 仓库。当前实现文件和 submodule 仍处于初始未提交状态；
后续修改前先查看 `git status`，不要将 `target/`、`.test-codex-home/` 或任何凭证提交。
