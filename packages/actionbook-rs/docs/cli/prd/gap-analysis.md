# CLI v1.0.0 PRD vs 代码现状差异分析

> 基准: PRD `/Users/junliangfeng/Downloads/Actionbook CLI v1.0.0.pdf`
> 代码: `packages/actionbook-rs/src/cli.rs` + 相关实现文件
> 日期: 2026-03-25

---

## 一、Config & Env 差异

### 1.1 Config 结构差异

| 项目 | PRD 定义 | 当前代码 | 状态 |
|------|---------|---------|------|
| browser mode | `mode = "local" \| "extension" \| "cloud"` | `mode = "isolated" \| "extension"` | ⚠️ 需调整: `isolated` → `local`, 新增 `cloud` |
| 浏览器路径 | `executable_path` | `executable` | ⚠️ 需重命名 |
| Profile 名称 | `profile_name` | `default_profile` | ⚠️ 需重命名 |
| headless | `headless = true \| false` | `headless = false` | ✅ 一致 |

### 1.2 环境变量差异

| PRD 环境变量 | 当前代码环境变量 | 状态 |
|-------------|----------------|------|
| `ACTIONBOOK_BASE_URL` | 无直接对应（config.toml 中 `api.base_url`） | ⚠️ 需新增 |
| `ACTIONBOOK_API_KEY` | `ACTIONBOOK_API_KEY` | ✅ 一致 |
| `ACTIONBOOK_BROWSER_MODE` | `ACTIONBOOK_BROWSER_MODE` | ✅ 一致（值域需调整） |
| `ACTIONBOOK_BROWSER_HEADLESS` | `ACTIONBOOK_HEADLESS` | ⚠️ 需重命名 |
| `ACTIONBOOK_BROWSER_EXECUTABLE_PATH` | `ACTIONBOOK_BROWSER_PATH` | ⚠️ 需重命名 |
| `ACTIONBOOK_BROWSER_PROFILE_NAME` | `ACTIONBOOK_PROFILE` | ⚠️ 需重命名 |

### 1.3 需删除的环境变量/Global Flags

以下当前代码中的全局 flags 在 PRD 中 **不存在**，需删除：

| 当前 Flag | 环境变量 | 说明 |
|----------|---------|------|
| `--cdp` | `ACTIONBOOK_CDP` | 合并到 `browser start --cdp-endpoint` |
| `--stealth` | `ACTIONBOOK_STEALTH` | PRD 无此功能 |
| `--stealth-os` | `ACTIONBOOK_STEALTH_OS` | PRD 无此功能 |
| `--stealth-gpu` | `ACTIONBOOK_STEALTH_GPU` | PRD 无此功能 |
| `--browser-mode` | `ACTIONBOOK_BROWSER_MODE` | 移到 config 和 `browser start --mode` |
| `--extension` | `ACTIONBOOK_EXTENSION` | 已废弃，PRD 无 |
| `--extension-port` | `ACTIONBOOK_EXTENSION_PORT` | 已废弃，PRD 无 |
| `-v, --verbose` | 无 | PRD 无 |
| `--block-images` | `ACTIONBOOK_BLOCK_IMAGES` | PRD 无 |
| `--block-media` | `ACTIONBOOK_BLOCK_MEDIA` | PRD 无 |
| `--no-animations` | `ACTIONBOOK_NO_ANIMATIONS` | PRD 无 |
| `--auto-dismiss-dialogs` | `ACTIONBOOK_AUTO_DISMISS_DIALOGS` | PRD 无 |
| `--session-tag` | `ACTIONBOOK_SESSION_TAG` | PRD 无 |
| `--rewrite-urls` | `ACTIONBOOK_REWRITE_URLS` | PRD 无 |
| `--wait-hint` | `ACTIONBOOK_WAIT_HINT` | PRD 无 |
| `--camofox` | `ACTIONBOOK_CAMOFOX` | PRD 无 |
| `--camofox-port` | `ACTIONBOOK_CAMOFOX_PORT` | PRD 无 |
| `--no-daemon` | `ACTIONBOOK_NO_DAEMON` | PRD 无（daemon 是内部实现细节） |
| `--auto-connect` | `ACTIONBOOK_AUTO_CONNECT` | PRD 无 |
| `-P, --profile` | `ACTIONBOOK_PROFILE` | 移到 config 和 `browser start --profile` |
| `-S, --session` | `ACTIONBOOK_SESSION` | 移到每个命令的 `--session` 参数 |
| `--browser-path` | `ACTIONBOOK_BROWSER_PATH` | 移到 config |

