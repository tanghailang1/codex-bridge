# Codex Bridge

把官方 Codex CLI 的能力封装成稳定的 **C ABI 动态库**，供其它语言（当前是 `agent-go-proxy` 的 Go 进程）直接调用，
无需在宿主进程里引入 Rust 异步运行时。

上游 Codex 以 Git submodule 形式引入（`upstream/codex`），Bridge 只做适配层，不改上游代码。

- Submodule 地址：`https://github.com/openai/codex.git`
- 当前已验证 commit：`cbc83d961e8132bfff4d340ab8342d181b79e95e`
- 编译时的上游 commit 会写进库里，可通过 ABI 查询（`build.rs`）

## 已实现的模块与功能

分成两个**相互独立**的库，可单独分发：

### 1. `libcodex_bridge` —— 登录与账号管理

| 模块 | 文件 | 功能 |
|---|---|---|
| 登录 | `src/login.rs` | 复用上游 `codex-login` 的 ChatGPT 浏览器 OAuth 全流程：本地 callback server、浏览器拉起、凭证持久化、凭证导出 |
| 额度 | `src/status.rs` | 用账号凭证查 `/wham/usage` 与 `/wham/rate-limit-reset-credits`，返回限流窗口与 credit 详情 |
| 刷新 | `src/refresh.rs` | 刷新 `access_token`。把凭证落到临时 Codex home 后交给上游 `AuthManager` 刷新（**会自动轮换并持久化 refresh_token**），再读回完整 `auth.json` |
| 模型 | `src/models.rs` | 拉取该账号可用的模型目录，等价于 CLI 的 `/model`：按 priority 排序、按登录态过滤、带 `show_in_picker` 标记 |
| 请求模板 | `src/template.rs` | 返回 Codex Responses 请求体的 JSON Schema，每个基线字段带 `default`。代理拿到原生 SDK 请求时可据此补齐缺失字段（如后端硬性要求的 `store:false`）。静态元数据，无需凭证 |
| 配置 | `src/config.rs` | `LoginConfig`：codex_home、端口、issuer、凭证存储后端等 |
| 错误 | `src/error.rs` | `BridgeError` 统一错误类型 |
| C ABI | `src/ffi.rs` | 稳定 C 接口，`catch_unwind` 兜住 panic，thread-local 错误串，字符串所有权显式转移 |
| CLI | `src/bin/codex-bridge-login.rs` | 可直接运行的测试程序 |

**关于刷新与模型的设计**：调用方（代理）把凭证存在自己的数据库里而不是 Codex home，
所以这两个能力都采用「凭证 → 临时 Codex home → 复用上游逻辑 → 读回结果」的方式，
既不重复实现 OAuth 刷新语义，也不污染宿主的 Codex 配置目录。

**模型目录的三层选项**（对应 CLI `/model` 的三级菜单）——注意后两者**由每个模型各自携带**，换模型后需重新读取：

- 模型：`display_name` / `model`(slug)
- 推理强度：`supported_reasoning_efforts`，取值 `none|minimal|low|medium|high|xhigh|max|ultra`
- 速度：`service_tiers`，默认档位常量为 `"default"`（标准）；账号/模型无额外档位时该列表为空

### 2. `libcodex_http_bridge` —— Responses HTTP/SSE 传输

| 模块 | 文件 | 功能 |
|---|---|---|
| 配置 | `http-bridge/src/config.rs` | codex_home、base URL、凭证存储、请求压缩、beta features |
| 请求 | `http-bridge/src/request.rs` | Responses 信封解析与规范化，保留未知字段，`stream` 强制为 true |
| 引擎 | `http-bridge/src/engine.rs` | 读取/刷新 ChatGPT 凭证、构造 Codex 请求头、发送、重试、SSE 流、取消 |
| 事件 | `http-bridge/src/event.rs` | 把上游 `ResponseEvent` **穷尽映射**为稳定 JSON 事件 |
| C ABI | `http-bridge/src/ffi.rs` | Bridge/request handle、轮询、超时、取消、内存释放 |
| CLI | `http-bridge/src/bin/codex-bridge-http.rs` | NDJSON 流式测试程序 |

事件映射刻意写成穷尽匹配：上游枚举变更时宁可编译报错，也不用「忽略未知事件」的方式绕过。

## C ABI 一览

头文件：`include/codex_bridge.h`、`http-bridge/include/codex_http_bridge.h`

返回码：`0 OK`、`1 ERROR`、`2 PANIC`、`3 TIMEOUT`、`4 CLOSED`

### libcodex_bridge

