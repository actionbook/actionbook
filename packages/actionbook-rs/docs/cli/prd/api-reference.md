# Actionbook CLI v1.0.0 — API Reference

> 源文档: `cli_prd.md`
> 日期: 2026-03-26

---

## 目录

**Part I — 原则与约定**

1. [设计目标与原则](#1-设计目标与原则)
2. [通用约定](#2-通用约定)
3. [错误协议](#3-错误协议)
4. [兼容与边界规则](#4-兼容与边界规则)
5. [测试场景](#5-测试场景)

**Part II — 命令详细定义**

6. [非浏览器命令](#6-非浏览器命令)
7. [Browser Lifecycle](#7-browser-lifecycle)
8. [Browser Tab 管理](#8-browser-tab-管理)
9. [Browser Navigation](#9-browser-navigation)
10. [Browser Observation](#10-browser-observation)
11. [Browser Interaction](#11-browser-interaction)
12. [Browser Waiting](#12-browser-waiting)
13. [Browser Cookies](#13-browser-cookies)
14. [Browser Storage](#14-browser-storage)

---

## 1. 设计目标与原则

### 1.1 设计目标

- 所有命令返回结构统一
- 会话类命令始终显式携带 `session_id` 和 `tab_id`
- 默认文本输出也具备稳定格式，不再只是"尽量可读"
- 这是全新版本协议，不兼容旧版返回格式

### 1.2 设计原则

1. `--json` 一律使用统一 envelope
2. 文本输出一律使用固定头部格式
3. 会话上下文是一级概念，不隐藏在日志里
4. 非会话命令不返回 `context`
5. `session_id` 支持语义化命名
6. `tab_id` 使用短 ID，适合人和 LLM 在终端里复用

### 1.3 ID 约定

#### session_id

- **类型：** 字符串
- **来源：** 可由 `actionbook browser start --set-session-id <SID>` 指定
- **要求：**
  - 对人类和 LLM 有语义
  - 在当前 CLI 生命周期和持久化状态里可唯一定位一个 session
- **示例：** `research-google`, `github-login-debug`, `airbnb-form-fill`

#### tab_id

- **类型：** 短字符串
- **格式：** `t1`, `t2`, `t3`, ...
- **范围：** 在单个 session 内唯一
- **说明：**
  - `tab_id` 是对外 ID
  - 底层浏览器原生 tab handle 如果需要，可额外通过 `native_tab_id` 暴露
  - 不允许用户手动命名 tab

#### 可选内部 ID

如果底层桥接需要，JSON 可附带：
- `native_tab_id`
- `native_window_id`

这些字段仅用于桥接和调试，不作为主要引用 ID。

### 1.4 默认结论

本版本先采纳以下默认方案：

- `session_id` 为语义化字符串，支持用户指定
- `tab_id` 为会话内短 ID `tN`
- `native_tab_id` 仅作为可选调试字段
- 非会话命令省略 `context`
- JSON 与文本输出都属于正式 contract
- 统一 envelope，不延续旧版返回格式

---

## 2. 通用约定

### 2.1 定义

| 符号 | 含义 |
|------|------|
| `<selector>` | ref（`@eN`）、CSS selector 或 XPath |
| `<coordinates>` | 格式为 `x,y` 的坐标 |
| `<SID>` | Session ID，语义化字符串（如 `research-google`） |
| `<TID>` | Tab ID，短 ID 格式 `tN`（如 `t1`, `t2`） |
| `<WID>` | Window ID，短 ID 格式 `wN`（如 `w0`, `w1`） |

### 2.2 Global Flags

所有 `browser` 子命令均可使用：

| Flag | 类型 | 说明 |
|------|------|------|
| `--timeout <ms>` | u64 | 超时时间（毫秒） |
| `--json` | bool | JSON 输出（默认纯文本） |

### 2.3 寻址级别

| 级别 | 要求 | 示例 |
|------|------|------|
| **Global** | 无 session/tab | `browser start`, `browser list-sessions` |
| **Session** | `--session <SID>` | `browser status`, `browser list-tabs`, `cookies *` |
| **Tab** | `--session <SID> --tab <TID>` | `browser goto`, `browser click`, `storage *` |

### 2.4 JSON Envelope（统一响应格式）

```json
{
  "ok": true,
  "command": "browser.snapshot",
  "context": {
    "session_id": "research-google",
    "tab_id": "t1",
    "window_id": "w1",
    "url": "https://google.com",
    "title": "Google"
  },
  "data": {},
  "error": null,
  "meta": {
    "duration_ms": 123,
    "warnings": [],
    "pagination": null,
    "truncated": false
  }
}
```

**顶层字段：**

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `ok` | boolean | 是 | 命令是否成功 |
| `command` | string | 是 | 规范化命令名（如 `browser.snapshot`） |
| `context` | object/null | 否 | 仅会话命令返回 |
| `data` | any/null | 是 | 业务返回值（通常为 object，help/version 为 string；失败时为 `null`） |
| `error` | object/null | 是 | 错误信息（成功时为 `null`） |
| `meta` | object | 是 | 元信息 |

**context 字段规则：**
- `session_id`：会话命令必带
- `tab_id`：tab 级命令必带
- `window_id`：仅多窗口场景返回
- `url` / `title`：当前上下文已知时返回
- 特例：`browser start` 虽然是 Global 级命令（不需要传入 session），但创建 session 后返回 context（含新建的 session_id 和 tab_id）

**error 结构：**

```json
{
  "code": "ELEMENT_NOT_FOUND",
  "message": "Element not found: #submit",
  "retryable": false,
  "details": { "selector": "#submit" }
}
```

**meta 结构：**

```json
{
  "duration_ms": 123,
  "warnings": [],
  "pagination": { "page": 1, "page_size": 10, "total": 42, "has_more": true },
  "truncated": false
}
```

### 2.5 文本输出协议

**非会话命令：** 首行无前缀，直接输出。

**Session 级命令：**
```
[<session_id>]
<body>
```

**Tab 级命令：**
```
[<session_id> <tab_id>] <url>
<body>
```

**通用规则：**
- 读取类命令直接输出正文
- 动作类命令用 `ok <command>` 起头
- 失败统一输出 `error <CODE>: <message>`
- 注意：某些命令的文本输出可能偏离严格格式（如 `restart` 为 Session 级但输出含 tab_id，`close-tab` 为 Tab 级但省略 URL），这些均以 PRD 示例为准

---

## 3. 错误协议

### 3.1 统一错误响应

```json
{
  "ok": false,
  "command": "browser.click",
  "context": { "session_id": "research-google", "tab_id": "t1", "url": "https://google.com" },
  "data": null,
  "error": {
    "code": "ELEMENT_NOT_FOUND",
    "message": "Element not found: button[type=submit]",
    "retryable": false,
    "details": { "selector": "button[type=submit]" }
  },
  "meta": { "duration_ms": 3012, "warnings": [], "pagination": null, "truncated": false }
}
```

**文本输出：**
```
[research-google t1] https://google.com
error ELEMENT_NOT_FOUND: Element not found: button[type=submit]
```

### 3.2 推荐错误码

| 错误码 | 说明 |
|--------|------|
| `INVALID_ARGUMENT` | 参数不合法 |
| `SESSION_NOT_FOUND` | Session 不存在 |
| `TAB_NOT_FOUND` | Tab 不存在 |
| `FRAME_NOT_FOUND` | Frame 不存在 |
| `ELEMENT_NOT_FOUND` | 元素不存在 |
| `MULTIPLE_MATCHES` | `query one` 匹配多于 1 个 |
| `INDEX_OUT_OF_RANGE` | `query nth` 索引超出范围 |
| `TIMEOUT` | 操作超时 |
| `NAVIGATION_FAILED` | 导航失败 |
| `EVAL_FAILED` | JavaScript 执行失败 |
| `ARTIFACT_WRITE_FAILED` | 文件写入失败（screenshot/pdf） |
| `UNSUPPORTED_OPERATION` | 不支持的操作 |
| `INTERNAL_ERROR` | 内部错误 |

---

## 4. 兼容与边界规则

- 非会话命令必须省略 `context`
- 会话命令只要已经定位到 session，就必须返回 `context.session_id`
- tab 命令只要已经定位到 tab，就必须返回 `context.tab_id`
- `data` 必须始终存在；失败时为 `null`
- `error` 必须始终存在；成功时为 `null`
- `command` 使用规范化名字，不使用原始 argv 字符串
- 文本输出不允许混入实现细节日志
- 默认文本输出和 JSON 语义必须一致

---

## 5. 测试场景

### 5.1 基础一致性

- `search` / `get` / `setup` / `help` / `--version` 不返回 `context`
- browser 命令在适用时总返回 `session_id`
- tab 级命令总返回 `tab_id`

### 5.2 ID 规则

- `browser start --set-session-id research-google` 返回 `session_id` = `"research-google"`
- 在同一 session 内依次新建 tab，返回 `t1`, `t2`, `t3`
- `native_tab_id` 可变化，但 `tab_id` 对上层保持稳定

### 5.3 文本格式

- snapshot 的正文不夹杂提示语
- `text` / `html` / `eval` 正文直接输出值
- `click` / `fill` / `upload` / `wait` 第二行固定以 `ok <command>` 开头
- 错误时固定为 `error <CODE>: <message>`

### 5.4 JSON 结构

- 所有命令都有 `ok` / `command` / `data` / `error` / `meta`
- 分页信息只出现在 `meta.pagination`
- 截断信息只出现在 `meta.truncated`

### 5.5 Query 语义

- `browser query one` 仅在恰好 1 个匹配时成功
- `browser query one` 在匹配数大于 1 时返回 `MULTIPLE_MATCHES`
- `browser query one` 在匹配数为 0 时返回 `ELEMENT_NOT_FOUND`
- `browser query nth <n>` 使用 1-based 索引
- `browser query nth <n>` 在 `n > count` 时返回 `INDEX_OUT_OF_RANGE`
- `browser query all` 在 0 个匹配时仍成功，返回空数组
- `browser query count` 在 0 个匹配时仍成功，仅返回 `count`

### 5.6 跳转与上下文更新

- `goto` / `click` / `back` / `forward` / `reload` 成功后更新 `context.url`
- 页面 title 已知时同步更新 `context.title`

---

## 6. 非浏览器命令

这些命令不返回 `context`。

### 6.1 `actionbook search <query>`

搜索 action。

**参数：**

| 参数 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `<query>` | string | 是 | 搜索关键词 |
| `-d, --domain` | string | 否 | 按域名过滤 |
| `-u, --url` | string | 否 | 按 URL 过滤 |
| `-p, --page` | int | 否 | 页码，默认 1 |
| `-s, --page-size` | int | 否 | 每页数量，默认 10 |

**JSON `data`：**

```json
{
  "query": "google login",
  "items": [
    {
      "area_id": "google.com:/login:default",
      "title": "Google Login",
      "summary": "Login form and related actions",
      "score": 0.98,
      "url": "https://google.com/login"
    }
  ]
}
```

**文本输出：**
```
1 result
1. google.com:/login:default
   Google Login
   score: 0.98
   https://google.com/login
```

---

### 6.2 `actionbook get <area_id>`

获取 action 详情。

**参数：**

| 参数 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `<area_id>` | string | 是 | Action 区域 ID |

**JSON `data`：**

```json
{
  "area_id": "google.com:/login:default",
  "url": "https://google.com/login",
  "description": "Login page",
  "elements": [
    {
      "element_id": "email",
      "type": "input",
      "description": "Email input",
      "css": "#identifierId",
      "xpath": null,
      "allow_methods": ["fill", "type", "focus"]
    }
  ]
}
```

**文本输出：**
```
google.com:/login:default
https://google.com/login

Login page

[email] input
description: Email input
css: #identifierId
methods: fill, type, focus
```

---

### 6.3 `actionbook setup`

交互式配置向导。

**参数：**

| 参数 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `--target` | string | 否 | 配置目标 |
| `--api-key` | string | 否 | API Key |
| `--browser` | string | 否 | 浏览器配置 |
| `--non-interactive` | bool | 否 | 非交互模式 |
| `--reset` | bool | 否 | 重置配置 |

> Setup 进入交互式配置流程。PRD 未定义 JSON/text 返回值协议（交互式命令不通过 `--json` 输出）。非交互模式（`--non-interactive`）下的返回值待 PRD 补充。

---

### 6.4 `actionbook help`

**JSON `data`：** `"help text here"` (string)

**文本输出：**
```
actionbook browser <subcommand>

start      Start or attach a browser session
list-tabs  List tabs in a session
snapshot   Capture accessibility snapshot
```

---

### 6.5 `actionbook --version`

**JSON `data`：** `"1.0.0"` (string)

**文本输出：** `1.0.0`

---

## 7. Browser Lifecycle

### 7.1 `actionbook browser start`

> 寻址级别: **Global**
> command: `browser.start`

创建或附着一个 session。可通过 `--set-session-id` 指定 session_id；可选自动打开初始 tab。

**参数：**

| 参数 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `--mode` | `local\|extension\|cloud` | 否 | 浏览器模式，默认 `local` |
| `--headless` | bool | 否 | 是否 headless 模式 |
| `--profile` | string | 否 | session 使用的 profile |
| `--open-url` | string | 否 | 打开浏览器时直接访问此 URL |
| `--cdp-endpoint` | string | 否 | 连接已有 CDP endpoint（不启动新浏览器） |
| `--header <KEY:VALUE>` | string | 否 | 仅 `--cdp-endpoint` 时生效，连接时传递 header |
| `--set-session-id` | string | 否 | 指定语义化 session ID |

**JSON `data`：**

```json
{
  "session": {
    "session_id": "research-google",
    "mode": "local",
    "status": "running",
    "headless": false,
    "cdp_endpoint": "ws://127.0.0.1:9222/devtools/browser/..."
  },
  "tab": {
    "tab_id": "t1",
    "url": "https://google.com",
    "title": "Google",
    "native_tab_id": 391
  },
  "reused": false
}
```

**文本输出：**
```
[research-google t1] https://google.com
ok browser.start
mode: local
status: running
title: Google
```

---

### 7.2 `actionbook browser list-sessions`

> 寻址级别: **Global**
> command: `browser.list-sessions`

列出所有活跃 session。

**参数：** 无

**JSON `data`：**

```json
{
  "total_sessions": 1,
  "sessions": [
    {
      "session_id": "research-google",
      "mode": "local",
      "status": "running",
      "headless": false,
      "tabs_count": 2
    }
  ]
}
```

**文本输出：**
```
1 session
[research-google]
status: running
tabs: 2
```

---

### 7.3 `actionbook browser status --session <SID>`

> 寻址级别: **Session**
> command: `browser.status`

查看指定 session 的详细状态。

**参数：**

| 参数 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `--session <SID>` | string | 是 | Session ID |

**JSON `data`：**

```json
{
  "session": {
    "session_id": "research-google",
    "mode": "local",
    "status": "running",
    "headless": false,
    "tabs_count": 2
  },
  "tabs": [
    {
      "tab_id": "t1",
      "url": "https://google.com",
      "title": "Google"
    }
  ],
  "capabilities": {
    "snapshot": true,
    "pdf": true,
    "upload": true
  }
}
```

**文本输出：**
```
[research-google]
status: running
mode: local
tabs: 2
```

---

### 7.4 `actionbook browser close --session <SID>`

> 寻址级别: **Session**
> command: `browser.close`

关闭指定 session 及其浏览器。

**参数：**

| 参数 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `--session <SID>` | string | 是 | Session ID |

**JSON `data`：**

```json
{
  "session_id": "research-google",
  "status": "closed",
  "closed_tabs": 2
}
```

**文本输出：**
```
[research-google]
ok browser.close
closed_tabs: 2
```

---

### 7.5 `actionbook browser restart --session <SID>`

> 寻址级别: **Session**
> command: `browser.restart`

关闭并以相同 profile/mode 重新启动 session。

**参数：**

| 参数 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `--session <SID>` | string | 是 | Session ID |

**JSON `data`：**

```json
{
  "session": {
    "session_id": "research-google",
    "mode": "local",
    "status": "running",
    "headless": false,
    "tabs_count": 1
  },
  "reopened": true
}
```

**文本输出：**
```
[research-google t1]
ok browser.restart
status: running
```

---

## 8. Browser Tab 管理

### 8.1 `actionbook browser list-tabs --session <SID>`

> 寻址级别: **Session**
> command: `browser.list-tabs`

列出指定 session 中的所有 tab。

**参数：**

| 参数 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `--session <SID>` | string | 是 | Session ID |

**JSON `data`：**

```json
{
  "total_tabs": 1,
  "tabs": [
    {
      "tab_id": "t1",
      "url": "https://google.com",
      "title": "Google",
      "native_tab_id": 391
    }
  ]
}
```

**文本输出：**
```
[research-google]
1 tab
[t1] Google
https://google.com
```

---

### 8.2 `actionbook browser new-tab <url> --session <SID>`

> 寻址级别: **Session**
> command: `browser.new-tab`
> alias: `browser open`

在指定 session 中打开新 tab。

**参数：**

| 参数 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `<url>` | string | 是 | 要打开的 URL |
| `--session <SID>` | string | 是 | Session ID |
| `--new-window` | bool | 否 | 在新窗口中打开 |
| `--window <WID>` | string | 否 | 在指定窗口中打开 |

**JSON `data`：**

```json
{
  "tab": {
    "tab_id": "t2",
    "url": "https://example.com",
    "title": "Example Domain",
    "native_tab_id": 392
  },
  "created": true,
  "new_window": false
}
```

**文本输出：**
```
[research-google t2] https://example.com
ok browser.new-tab
title: Example Domain
```

---

### 8.3 `actionbook browser close-tab --session <SID> --tab <TID>`

> 寻址级别: **Tab**
> command: `browser.close-tab`

关闭指定 tab。

**参数：**

| 参数 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `--session <SID>` | string | 是 | Session ID |
| `--tab <TID>` | string | 是 | Tab ID |

**JSON `data`：**

```json
{
  "closed_tab_id": "t2"
}
```

**文本输出：**
```
[research-google t2]
ok browser.close-tab
```

---

## 9. Browser Navigation

所有 Navigation 命令寻址级别: **Tab**，要求 `--session <SID> --tab <TID>`。

### 9.1 `actionbook browser goto <url>`

> command: `browser.goto`

导航到指定 URL。

**参数：**

| 参数 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `<url>` | string | 是 | 目标 URL |
| `--session <SID>` | string | 是 | Session ID |
| `--tab <TID>` | string | 是 | Tab ID |

**JSON `data`：**

```json
{
  "kind": "goto",
  "requested_url": "https://google.com/search?q=actionbook",
  "from_url": "https://google.com",
  "to_url": "https://google.com/search?q=actionbook",
  "title": "actionbook - Google Search"
}
```

**文本输出：**
```
[research-google t1] https://google.com/search?q=actionbook
ok browser.goto
title: actionbook - Google Search
```

---

### 9.2 `actionbook browser back`

> command: `browser.back`

导航后退。

**参数：** `--session <SID> --tab <TID>`

**JSON `data`：** 与 `goto` 结构相同，`kind` = `"back"`。

> 规则：goto/back/forward/reload 成功后 `context.url` 必须更新为导航后 URL，`context.title` 已知时同步更新。

---

### 9.3 `actionbook browser forward`

> command: `browser.forward`

导航前进。

**参数：** `--session <SID> --tab <TID>`

**JSON `data`：** 与 `goto` 结构相同，`kind` = `"forward"`。

---

### 9.4 `actionbook browser reload`

> command: `browser.reload`

重新加载页面。

**参数：** `--session <SID> --tab <TID>`

**JSON `data`：** 与 `goto` 结构相同，`kind` = `"reload"`。

---

## 10. Browser Observation

所有 Observation 命令寻址级别: **Tab**（除非另行标注）。

### 10.1 `actionbook browser snapshot`

> command: `browser.snapshot`

捕获页面无障碍树快照。

**参数：**

| 参数 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `--session <SID>` | string | 是 | Session ID |
| `--tab <TID>` | string | 是 | Tab ID |
| `--interactive` | bool | 否 | 仅包含可交互元素 |
| `--cursor` | bool | 否 | 额外包含鼠标/焦点可交互的自定义元素（cursor:pointer、onclick、tabindex 等） |
| `--compact` | bool | 否 | 压缩输出，去掉空的结构节点 |
| `--depth <n>` | int | 否 | 限制树的最大深度 |
| `--selector <sel>` | string | 否 | 限定到某个子树 |

**JSON `data`：**

```json
{
  "format": "snapshot",
  "content": "- textbox \"Search\" [ref=e1]\n- button \"Google Search\" [ref=e2]\n",
  "nodes": [
    {
      "ref": "e1",
      "role": "textbox",
      "name": "Search",
      "value": ""
    }
  ],
  "stats": {
    "node_count": 2,
    "interactive_count": 2
  }
}
```

**文本输出：**
```
[research-google t1] https://google.com
- textbox "Search" [ref=e1]
- button "Google Search" [ref=e2]
```

> 规则: 文本模式下直接输出 snapshot 内容，不加额外说明文字。截断时 `meta.truncated = true`，文本可末尾附 `truncated: true`。

---

### 10.2 `actionbook browser screenshot <path>`

> command: `browser.screenshot`

截取页面截图。

**参数：**

| 参数 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `<path>` | string | 是 | 输出文件路径 |
| `--session <SID>` | string | 是 | Session ID |
| `--tab <TID>` | string | 是 | Tab ID |
| `--full` | bool | 否 | 截整页（不只是当前 viewport） |
| `--annotate` | bool | 否 | 叠加编号标签，标出可交互元素，编号 `[N]` 对应 ref `@eN` |
| `--screenshot-quality <0-100>` | int | 否 | JPEG 质量（仅 jpeg 生效） |
| `--screenshot-format <png\|jpeg>` | string | 否 | 图片格式 |
| `--selector <sel>` | string | 否 | 限定到某个子区域 |

**JSON `data`：**

```json
{
  "artifact": {
    "path": "/tmp/google.png",
    "mime_type": "image/png",
    "bytes": 183920
  }
}
```

**文本输出：**
```
[research-google t1] https://google.com
ok browser.screenshot
path: /tmp/google.png
```

---

### 10.3 `actionbook browser pdf <path>`

> command: `browser.pdf`

将当前页面保存为 PDF（类似打印导出）。

**参数：**

| 参数 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `<path>` | string | 是 | 输出文件路径 |
| `--session <SID>` | string | 是 | Session ID |
| `--tab <TID>` | string | 是 | Tab ID |

**JSON `data`：** 与 `screenshot` 相同，`artifact.mime_type` = `"application/pdf"`。

**文本输出：**
```
[research-google t1] https://google.com
ok browser.pdf
path: /tmp/google.pdf
```

---

### 10.4 `actionbook browser title`

> command: `browser.title`

获取页面标题。

**参数：** `--session <SID> --tab <TID>`

**JSON `data`：** `{ "value": "Google" }`

**文本输出：**
```
[research-google t1] https://google.com
Google
```

---

### 10.5 `actionbook browser url`

> command: `browser.url`

获取当前页面 URL。

**参数：** `--session <SID> --tab <TID>`

**JSON `data`：** `{ "value": "https://google.com" }`

**文本输出：**
```
[research-google t1] https://google.com
https://google.com
```

---

### 10.6 `actionbook browser viewport`

> command: `browser.viewport`

获取视口尺寸。

**参数：** `--session <SID> --tab <TID>`

**JSON `data`：** `{ "width": 1440, "height": 900 }`

**文本输出：**
```
[research-google t1] https://google.com
1440x900
```

---

### 10.7 `actionbook browser query <mode> <query_str>`

> command: `browser.query`

带基数约束的元素查询命令。

**子命令形态：**

```
actionbook browser query one <query_str> --session <SID> --tab <TID>
actionbook browser query all <query_str> --session <SID> --tab <TID>
actionbook browser query nth <n> <query_str> --session <SID> --tab <TID>
actionbook browser query count <query_str> --session <SID> --tab <TID>
```

**参数：**

| 参数 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `<mode>` | `one\|all\|nth\|count` | 是 | 查询模式 |
| `<query_str>` | string | 是 | CSS selector 或扩展语法 |
| `<n>` | int | nth 模式必填 | 1-based 索引 |
| `--session <SID>` | string | 是 | Session ID |
| `--tab <TID>` | string | 是 | Tab ID |

**支持的 `query_str` 语法：**
- 标准 CSS selector（`.item`, `#some_id`, `.c > .a > input[name=b]`）
- 扩展语法（参考 jQuery）:
  - `:visible`
  - `:contains(...)`
  - `:has(...)`
  - `:enabled`
  - `:disabled`
  - `:checked`

**匹配元素结构：**

```json
{
  "selector": ".item:nth-of-type(1)",
  "tag": "div",
  "text": "Item A",
  "visible": true,
  "enabled": true
}
```

#### mode = `one`

- 成功条件: 恰好 1 个匹配
- 0 个匹配 → `ELEMENT_NOT_FOUND`
- 多于 1 个匹配 → `MULTIPLE_MATCHES`

**JSON `data`（成功）：**

```json
{
  "mode": "one",
  "query": ".item",
  "count": 1,
  "item": { "selector": ".item:nth-of-type(1)", "tag": "div", "text": "Item A", "visible": true, "enabled": true }
}
```

**JSON `error`（多匹配）：**

```json
{
  "code": "MULTIPLE_MATCHES",
  "message": "Query mode 'one' requires exactly 1 match, found 3",
  "retryable": false,
  "details": { "query": ".item", "count": 3, "sample_selectors": [".item:nth-of-type(1)", ".item:nth-of-type(2)", ".item:nth-of-type(3)"] }
}
```

**文本输出（成功）：**
```
[research-google t1] https://example.com
1 match
selector: .item:nth-of-type(1)
text: Item A
```

#### mode = `all`

- 始终返回列表
- 0 个匹配也视为成功，`items = []`

**JSON `data`：**

```json
{
  "mode": "all",
  "query": ".item",
  "count": 3,
  "items": [
    { "selector": ".item:nth-of-type(1)", "tag": "div", "text": "Item A", "visible": true, "enabled": true }
  ]
}
```

**文本输出：**
```
[research-google t1] https://example.com
3 matches
1. .item:nth-of-type(1)
   Item A
```

#### mode = `nth <n>`

- `n` 为 1-based
- `n > count` → `INDEX_OUT_OF_RANGE`

**JSON `data`：**

```json
{
  "mode": "nth",
  "query": ".item",
  "index": 2,
  "count": 3,
  "item": { "selector": ".item:nth-of-type(2)", "tag": "div", "text": "Item B", "visible": true, "enabled": true }
}
```

**文本输出：**
```
[research-google t1] https://example.com
match 2/3
selector: .item:nth-of-type(2)
text: Item B
```

#### mode = `count`

- 只返回匹配数量，不返回元素详情
- 0 个匹配也成功

**JSON `data`：**

```json
{
  "mode": "count",
  "query": ".item",
  "count": 3
}
```

**文本输出：**
```
[research-google t1] https://example.com
3
```

---

### 10.8 读取类命令（html / text / value / attr / attrs / box / styles）

统一寻址: **Tab**。统一 JSON 结构。

#### `actionbook browser html <selector>`

> command: `browser.html`

获取元素的 outer HTML。

**参数：** `<selector>` (必选), `--session <SID> --tab <TID>`

**JSON `data`：**

```json
{
  "target": { "selector": "#title" },
  "value": "<h1 id=\"title\">Example Domain</h1>"
}
```

**文本输出：**
```
[research-google t1] https://example.com
<h1 id="title">Example Domain</h1>
```

#### `actionbook browser text <selector>`

> command: `browser.text`

获取元素的 inner text。

**参数：**

| 参数 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `<selector>` | string | 是 | 目标元素 |
| `--mode <raw\|readability>` | string | 否 | 文本提取模式 |
| `--session <SID>` | string | 是 | Session ID |
| `--tab <TID>` | string | 是 | Tab ID |

**JSON `data`：** `{ "target": { "selector": "#title" }, "value": "Example Domain" }`

**文本输出：**
```
[research-google t1] https://example.com
Example Domain
```

#### `actionbook browser value <selector>`

> command: `browser.value`

获取 input 元素的值。

**参数：** `<selector>` (必选), `--session <SID> --tab <TID>`

**JSON `data`：** `{ "target": { "selector": "#email" }, "value": "user@example.com" }`

#### `actionbook browser attr <selector> <name>`

> command: `browser.attr`

获取元素的指定属性。

**参数：** `<selector>` (必选), `<name>` (必选), `--session <SID> --tab <TID>`

**JSON `data`：** `{ "target": { "selector": "a.link" }, "value": "https://google.com" }`

**文本输出：**
```
[research-google t1] https://example.com
https://google.com
```

#### `actionbook browser attrs <selector>`

> command: `browser.attrs`

获取元素的全部属性。

**参数：** `<selector>` (必选), `--session <SID> --tab <TID>`

**JSON `data`：** `{ "target": { "selector": "#btn" }, "value": { "id": "btn", "type": "submit", "class": "primary" } }`

**文本输出：** key-value 列表。

#### `actionbook browser box <selector>`

> command: `browser.box`

获取元素的 bounding box。

**参数：** `<selector>` (必选), `--session <SID> --tab <TID>`

**JSON `data`：** `{ "target": { "selector": "#btn" }, "value": { "x": 10, "y": 20, "width": 120, "height": 32 } }`

**文本输出：**
```
[research-google t1] https://example.com
x: 10
y: 20
width: 120
height: 32
```

#### `actionbook browser styles <selector> [names...]`

> command: `browser.styles`

获取元素的计算样式。

**参数：** `<selector>` (必选), `[names...]` (可选，指定属性名), `--session <SID> --tab <TID>`

**JSON `data`：** `{ "target": { "selector": "#btn" }, "value": { "color": "rgb(0,0,0)", "font-size": "14px" } }`

**文本输出：** 请求到的 style 名和值。

---

### 10.9 `actionbook browser describe <selector>`

> command: `browser.describe`

返回元素的规则化摘要（确定性生成，不调用 LLM）。

**参数：**

| 参数 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `<selector>` | string | 是 | 目标元素 |
| `--nearby` | bool | 否 | 额外返回一层浅邻近上下文 |
| `--session <SID>` | string | 是 | Session ID |
| `--tab <TID>` | string | 是 | Tab ID |

**摘要生成规则：**
- 基于 DOM tag、ARIA role、accessible name、label/text/placeholder/value、关键属性（type、href）、当前状态（disabled、checked、selected）
- 不做额外语义推断
- 拼装优先级: role → name → qualifiers

**`--nearby` 约束：**
- 只返回 1 层，不递归
- parent 最多 1 个
- previous_sibling / next_sibling 最多各 1 个
- children 最多 3 个
- 仅返回有名字、文本或可交互意义的节点

**JSON `data`（无 --nearby）：**

```json
{
  "target": { "selector": "button[type=submit]" },
  "summary": "button \"Google Search\"",
  "role": "button",
  "name": "Google Search",
  "tag": "button",
  "attributes": { "type": "submit" },
  "state": { "visible": true, "enabled": true },
  "nearby": null
}
```

**JSON `data`（--nearby）：**

```json
{
  "target": { "selector": "button[type=submit]" },
  "summary": "button \"Edit\"",
  "role": "button",
  "name": "Edit",
  "tag": "button",
  "attributes": { "type": "button" },
  "state": { "visible": true, "enabled": true },
  "nearby": {
    "parent": "listitem \"John Smith\"",
    "previous_sibling": "text \"John Smith\"",
    "next_sibling": null,
    "children": []
  }
}
```

**文本输出：**
```
[research-google t1] https://google.com
button "Google Search"
```

**文本输出（--nearby）：**
```
[research-google t1] https://google.com
button "Edit"
parent: listitem "John Smith"
previous_sibling: text "John Smith"
```

---

### 10.10 `actionbook browser state <selector>`

> command: `browser.state`

返回元素的交互状态。

**参数：** `<selector>` (必选), `--session <SID> --tab <TID>`

**JSON `data`：**

```json
{
  "target": { "selector": "#search" },
  "state": {
    "visible": true,
    "enabled": true,
    "checked": false,
    "focused": true,
    "editable": true,
    "selected": false
  }
}
```

**文本输出：**
```
[research-google t1] https://google.com
visible: true
enabled: true
checked: false
focused: true
editable: true
selected: false
```

---

### 10.11 `actionbook browser inspect-point <coordinates>`

> command: `browser.inspect-point`

检查指定坐标处的元素（建议配合 screenshot 使用）。

**参数：**

| 参数 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `<coordinates>` | string | 是 | 格式 `x,y` |
| `--parent-depth <n>` | int | 否 | 向上追溯的父元素层数 |
| `--session <SID>` | string | 是 | Session ID |
| `--tab <TID>` | string | 是 | Tab ID |

**JSON `data`：**

```json
{
  "point": { "x": 420, "y": 310 },
  "element": {
    "role": "button",
    "name": "Google Search",
    "selector": "input[name=btnK]"
  },
  "parents": [],
  "screenshot_path": null
}
```

**文本输出：**
```
[research-google t1] https://google.com
button "Google Search"
selector: input[name=btnK]
point: 420,310
```

---

### 10.12 `actionbook browser logs console`

> command: `browser.logs.console`

获取控制台日志。

**参数：**

| 参数 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `--session <SID>` | string | 是 | Session ID |
| `--tab <TID>` | string | 是 | Tab ID |
| `--level <level[,level...]>` | string | 否 | 按级别过滤（逗号分隔多值） |
| `--tail <n>` | int | 否 | 只返回最后 n 条 |
| `--since <id>` | string | 否 | 只返回指定 ID 之后的日志 |
| `--clear` | bool | 否 | 获取后清除日志 |

**JSON `data`：**

```json
{
  "items": [
    {
      "id": "log-1",
      "level": "info",
      "text": "App mounted",
      "source": "app.js",
      "timestamp_ms": 1710000000000
    }
  ],
  "cleared": false
}
```

**文本输出：**
```
[research-google t1] https://example.com
1 log
info 1710000000000 app.js App mounted
```

---

### 10.13 `actionbook browser logs errors`

> command: `browser.logs.errors`

获取错误日志。

**参数：**

| 参数 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `--session <SID>` | string | 是 | Session ID |
| `--tab <TID>` | string | 是 | Tab ID |
| `--source <file>` | string | 否 | 按错误来源文件过滤 |
| `--tail <n>` | int | 否 | 只返回最后 n 条 |
| `--since <id>` | string | 否 | 只返回指定 ID 之后的日志 |
| `--clear` | bool | 否 | 获取后清除日志 |

**JSON `data`：** 与 `logs console` 相同结构。

---

## 11. Browser Interaction

所有 Interaction 命令寻址级别: **Tab**。

### 11.1 动作类命令（click / hover / focus / press / drag / mouse-move / scroll）

统一 JSON `data` 结构：

```json
{
  "action": "click",
  "target": { "selector": "button[type=submit]" },
  "changed": {
    "url_changed": false,
    "focus_changed": true
  }
}
```

**文本输出：**
```
[research-google t1] https://google.com
ok browser.click
target: button[type=submit]
```

#### `actionbook browser click <selector|coordinates>`

> command: `browser.click`

**参数：**

| 参数 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `<selector\|coordinates>` | string | 是 | 目标元素或坐标 |
| `--session <SID>` | string | 是 | Session ID |
| `--tab <TID>` | string | 是 | Tab ID |
| `--new-tab` | bool | 否 | 若目标有 href，在新 tab 中打开 |
| `--button <left\|right\|middle>` | string | 否 | 鼠标按键，默认 left |
| `--count <n>` | int | 否 | 点击次数（2 = 双击） |

> 特殊规则：若 click 导致跳转，`context.url` 必须更新为跳转后 URL。

#### `actionbook browser hover <selector>`

> command: `browser.hover`

**参数：** `<selector>` (必选), `--session <SID> --tab <TID>`

#### `actionbook browser focus <selector>`

> command: `browser.focus`

**参数：** `<selector>` (必选), `--session <SID> --tab <TID>`

#### `actionbook browser press <key-or-chord>`

> command: `browser.press`

**参数：**

| 参数 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `<key-or-chord>` | string | 是 | 单键或组合键（如 `Enter`, `Control+A`, `Shift+Tab`） |
| `--session <SID>` | string | 是 | Session ID |
| `--tab <TID>` | string | 是 | Tab ID |

> 特殊规则：chord 时 target 可省略，改为 `keys`。

#### `actionbook browser drag <selector> <selector|coordinates>`

> command: `browser.drag`

**参数：**

| 参数 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `<selector>` | string | 是 | 拖拽源元素 |
| `<selector\|coordinates>` | string | 是 | 放置目标元素或坐标 |
| `--session <SID>` | string | 是 | Session ID |
| `--tab <TID>` | string | 是 | Tab ID |
| `--button <left\|right\|middle>` | string | 否 | 鼠标按键 |

#### `actionbook browser mouse-move <coordinates>`

> command: `browser.mouse-move`

移动鼠标到绝对坐标。

**参数：** `<coordinates>` (必选, 格式 `x,y`), `--session <SID> --tab <TID>`

#### `actionbook browser cursor-position`

> command: `browser.cursor-position`

获取当前鼠标位置。

**参数：** `--session <SID> --tab <TID>`

**JSON `data`：** `{ "x": 420, "y": 310 }`

#### `actionbook browser scroll`

> command: `browser.scroll`

滚动页面或容器。

**三种子形态：**

```
actionbook browser scroll up|down|left|right <pixels> --session <SID> --tab <TID> [--container <selector>]
actionbook browser scroll top|bottom --session <SID> --tab <TID> [--container <selector>]
actionbook browser scroll into-view <selector> --session <SID> --tab <TID> [--align <start|center|end|nearest>]
```

**参数：**

| 参数 | 类型 | 必填 | 说明 |
|------|------|------|------|
| 方向/动作 | `up\|down\|left\|right\|top\|bottom\|into-view` | 是 | 滚动方式 |
| `<pixels>` | int | 方向滚动时必填 | 滚动像素数 |
| `<selector>` | string | `into-view` 时必填 | 目标元素 |
| `--container <selector>` | string | 否 | 在指定容器内滚动 |
| `--align <start\|center\|end\|nearest>` | string | 否 | `into-view` 的对齐方式 |

> 特殊规则：`data.changed.scroll_changed = true` 表示产生了位移。

---

### 11.2 输入类命令（type / fill / select / upload）

统一 JSON `data` 结构：

```json
{
  "action": "fill",
  "target": { "selector": "textarea[name=q]" },
  "value_summary": { "text_length": 10 }
}
```

**文本输出：**
```
[research-google t1] https://google.com
ok browser.fill
target: textarea[name=q]
text_length: 10
```

#### `actionbook browser type <selector> <text>`

> command: `browser.type`

逐字符输入文本（触发键盘事件）。

**参数：**

| 参数 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `<selector>` | string | 是 | 目标元素 |
| `<text>` | string | 是 | 要输入的文本 |
| `--session <SID>` | string | 是 | Session ID |
| `--tab <TID>` | string | 是 | Tab ID |

#### `actionbook browser fill <selector> <text>`

> command: `browser.fill`

直接设置输入框的值（触发 input 事件）。

**参数：** 与 `type` 相同。

#### `actionbook browser select <selector> <value>`

> command: `browser.select`

从下拉列表中选择值。

**参数：**

| 参数 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `<selector>` | string | 是 | `<select>` 元素 |
| `<value>` | string | 是 | 要选择的值 |
| `--by-text` | bool | 否 | 使用显示文本而非 value 属性匹配 |
| `--session <SID>` | string | 是 | Session ID |
| `--tab <TID>` | string | 是 | Tab ID |

> `value_summary` = `{ "value": "...", "by_text": true|false }`

#### `actionbook browser upload <selector> <file...>`

> command: `browser.upload`

上传文件到文件输入框。

**参数：**

| 参数 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `<selector>` | string | 是 | 文件输入元素 |
| `<file...>` | string[] | 是 | 绝对文件路径（支持多个） |
| `--session <SID>` | string | 是 | Session ID |
| `--tab <TID>` | string | 是 | Tab ID |

> `value_summary` = `{ "files": ["/abs/path/a.pdf"], "count": 1 }`

---

### 11.3 `actionbook browser eval <code>`

> command: `browser.eval`

在页面上下文中执行 JavaScript。

**参数：**

| 参数 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `<code>` | string | 是 | JavaScript 表达式 |
| `--session <SID>` | string | 是 | Session ID |
| `--tab <TID>` | string | 是 | Tab ID |

**JSON `data`：**

```json
{
  "value": 42,
  "type": "number",
  "preview": "42"
}
```

**文本输出：**
```
[research-google t1] https://example.com
42
```

---

## 12. Browser Waiting

所有 Waiting 命令寻址级别: **Tab**。

统一 JSON `data` 结构：

```json
{
  "kind": "element",
  "satisfied": true,
  "elapsed_ms": 182,
  "observed_value": { "selector": "#loaded" }
}
```

**文本输出：**
```
[research-google t1] https://example.com
ok browser.wait.element
elapsed_ms: 182
```

### 12.1 `actionbook browser wait element <selector>`

> command: `browser.wait.element`

等待元素出现在 DOM 中。

**参数：**

| 参数 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `<selector>` | string | 是 | 等待的元素选择器 |
| `--session <SID>` | string | 是 | Session ID |
| `--tab <TID>` | string | 是 | Tab ID |
| `--timeout <ms>` | u64 | 是 | 超时时间（毫秒） |

**`data.kind`** = `"element"`
**`data.observed_value`** = `{ "selector": "#loaded" }`

---

### 12.2 `actionbook browser wait navigation`

> command: `browser.wait.navigation`

等待导航完成。

**参数：**

| 参数 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `--session <SID>` | string | 是 | Session ID |
| `--tab <TID>` | string | 是 | Tab ID |
| `--timeout <ms>` | u64 | 是 | 超时时间（毫秒） |

**`data.kind`** = `"navigation"`

---

### 12.3 `actionbook browser wait network-idle`

> command: `browser.wait.network-idle`

等待网络空闲。

**参数：**

| 参数 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `<selector>` | string | **待确认** | PRD 含此参数，但 network-idle 通常是全局状态，语义待确认 |
| `--session <SID>` | string | 是 | Session ID |
| `--tab <TID>` | string | 是 | Tab ID |
| `--timeout <ms>` | u64 | 是 | 超时时间（毫秒） |

**`data.kind`** = `"network-idle"`

> ⚠️ **PRD 待确认**: `<selector>` 参数的语义不明确，可能是 PRD 笔误。

---

### 12.4 `actionbook browser wait condition <expression>`

> command: `browser.wait.condition`

等待 JS 表达式为 truthy。

**参数：**

| 参数 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `<expression>` | string | 是 | JavaScript 表达式（应返回 truthy 值） |
| `--session <SID>` | string | 是 | Session ID |
| `--tab <TID>` | string | 是 | Tab ID |
| `--timeout <ms>` | u64 | 是 | 超时时间（毫秒） |

**`data.kind`** = `"condition"`

---

## 13. Browser Cookies

所有 Cookies 命令寻址级别: **Session**（仅 `--session`，不需要 `--tab`）。

### 13.1 `actionbook browser cookies list`

> command: `browser.cookies.list`

**参数：**

| 参数 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `--session <SID>` | string | 是 | Session ID |
| `--domain <domain>` | string | 否 | 按域名过滤 |

**JSON `data`：**

```json
{
  "items": [
    {
      "name": "SID",
      "value": "xxx",
      "domain": ".google.com",
      "path": "/",
      "http_only": true,
      "secure": true,
      "same_site": "Lax",
      "expires": null
    }
  ]
}
```

**文本输出：**
```
[research-google]
1 cookie
SID .google.com /
```

---

### 13.2 `actionbook browser cookies get <name>`

> command: `browser.cookies.get`

**参数：**

| 参数 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `<name>` | string | 是 | Cookie 名称 |
| `--session <SID>` | string | 是 | Session ID |

**JSON `data`：** `{ "item": { ...cookie 对象 } }`

---

### 13.3 `actionbook browser cookies set <name> <value>`

> command: `browser.cookies.set`

**参数：**

| 参数 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `<name>` | string | 是 | Cookie 名称 |
| `<value>` | string | 是 | Cookie 值 |
| `--session <SID>` | string | 是 | Session ID |
| `--domain` | string | 否 | 域名 |
| `--path` | string | 否 | 路径 |
| `--secure` | bool | 否 | Secure 标志 |
| `--http-only` | bool | 否 | HttpOnly 标志 |
| `--same-site <Strict\|Lax\|None>` | string | 否 | SameSite 策略 |
| `--expires <timestamp>` | f64 | 否 | 过期时间（Unix 时间戳） |

**JSON `data`：** `{ "action": "set", "affected": 1, "domain": ".google.com" }`

---

### 13.4 `actionbook browser cookies delete <name>`

> command: `browser.cookies.delete`

**参数：** `<name>` (必选), `--session <SID>`

**JSON `data`：** `{ "action": "delete", "affected": 1 }`

---

### 13.5 `actionbook browser cookies clear`

> command: `browser.cookies.clear`

**参数：**

| 参数 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `--session <SID>` | string | 是 | Session ID |
| `--domain <domain>` | string | 否 | 按域名过滤 |

**JSON `data`：** `{ "action": "clear", "affected": 5, "domain": ".google.com" }`

---

## 14. Browser Storage

所有 Storage 命令寻址级别: **Tab**（`--session <SID> --tab <TID>`）。

两种存储类型共享相同的子命令结构：
- `session-storage` → `window.sessionStorage`
- `local-storage` → `window.localStorage`

### 14.1 `actionbook browser session-storage|local-storage list`

> command: `browser.local-storage.list` / `browser.session-storage.list`

**参数：** `--session <SID> --tab <TID>`

**JSON `data`：**

```json
{
  "storage": "local",
  "items": [
    { "key": "theme", "value": "dark" }
  ]
}
```

**文本输出：**
```
[research-google t1] https://example.com
1 key
theme=dark
```

---

### 14.2 `actionbook browser session-storage|local-storage get <key>`

> command: `browser.local-storage.get` / `browser.session-storage.get`

**参数：** `<key>` (必选), `--session <SID> --tab <TID>`

**JSON `data`：** `{ "storage": "local", "item": { "key": "theme", "value": "dark" } }`

---

### 14.3 `actionbook browser session-storage|local-storage set <key> <value>`

> command: `browser.local-storage.set` / `browser.session-storage.set`

**参数：** `<key>` (必选), `<value>` (必选), `--session <SID> --tab <TID>`

**JSON `data`：** `{ "storage": "local", "action": "set", "affected": 1 }`

---

### 14.4 `actionbook browser session-storage|local-storage delete <key>`

> command: `browser.local-storage.delete` / `browser.session-storage.delete`

**参数：** `<key>` (必选), `--session <SID> --tab <TID>`

**JSON `data`：** `{ "storage": "local", "action": "delete", "affected": 1 }`

---

### 14.5 `actionbook browser session-storage|local-storage clear <key>`

> command: `browser.local-storage.clear` / `browser.session-storage.clear`

清空指定 key 的存储值。

**参数：** `<key>` (必选), `--session <SID> --tab <TID>`

**JSON `data`：** `{ "storage": "local", "action": "clear", "affected": 1 }`

---

## 附录: 命令总览（共 70 个接口）

### 非浏览器命令（5 个）

| # | 命令 | 寻址级别 | command 名 |
|---|------|----------|-----------|
| 1 | `search <query>` | — | `search` |
| 2 | `get <area_id>` | — | `get` |
| 3 | `setup` | — | `setup` |
| 4 | `help` | — | `help` |
| 5 | `--version` | — | `version` |

### Browser Lifecycle（5 个）

| # | 命令 | 寻址级别 | command 名 |
|---|------|----------|-----------|
| 6 | `browser start` | Global | `browser.start` |
| 7 | `browser list-sessions` | Global | `browser.list-sessions` |
| 8 | `browser status` | Session | `browser.status` |
| 9 | `browser close` | Session | `browser.close` |
| 10 | `browser restart` | Session | `browser.restart` |

### Browser Tab 管理（3 个）

| # | 命令 | 寻址级别 | command 名 |
|---|------|----------|-----------|
| 11 | `browser list-tabs` | Session | `browser.list-tabs` |
| 12 | `browser new-tab` / `open` | Session | `browser.new-tab` |
| 13 | `browser close-tab` | Tab | `browser.close-tab` |

### Browser Navigation（4 个）

| # | 命令 | 寻址级别 | command 名 |
|---|------|----------|-----------|
| 14 | `browser goto` | Tab | `browser.goto` |
| 15 | `browser back` | Tab | `browser.back` |
| 16 | `browser forward` | Tab | `browser.forward` |
| 17 | `browser reload` | Tab | `browser.reload` |

### Browser Observation（17 个）

| # | 命令 | 寻址级别 | command 名 |
|---|------|----------|-----------|
| 18 | `browser snapshot` | Tab | `browser.snapshot` |
| 19 | `browser screenshot` | Tab | `browser.screenshot` |
| 20 | `browser pdf` | Tab | `browser.pdf` |
| 21 | `browser title` | Tab | `browser.title` |
| 22 | `browser url` | Tab | `browser.url` |
| 23 | `browser viewport` | Tab | `browser.viewport` |
| 24 | `browser query` | Tab | `browser.query` |
| 25 | `browser html` | Tab | `browser.html` |
| 26 | `browser text` | Tab | `browser.text` |
| 27 | `browser value` | Tab | `browser.value` |
| 28 | `browser attr` | Tab | `browser.attr` |
| 29 | `browser attrs` | Tab | `browser.attrs` |
| 30 | `browser box` | Tab | `browser.box` |
| 31 | `browser styles` | Tab | `browser.styles` |
| 32 | `browser describe` | Tab | `browser.describe` |
| 33 | `browser state` | Tab | `browser.state` |
| 34 | `browser inspect-point` | Tab | `browser.inspect-point` |

### Browser Logging（2 个）

| # | 命令 | 寻址级别 | command 名 |
|---|------|----------|-----------|
| 35 | `browser logs console` | Tab | `browser.logs.console` |
| 36 | `browser logs errors` | Tab | `browser.logs.errors` |

### Browser Interaction（15 个）

| # | 命令 | 寻址级别 | command 名 |
|---|------|----------|-----------|
| 37 | `browser click` | Tab | `browser.click` |
| 38 | `browser type` | Tab | `browser.type` |
| 39 | `browser fill` | Tab | `browser.fill` |
| 40 | `browser select` | Tab | `browser.select` |
| 41 | `browser hover` | Tab | `browser.hover` |
| 42 | `browser focus` | Tab | `browser.focus` |
| 43 | `browser press` | Tab | `browser.press` |
| 44 | `browser drag` | Tab | `browser.drag` |
| 45 | `browser upload` | Tab | `browser.upload` |
| 46 | `browser eval` | Tab | `browser.eval` |
| 47 | `browser mouse-move` | Tab | `browser.mouse-move` |
| 48 | `browser cursor-position` | Tab | `browser.cursor-position` |
| 49 | `browser scroll (direction)` | Tab | `browser.scroll` |
| 50 | `browser scroll (top/bottom)` | Tab | `browser.scroll` |
| 51 | `browser scroll into-view` | Tab | `browser.scroll` |

### Browser Waiting（4 个）

| # | 命令 | 寻址级别 | command 名 |
|---|------|----------|-----------|
| 52 | `browser wait element` | Tab | `browser.wait.element` |
| 53 | `browser wait navigation` | Tab | `browser.wait.navigation` |
| 54 | `browser wait network-idle` | Tab | `browser.wait.network-idle` |
| 55 | `browser wait condition` | Tab | `browser.wait.condition` |

### Browser Cookies（5 个）

| # | 命令 | 寻址级别 | command 名 |
|---|------|----------|-----------|
| 56 | `browser cookies list` | Session | `browser.cookies.list` |
| 57 | `browser cookies get` | Session | `browser.cookies.get` |
| 58 | `browser cookies set` | Session | `browser.cookies.set` |
| 59 | `browser cookies delete` | Session | `browser.cookies.delete` |
| 60 | `browser cookies clear` | Session | `browser.cookies.clear` |

### Browser Storage（10 个）

| # | 命令 | 寻址级别 | command 名 |
|---|------|----------|-----------|
| 61 | `browser session-storage list` | Tab | `browser.session-storage.list` |
| 62 | `browser session-storage get` | Tab | `browser.session-storage.get` |
| 63 | `browser session-storage set` | Tab | `browser.session-storage.set` |
| 64 | `browser session-storage delete` | Tab | `browser.session-storage.delete` |
| 65 | `browser session-storage clear` | Tab | `browser.session-storage.clear` |
| 66 | `browser local-storage list` | Tab | `browser.local-storage.list` |
| 67 | `browser local-storage get` | Tab | `browser.local-storage.get` |
| 68 | `browser local-storage set` | Tab | `browser.local-storage.set` |
| 69 | `browser local-storage delete` | Tab | `browser.local-storage.delete` |
| 70 | `browser local-storage clear` | Tab | `browser.local-storage.clear` |

**总计: 70 个接口**

---

## 附录: PRD 待确认项

| # | 位置 | 问题 |
|---|------|------|
| 1 | `wait network-idle` | PRD 含 `<selector>` 位置参数，但 network-idle 通常是全局状态，`<selector>` 的语义待确认 |