### 1.4 PRD Global Flags（仅两个）

PRD 的 browser 命令只保留两个全局 flag：
- `--timeout <ms>` — 超时时间
- `--json` — JSON 输出（默认纯文本）

---

## 二、已实现需调整的命令

### 核心结构性变更

PRD 的核心设计变更（参见 core_design.md 原则 "绝对路径式寻址"）：
1. **所有 per-tab 命令必须带 `--session <SID>` + `--tab <TID>`**
2. **Session/Tab ID 是显式短 ID（s0, t3），不再有隐式"当前 tab"**
3. **移除所有 `--ref` / `--human` / `--wait`(pre-action delay) 等非 PRD flag**

### 2.1 Navigation 命令

#### `browser goto`
| 项目 | PRD | 当前代码 |
|------|-----|---------|
| 签名 | `goto <url> --session <SID> --tab <TID>` | `goto <url> [--timeout 30000]` |
| 变更 | 增加 `--session`, `--tab`；移除 `--timeout`（使用全局 `--timeout`） | |

#### `browser back`
| 项目 | PRD | 当前代码 |
|------|-----|---------|
| 签名 | `back --session <SID> --tab <TID>` | `back` |
| 变更 | 增加 `--session`, `--tab` | |

#### `browser forward`
| 项目 | PRD | 当前代码 |
|------|-----|---------|
| 签名 | `forward --session <SID> --tab <TID>` | `forward` |
| 变更 | 增加 `--session`, `--tab` | |

#### `browser reload`
| 项目 | PRD | 当前代码 |
|------|-----|---------|
| 签名 | `reload --session <SID> --tab <TID>` | `reload` |
| 变更 | 增加 `--session`, `--tab` | |

#### `browser open`
| 项目 | PRD | 当前代码 |
|------|-----|---------|
| PRD 签名 | `open <url> --session SID [--new-window] [--window WID]` | `open <url> [--new-window]` |
| 语义变更 | PRD 中 `open` 是 `new-tab` 的 alias，在指定 session 中开新 tab | 当前直接在 browser 中开新 tab |
| 变更 | 增加 `--session`(必选), 增加 `--window` | |

### 2.2 Observation 命令

#### `browser snapshot`
| 项目 | PRD | 当前代码 |
|------|-----|---------|
| 签名 | `snapshot --session <SID> --tab <TID> [flags]` | `snapshot [flags]` |
| PRD flags | `--interactive`, `--cursor`, `--compact`, `--depth <n>`, `--selector <sel>` | 同 + `--format`, `--diff`, `--max-tokens` |
| 变更 | 增加 `--session`, `--tab`；移除 `--format`, `--diff`, `--max-tokens`（PRD 未定义） | |

#### `browser screenshot`
| 项目 | PRD | 当前代码 |
|------|-----|---------|
| 签名 | `screenshot <path> --session <SID> --tab <TID> [flags]` | `screenshot [path=screenshot.png] [--full-page]` |
| PRD 新增 flags | `--full`, `--annotate`, `--screenshot-quality <0-100>`, `--screenshot-format <png\|jpeg>`, `--selector <sel>` | |
| 变更 | `--full-page` → `--full`；新增 `--annotate`, `--screenshot-quality`, `--screenshot-format`, `--selector`；增加 `--session`, `--tab` | |

#### `browser pdf`
| 项目 | PRD | 当前代码 |
|------|-----|---------|
| 签名 | `pdf <path> --session <SID> --tab <TID>` | `pdf <path>` |
| 变更 | 增加 `--session`, `--tab` | |