```text
codex_bridge_login_abi_version()          ABI 版本
codex_bridge_codex_commit()               编译时的上游 commit
codex_bridge_login_last_error()           线程内最近一次错误

codex_bridge_login_start()                启动浏览器登录，返回 session + auth_url
codex_bridge_login_wait()                 阻塞等待回调完成，返回完整凭证 JSON
codex_bridge_login_cancel()               取消登录
codex_bridge_login_credentials()          读取已持久化的凭证，不发起新登录

codex_bridge_status_query()               查询额度 / 限流 / reset credits
codex_bridge_token_refresh()              刷新 access_token，返回轮换后的完整 auth.json
codex_bridge_models_list()                列出该账号可用模型（含推理强度与速度档位）
codex_bridge_responses_schema()           Responses 请求体 JSON Schema（含各字段默认值），无需凭证

codex_bridge_login_session_free()         释放 session
codex_bridge_string_free()                释放返回的字符串
```

`codex_bridge_token_refresh` 对**成功与失败都返回 OK**，结果在 JSON 里区分，便于调用方判断是否需要重新登录：

```json
{"refreshed": true,  "permanent": false, "message": "",    "auth": { ... }}
{"refreshed": false, "permanent": true,  "message": "...", "auth": null}
```

`permanent=true` 表示 refresh token 已过期/被复用/被吊销，**只能重新走浏览器登录**。

### libcodex_http_bridge

```text
codex_http_bridge_create()                读取配置与凭证，构造传输层
codex_http_bridge_request_start()         发起一次流式请求
codex_http_bridge_event_next()            阻塞/带超时取一条 JSON 事件
codex_http_bridge_request_cancel()        取消进行中的请求
codex_http_bridge_request_free() / _free() / _string_free()
```

`event_next` 是阻塞 API，应放在 worker 线程；同一 request handle 同时只能有一个消费者。

HTTP 库**不链接**登录库，它自己用上游 `codex-login` 读取和刷新 `CODEX_HOME/auth.json`。
两个库的运行时交界仅仅是 Codex 格式的 `auth.json`。

## 构建

```bash
git submodule update --init --recursive
cargo build --release -p codex-bridge -j 2
cargo build --release -p codex-http-bridge -j 2
```

产物（macOS）：

```text
target/release/libcodex_bridge.dylib        target/release/codex-bridge-login
target/release/libcodex_http_bridge.dylib   target/release/codex-bridge-http
```

Linux 产出 `.so`，Windows 产出 `.dll`。上游依赖树较大，根 `Cargo.toml` 已关闭调试信息与增量编译。

## 命令行自测

```bash
# 浏览器登录（用独立的测试 Codex home，避免污染真实配置）
target/release/codex-bridge-login login --codex-home "$PWD/.test-codex-home"

# 查看凭证摘要（加 --show-secrets 才打印 token）
target/release/codex-bridge-login credentials --codex-home "$PWD/.test-codex-home"

# 列出可用模型 / 推理强度 / 速度档位
target/release/codex-bridge-login models --codex-home "$PWD/.test-codex-home"

# 进程集成用的 NDJSON 登录事件流
target/release/codex-bridge-login login-json --codex-home "$PWD/.test-codex-home" --no-open-browser --port 0

# 发一次真实 Responses 请求，事件以 NDJSON 打印
target/release/codex-bridge-http request --codex-home "$PWD/.test-codex-home" --body request.json
```

C ABI 冒烟测试：

```bash
cc examples/c_abi_smoke.c -I include -L target/release -lcodex_bridge \
  -Wl,-rpath,"$PWD/target/release" -o target/c-abi-smoke && ./target/c-abi-smoke

cc http-bridge/examples/c_abi_smoke.c -I http-bridge/include -L target/release -lcodex_http_bridge \
  -Wl,-rpath,"$PWD/target/release" -o target/http-c-abi-smoke && ./target/http-c-abi-smoke
```

## 测试

```bash
cargo test --release -p codex-bridge --lib -j 2
cargo test --release -p codex-http-bridge --lib -j 2
cargo fmt --all -- --check
```

网络相关能力（额度、刷新、模型）的单测只覆盖**发网络请求之前**的参数校验，
真实链路靠上面的命令行自测验证。

## 边界

Bridge 是**上游通信库，不是完整代理服务器**。调用方仍需自己负责：

- 对外 HTTP server 与 SSE 响应
- Chat Completions / Messages 与 Responses 之间的协议转换
- 模型选择、会话映射、多账号调度
- 客户端断开后调用 cancel

尚未实现：WebSocket Bridge、完整 Agent 执行器、MCP 代理、多账号凭证池、独立 HTTP server。

## 更新上游

```bash
git -C upstream/codex fetch origin
git -C upstream/codex checkout origin/main
git add upstream/codex
cargo build --release && cargo test --release
```

上游公开 API 未变时可直接重编。类型、构造函数或 SSE 事件枚举有变更时，**应修复 Bridge 适配层，
不要用忽略未知事件的方式绕过编译错误**。

## 安全提醒

`.test-codex-home/auth.json` 是本地敏感凭证，已被 `.gitignore` 忽略。
不要把它的内容、access token 或 refresh token 写进文档、日志或测试输出。