#### `browser viewport`
| 项目 | PRD | 当前代码 |
|------|-----|---------|
| 签名 | `viewport --session <SID> --tab <TID>` | `viewport` |
| 变更 | 增加 `--session`, `--tab` | |

#### `browser html`
| 项目 | PRD | 当前代码 |
|------|-----|---------|
| 签名 | `html <selector> --session <SID> --tab <TID>` | `html [selector]` |
| 变更 | selector 从可选变为**必选**；增加 `--session`, `--tab` | |

#### `browser text`
| 项目 | PRD | 当前代码 |
|------|-----|---------|
| 签名 | `text <selector> --session <SID> --tab <TID> [--mode <raw\|readability>]` | `text [selector] [--mode readability]` |
| 变更 | selector 从可选变为**必选**；增加 `--session`, `--tab` | |

#### `browser eval`
| 项目 | PRD | 当前代码 |
|------|-----|---------|
| 签名 | `eval <code> --session <SID> --tab <TID>` | `eval <code>` |
| 变更 | 增加 `--session`, `--tab` | |

### 2.3 Interaction 命令

#### `browser click`
| 项目 | PRD | 当前代码 |
|------|-----|---------|
| PRD 签名 | `click <selector\|coordinates> --session <SID> --tab <TID> [--new-tab] [--button <left\|right\|middle>] [--count <n>]` | `click [selector] [--wait 0] [--ref] [--human]` |
| 新增 flags | `--new-tab`, `--button`, `--count` | |
| 删除 flags | `--wait`, `--ref`, `--human` | |
| 参数变更 | selector 和 coordinates 统一为一个位置参数 | |

#### `browser type`
| 项目 | PRD | 当前代码 |
|------|-----|---------|
| PRD 签名 | `type <selector> <text> --session <SID> --tab <TID>` | `type <text> [selector] [--wait] [--ref] [--human]` |
| **参数顺序变更** | PRD: `<selector> <text>`，代码: `<text> [selector]` | |
| 删除 flags | `--wait`, `--ref`, `--human` | |
| selector | PRD 中**必选** | 代码中可选（可用 --ref 替代） |

#### `browser fill`
| 项目 | PRD | 当前代码 |
|------|-----|---------|
| PRD 签名 | `fill <selector> <text> --session <SID> --tab <TID>` | `fill <text> [selector] [--wait] [--ref]` |
| **参数顺序变更** | PRD: `<selector> <text>`，代码: `<text> [selector]` | |
| 删除 flags | `--wait`, `--ref` | |

#### `browser select`
| 项目 | PRD | 当前代码 |
|------|-----|---------|
| PRD 签名 | `select <selector> <value> --session <SID> --tab <TID> [--by-text]` | `select <selector> <value>` |
| 新增 flags | `--by-text` | |
| 变更 | 增加 `--session`, `--tab`, `--by-text` | |

#### `browser hover`
| 项目 | PRD | 当前代码 |
|------|-----|---------|
| 签名 | `hover <selector> --session <SID> --tab <TID>` | `hover <selector>` |
| 变更 | 增加 `--session`, `--tab` | |

#### `browser focus`
| 项目 | PRD | 当前代码 |
|------|-----|---------|
| 签名 | `focus <selector> --session <SID> --tab <TID>` | `focus <selector>` |
| 变更 | 增加 `--session`, `--tab` | |

#### `browser press`
| 项目 | PRD | 当前代码 |
|------|-----|---------|
| PRD 签名 | `press <key-or-chord> --session <SID> --tab <TID>` | `press <key>` + 独立 `hotkey <keys>` |
| 语义变更 | PRD 合并了 `press` 和 `hotkey` 为一个命令，支持单键和组合键 | 当前分开为两个命令 |
| 变更 | 合并 `hotkey` 到 `press`；增加 `--session`, `--tab` | |

#### `browser upload`
| 项目 | PRD | 当前代码 |
|------|-----|---------|
| PRD 签名 | `upload <selector> <file...> --session <SID> --tab <TID>` | `upload <files...> [--selector] [--ref] [--wait]` |
| 参数变更 | PRD: selector 是位置参数且**必选** | 代码: selector 是可选 flag |
| 删除 flags | `--ref`, `--wait` | |

#### `browser scroll`
| 项目 | PRD | 当前代码 |
|------|-----|---------|
| PRD 签名 | `scroll up\|down\|left\|right <pixels>`, `scroll top\|bottom`, `scroll into-view <selector>` | 子命令: `down [px]`, `up [px]`, `bottom`, `top`, `to <selector>` |
| 新增方向 | `left`, `right` | 当前仅 up/down/top/bottom |
| 新增 flags | `--container <selector>` | 当前无 |
| 子命令重命名 | `to` → `into-view`；增加 `--align` | |
| 删除 flags | `--smooth`, `--wait` | |

#### `browser cookies`
| 项目 | PRD | 当前代码 |
|------|-----|---------|
| 作用域 | `--session <SID>`（session 级） | 无显式 session 绑定 |
| 变更 | 所有 cookies 子命令增加 `--session`；移除 `--dry-run`, `-y/--yes` | |

> **待确认**: PRD 中 `cookies set` 写了 `[every cookies params]`，具体应包含哪些标准 Cookie 属性（domain/path/expires/maxAge/secure/httpOnly/sameSite 等），需与 PRD 作者确认完整参数列表。

### 2.4 Session/Status 命令语义变更

#### `browser status`
| 项目 | PRD | 当前代码 |
|------|-----|---------|
| PRD 签名 | `status --session <SID>` | `status` （无参数） |
| 语义变更 | PRD 中查看指定 session 的状态 | 当前查看 browser 整体状态 |

#### `browser close`
| 项目 | PRD | 当前代码 |
|------|-----|---------|
| PRD 签名 | `close --session <SID>` | `close` （关闭整个浏览器） |
| 语义变更 | PRD 中关闭指定 session | 当前关闭整个浏览器 |

#### `browser restart`
| 项目 | PRD | 当前代码 |
|------|-----|---------|
| PRD 签名 | `restart --session <SID>` | `restart` |
| 语义变更 | PRD 中重启指定 session | 当前重启整个浏览器 |

---

## 三、未实现的命令

### 3.1 Lifecycle 命令（全新）

#### `browser start` — 新起一个 session
```
actionbook browser start
    --mode <local|extension|cloud>
    --headless
    --profile
    --open-url          # 打开浏览器时直接访问这个 url
    --cdp-endpoint      # 指定时不再启动浏览器，连接到这个 cdp endpoint
    --header <KEY:VALUE> # 仅当 cdp_endpoint 时生效
```
**说明**: 这是 v1.0.0 的核心新命令，替代了当前隐式启动浏览器的行为。返回 session ID（如 s0）。

#### `browser list-sessions` — 列出所有 session
```
actionbook browser list-sessions
```

### 3.2 Tab 管理命令（全新）

#### `browser list-tabs`
```
actionbook browser list-tabs --session <SID>
```

#### `browser new-tab`
```
actionbook browser new-tab <url> --session SID [--new-window] [--window WID]
```
**说明**: `open` 是 `new-tab` 的 alias。

#### `browser close-tab`
```
actionbook browser close-tab --session <SID> --tab <TID>
```

### 3.3 Observation 命令（全新）

#### `browser title`
```
actionbook browser title --session <SID> --tab <TID>
```

#### `browser url`
```
actionbook browser url --session <SID> --tab <TID>
```

#### `browser query` — CSS/jQuery 式元素查询
```
actionbook browser query <one|all|nth <n>|count> <query_str>
    --session <SID>
    --tab <TID>
```
支持的 query_str:
- 标准 CSS selector（`.item`, `#some_id`, `input[name=b]`）
- 扩展语法（参考 jQuery，v1.0.0 初始支持以下 6 种，后续可扩展）: `:visible`, `:contains(...)`, `:has(...)`, `:enabled`, `:disabled`, `:checked`

#### `browser value`
```
actionbook browser value <selector> --session <SID> --tab <TID>
```

#### `browser attr`
```
actionbook browser attr <selector> <name> --session <SID> --tab <TID>
```

#### `browser attrs`
```
actionbook browser attrs <selector> --session <SID> --tab <TID>
```

#### `browser box`
```
actionbook browser box <selector> --session <SID> --tab <TID>
```

#### `browser styles`
```
actionbook browser styles <selector> [names...] --session <SID> --tab <TID>
```

#### `browser describe`
```
actionbook browser describe <selector> --session <SID> --tab <TID>
```
返回元素的摘要信息。

#### `browser state`
```
actionbook browser state <selector> --session <SID> --tab <TID>
```
返回元素的 visible/enabled/checked/focused/editable/selected 状态。

#### `browser inspect-point`
```
actionbook browser inspect-point <coordinates> --session <SID> --tab <TID>
    [--parent-depth <n>]
```
**说明**: 类似当前 `inspect` 但重命名，坐标格式为 `x,y`。移除当前的 `--desc` flag，新增 `--parent-depth`。

### 3.4 Logging 命令（全新结构）

#### `browser logs console`
```
actionbook browser logs console
    --session <SID> --tab <TID>
    [--level <level[,level...]>]
    [--tail <n>]
    [--since <id>]
    [--clear]
```
**说明**: 替代当前的 `console` 命令。新增 `--tail`, `--since`, `--clear` flags；`--level` 支持多值；移除 `--duration`。

#### `browser logs errors`
```
actionbook browser logs errors
    --session <SID> --tab <TID>
    [--source <file>]
    [--tail <n>]
    [--since <id>]
    [--clear]
```
**说明**: 全新命令，专门过滤错误日志。

### 3.5 Interaction 命令（全新）

#### `browser drag`
```
actionbook browser drag <selector> <selector|coordinates>
    --session <SID> --tab <TID>
    [--button <left|right|middle>]
```

#### `browser mouse-move`
```
actionbook browser mouse-move <coordinates>
    --session <SID> --tab <TID>
```

#### `browser cursor-position`
```
actionbook browser cursor-position
    --session <SID> --tab <TID>
```

### 3.6 Waiting 命令（重构，新的子命令结构）

PRD 将等待命令统一到 `wait` 下作为子命令：

#### `browser wait element`
```
actionbook browser wait element <selector> --session <SID> --tab <TID> --timeout <ms>
```
替代当前: `browser wait <selector> --timeout`

#### `browser wait navigation`
```
actionbook browser wait navigation --session <SID> --tab <TID> --timeout <ms>
```
替代当前: `browser wait-nav --timeout`

#### `browser wait network-idle`
```
actionbook browser wait network-idle <selector> --session <SID> --tab <TID> --timeout <ms>
```
> **待确认**: PRD 原文包含 `<selector>` 位置参数，但 network-idle 通常是全局状态，`<selector>` 的语义不明确，可能是 PRD 笔误，需与 PRD 作者确认。

替代当前: `browser wait-idle --timeout --idle-time`（移除 `--idle-time`）

#### `browser wait condition`
```
actionbook browser wait condition <expression> --session <SID> --tab <TID> --timeout <ms>
```
替代当前: `browser wait-fn <expression> --timeout --interval`（移除 `--interval`）

### 3.7 Storage 命令（重构，独立顶级子命令）

PRD 将 storage 拆为两个独立子命令，替代当前的 `storage` 统一命令：

#### `browser session-storage`
```
actionbook browser session-storage list --session <SID> --tab <TID>
actionbook browser session-storage get <key> --session <SID> --tab <TID>
actionbook browser session-storage set <key> <value> --session <SID> --tab <TID>
actionbook browser session-storage delete <key> --session <SID> --tab <TID>
actionbook browser session-storage clear --session <SID> --tab <TID>
```

> **待确认**: PRD 原文中 `clear` 后面跟了 `<key>` 参数，但 `clear` 通常表示清空所有，不需要指定 key。可能是 PRD 笔误，需与 PRD 作者确认。

#### `browser local-storage`
```
actionbook browser local-storage list --session <SID> --tab <TID>
actionbook browser local-storage get <key> --session <SID> --tab <TID>
actionbook browser local-storage set <key> <value> --session <SID> --tab <TID>
actionbook browser local-storage delete <key> --session <SID> --tab <TID>
actionbook browser local-storage clear --session <SID> --tab <TID>
```

---

## 四、需要删除的命令

### 4.1 需删除的顶级命令

| 命令 | 当前功能 | 删除原因 |
|------|---------|---------|
| `sources` (list/search) | 列出/搜索数据源 | PRD 未定义 |
| `act` | 显示 area 的可执行元素 | PRD 未定义 |
| `config` (show/set/get/edit/path/reset) | 配置管理 | PRD 未定义（通过 setup 和 env 管理） |
| `profile` (list/create/delete/show) | Profile 管理 | PRD 未定义（profile 通过 config 和 `browser start --profile` 管理） |
| `extension` (serve/status/ping/install/stop/path/uninstall) | Extension bridge 管理 | PRD 未定义（内部实现细节） |
| `daemon` (serve/status/stop) | Daemon 管理 | PRD 未定义（内部实现细节） |
| `app` (launch/attach/list/status/close/restart/...) | Electron 应用控制 | PRD 明确标注 "TODO: 后面再补充"，v1.0.0 不含 |

### 4.2 需删除的 Browser 子命令

| 命令 | 当前功能 | 删除/替代说明 |
|------|---------|-------------|
| `browser batch` | 批量执行 JSON actions | PRD 未定义 |
| `browser fingerprint` (rotate) | 轮换浏览器指纹 | PRD 未定义 |
| `browser emulate` | 设备模拟 | PRD 未定义 |
| `browser fetch` | 一次性抓取页面 | PRD 未定义 |
| `browser connect` | 连接已有浏览器 | 合并到 `browser start --cdp-endpoint` |
| `browser tab` (list/new/switch/close/active) | Tab 管理子命令组 | 替代为 `list-tabs`, `new-tab`, `close-tab` 顶级子命令 |
| `browser session` (list/active/destroy) | Session 管理子命令组 | 替代为 `list-sessions`；`destroy` → `close --session` |
| `browser switch` | 切换到指定 page | PRD 使用 `--tab` 显式寻址 |
| `browser pages` | 列出所有 pages | 替代为 `list-tabs` |
| `browser wait` (旧) | 等待元素 | 替代为 `wait element` |
| `browser wait-nav` | 等待导航 | 替代为 `wait navigation` |
| `browser wait-idle` | 等待网络空闲 | 替代为 `wait network-idle` |
| `browser wait-fn` | 等待 JS 条件 | 替代为 `wait condition` |
| `browser hotkey` | 发送组合键 | 合并到 `press` |
| `browser info` | 元素详情 | 拆分为 `box`, `attrs`, `styles`, `describe`, `state` |
| `browser switch-frame` | 切换 iframe | PRD: iframe 默认展开，不需显式切换 |
| `browser inspect` | 坐标检查元素 | 重命名为 `inspect-point`，参数变更 |
| `browser console` | 控制台日志 | 重构为 `logs console` |
| `browser storage` (get/set/remove/clear/list) | 统一存储管理 | 拆分为 `session-storage` 和 `local-storage` |

---

## 五、设计原则提醒

参考 `core_design.md`：

1. **无状态接口，有状态运行时**: CLI 接口完全无状态，每条命令通过 `--session` + `--tab` 显式寻址
2. **绝对路径式寻址**: 无隐式"当前 tab"，缺少 session/tab 参数直接报错
3. **为 LLM 消费者设计**: 输出默认紧凑文本，短 ID（s0, t3），每条响应带 `[session tab] url` 前缀
4. **错误即引导**: 每个错误包含 hint 字段
5. **干净断裂**: 不做向后兼容
6. **严格按 PRD**: 不增加额外接口
