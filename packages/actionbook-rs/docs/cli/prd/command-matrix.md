# CLI v1.0.0 命令对照矩阵

> 状态标记: ✅ 已实现 | ⚠️ 需调整 | ❌ 未实现 | 🗑️ 需删除

---

## 一、Non-browser 命令

| PRD 命令 | 当前代码 | 状态 | 差异说明 |
|----------|---------|------|---------|
| `actionbook search <query> [-d domain] [-u url] [-p page] [-s page_size]` | `search <query> [-d] [-u] [-p 1] [-s 10]` | ✅ | 一致 |
| `actionbook get <area_id>` | `get <area_id>` | ✅ | 一致 |
| `actionbook setup [--target] [--api-key] [--browser] [--non-interactive] [--reset]` | `setup [--target] [--api-key] [--browser] [--non-interactive] [--reset]` | ✅ | 一致 |
| `actionbook help` | clap 内置 | ✅ | 一致 |
| `actionbook --version` | clap 内置 | ✅ | 一致 |
| — | `actionbook act <area_id>` | 🗑️ | PRD 未定义 |
| — | `actionbook sources list/search` | 🗑️ | PRD 未定义 |
| — | `actionbook config show/set/get/edit/path/reset` | 🗑️ | PRD 未定义 |
| — | `actionbook profile list/create/delete/show` | 🗑️ | PRD 未定义 |
| — | `actionbook extension serve/status/ping/install/stop/path/uninstall` | 🗑️ | PRD 未定义 |
| — | `actionbook daemon serve/status/stop` | 🗑️ | PRD 未定义 |
| — | `actionbook app launch/attach/list/status/...` | 🗑️ | PRD 标注 TODO，v1.0.0 不含 |

---

## 二、Browser Lifecycle 命令

| PRD 命令 | 当前代码 | 状态 | 差异说明 |
|----------|---------|------|---------|
| `browser start [--mode] [--headless] [--profile] [--open-url] [--cdp-endpoint] [--header]` | 无 | ❌ | 全新命令 |
| `browser list-sessions` | 无 (`session list` 部分覆盖) | ❌ | 全新顶级子命令 |
| `browser close --session <SID>` | `browser close`（关闭整个浏览器） | ⚠️ | 语义变更: 关闭 session 而非浏览器 |
| `browser restart --session <SID>` | `browser restart` | ⚠️ | 语义变更: 重启 session |
| `browser status --session <SID>` | `browser status` | ⚠️ | 增加 `--session` 参数 |
| — | `browser connect <endpoint> [-H header]` | 🗑️ | 合并到 `browser start --cdp-endpoint` |
| — | `browser session list/active/destroy` | 🗑️ | 替代为 `list-sessions`, `close --session` |

---

## 三、Browser Tab 管理命令

| PRD 命令 | 当前代码 | 状态 | 差异说明 |
|----------|---------|------|---------|
| `browser list-tabs --session <SID>` | 无 (`tab list`, `pages` 部分覆盖) | ❌ | 全新顶级子命令 |
| `browser new-tab <url> --session SID [--new-window] [--window WID]` | 无 (`tab new [url]`) | ❌ | 全新顶级子命令 |
| `browser open <url> --session SID [--new-window] [--window WID]` | `browser open <url> [--new-window]` | ⚠️ | 增加 `--session`, `--window`；成为 new-tab alias |
| `browser close-tab --session <SID> --tab <TID>` | 无 (`tab close [page_id]`) | ❌ | 全新顶级子命令 |
| — | `browser tab list/new/switch/close/active` | 🗑️ | 替代为顶级子命令 |
| — | `browser pages` | 🗑️ | 替代为 `list-tabs` |
| — | `browser switch <page_id>` | 🗑️ | 使用 `--tab` 显式寻址 |

---

## 四、Browser Navigation 命令

| PRD 命令 | 当前代码 | 状态 | 差异说明 |
|----------|---------|------|---------|
| `browser goto <url> --session <SID> --tab <TID>` | `browser goto <url> [--timeout 30000]` | ⚠️ | +`--session`, `--tab`; 移除命令级 `--timeout` |
| `browser back --session <SID> --tab <TID>` | `browser back` | ⚠️ | +`--session`, `--tab` |
| `browser forward --session <SID> --tab <TID>` | `browser forward` | ⚠️ | +`--session`, `--tab` |
| `browser reload --session <SID> --tab <TID>` | `browser reload` | ⚠️ | +`--session`, `--tab` |

---

## 五、Browser Observation 命令

| PRD 命令 | 当前代码 | 状态 | 差异说明 |
|----------|---------|------|---------|
| `browser snapshot --session <SID> --tab <TID> [-i] [-C] [-c] [-d n] [-s sel]` | `browser snapshot [-i] [-C] [-c] [-d] [-s] [--format] [--diff] [--max-tokens]` | ⚠️ | +`--session`, `--tab`; 移除 `--format`, `--diff`, `--max-tokens` |
| `browser screenshot <path> --session <SID> --tab <TID> [--full] [--annotate] [--screenshot-quality] [--screenshot-format] [--selector]` | `browser screenshot [path] [--full-page]` | ⚠️ | `--full-page`→`--full`; +4 个新 flag; +`--session`, `--tab` |
| `browser pdf <path> --session <SID> --tab <TID>` | `browser pdf <path>` | ⚠️ | +`--session`, `--tab` |
| `browser title --session <SID> --tab <TID>` | 无 | ❌ | 全新命令 |
| `browser url --session <SID> --tab <TID>` | 无 | ❌ | 全新命令 |
| `browser viewport --session <SID> --tab <TID>` | `browser viewport` | ⚠️ | +`--session`, `--tab` |
| `browser query <one\|all\|nth <n>\|count> <query_str> --session <SID> --tab <TID>` | 无 | ❌ | 全新命令; 支持 CSS + jQuery 扩展语法（`:visible`, `:contains()`, `:has()`, `:enabled`, `:disabled`, `:checked`，后续可扩展） |
| `browser html <selector> --session <SID> --tab <TID>` | `browser html [selector]` | ⚠️ | selector 必选; +`--session`, `--tab` |
| `browser text <selector> --session <SID> --tab <TID> [--mode]` | `browser text [selector] [--mode]` | ⚠️ | selector 必选; +`--session`, `--tab` |
| `browser value <selector> --session <SID> --tab <TID>` | 无 | ❌ | 全新命令 |
| `browser attr <selector> <name> --session <SID> --tab <TID>` | 无 | ❌ | 全新命令 |
| `browser attrs <selector> --session <SID> --tab <TID>` | 无 | ❌ | 全新命令 |
| `browser box <selector> --session <SID> --tab <TID>` | 无 | ❌ | 全新命令 |
| `browser styles <selector> [names...] --session <SID> --tab <TID>` | 无 | ❌ | 全新命令 |
| `browser describe <selector> --session <SID> --tab <TID>` | 无 | ❌ | 全新命令 |
| `browser state <selector> --session <SID> --tab <TID>` | 无 | ❌ | 全新命令 |
| `browser inspect-point <coordinates> --session <SID> --tab <TID> [--parent-depth]` | 无 (`inspect <x> <y> [--desc]`) | ❌ | 重命名+参数重构 |
| — | `browser inspect <x> <y> [--desc]` | 🗑️ | 替代为 `inspect-point` |
| — | `browser info <selector>` | 🗑️ | 拆分为 box/attrs/styles/describe/state |

---

## 六、Browser Logging 命令

| PRD 命令 | 当前代码 | 状态 | 差异说明 |
|----------|---------|------|---------|
| `browser logs console --session <SID> --tab <TID> [--level <level[,level...]>] [--tail <n>] [--since <id>] [--clear]` | 无 (`console [--duration] [--level]`) | ❌ | 全新子命令结构; 移除 `--duration`, 新增 `--tail`, `--since`, `--clear`; `--level` 支持多值逗号分隔 |
| `browser logs errors --session <SID> --tab <TID> [--source <file>] [--tail <n>] [--since <id>] [--clear]` | 无 | ❌ | 全新命令; `--source` 按文件过滤错误来源 |
| — | `browser console [--duration] [--level]` | 🗑️ | 替代为 `logs console` |

---

## 七、Browser Interaction 命令

| PRD 命令 | 当前代码 | 状态 | 差异说明 |
|----------|---------|------|---------|
| `browser click <selector\|coordinates> --session <SID> --tab <TID> [--new-tab] [--button] [--count]` | `browser click [selector] [--wait] [--ref] [--human]` | ⚠️ | +3 新 flag; -3 旧 flag; selector 必选 |
| `browser type <selector> <text> --session <SID> --tab <TID>` | `browser type <text> [selector] [--wait] [--ref] [--human]` | ⚠️ | **参数顺序反转**; -3 旧 flag |
| `browser fill <selector> <text> --session <SID> --tab <TID>` | `browser fill <text> [selector] [--wait] [--ref]` | ⚠️ | **参数顺序反转**; -2 旧 flag |
| `browser select <selector> <value> --session <SID> --tab <TID> [--by-text]` | `browser select <selector> <value>` | ⚠️ | +`--by-text`, `--session`, `--tab` |
| `browser hover <selector> --session <SID> --tab <TID>` | `browser hover <selector>` | ⚠️ | +`--session`, `--tab` |
| `browser focus <selector> --session <SID> --tab <TID>` | `browser focus <selector>` | ⚠️ | +`--session`, `--tab` |
| `browser press <key-or-chord> --session <SID> --tab <TID>` | `browser press <key>` + `browser hotkey <keys>` | ⚠️ | 合并 press+hotkey; +`--session`, `--tab` |
| `browser drag <selector> <selector\|coordinates> --session <SID> --tab <TID> [--button]` | 无 | ❌ | 全新命令 |
| `browser upload <selector> <file...> --session <SID> --tab <TID>` | `browser upload <files...> [--selector] [--ref] [--wait]` | ⚠️ | selector 变为位置参数且必选; -2 旧 flag |
| `browser eval <code> --session <SID> --tab <TID>` | `browser eval <code>` | ⚠️ | +`--session`, `--tab` |
| `browser mouse-move <coordinates> --session <SID> --tab <TID>` | 无 | ❌ | 全新命令 |
| `browser cursor-position --session <SID> --tab <TID>` | 无 | ❌ | 全新命令 |
| — | `browser hotkey <keys>` | 🗑️ | 合并到 `press` |

---

## 八、Browser Scroll 命令

| PRD 命令 | 当前代码 | 状态 | 差异说明 |
|----------|---------|------|---------|
| `browser scroll up\|down\|left\|right <pixels> --session <SID> --tab <TID> [--container]` | `browser scroll down\|up [pixels] [--smooth] [--wait]` | ⚠️ | +left/right; +`--container`; -`--smooth`, `-wait`; +`--session`, `--tab` |
| `browser scroll top\|bottom --session <SID> --tab <TID> [--container]` | `browser scroll top\|bottom` | ⚠️ | +`--container`; +`--session`, `--tab` |
| `browser scroll into-view <selector> --session <SID> --tab <TID> [--align]` | `browser scroll to <selector> [--align]` | ⚠️ | `to` → `into-view`; +`--session`, `--tab` |

---

## 九、Browser Waiting 命令

| PRD 命令 | 当前代码 | 状态 | 差异说明 |
|----------|---------|------|---------|
| `browser wait element <selector> --session <SID> --tab <TID> --timeout <ms>` | 无 (`wait <selector> --timeout`) | ❌ | 重构为 wait 子命令 |
| `browser wait navigation --session <SID> --tab <TID> --timeout <ms>` | 无 (`wait-nav --timeout`) | ❌ | 重构为 wait 子命令 |
| `browser wait network-idle <selector> --session <SID> --tab <TID> --timeout <ms>` | 无 (`wait-idle --timeout --idle-time`) | ❌ | 重构; 移除 `--idle-time`; **注意**: PRD 含 `<selector>` 参数，语义待确认 |
| `browser wait condition <expression> --session <SID> --tab <TID> --timeout <ms>` | 无 (`wait-fn <expr> --timeout --interval`) | ❌ | 重构; 移除 `--interval` |
| — | `browser wait <selector> --timeout` | 🗑️ | 替代为 `wait element` |
| — | `browser wait-nav --timeout` | 🗑️ | 替代为 `wait navigation` |
| — | `browser wait-idle --timeout --idle-time` | 🗑️ | 替代为 `wait network-idle` |
| — | `browser wait-fn <expr> --timeout --interval` | 🗑️ | 替代为 `wait condition` |

---

## 十、Browser Cookies 命令

| PRD 命令 | 当前代码 | 状态 | 差异说明 |
|----------|---------|------|---------|
| `browser cookies list --session <SID> [--domain]` | `cookies list` | ⚠️ | +`--session`（session 级，非 tab 级） |
| `browser cookies get <name> --session <SID>` | `cookies get <name>` | ⚠️ | +`--session` |
| `browser cookies set <name> <value> --session <SID> [cookie params]` | `cookies set <name> <value> [--domain]` | ⚠️ | +`--session`; PRD 支持完整 cookie 参数 |
| `browser cookies delete <name> --session <SID>` | `cookies delete <name>` | ⚠️ | +`--session` |
| `browser cookies clear --session <SID> [--domain]` | `cookies clear [--domain] [--dry-run] [-y]` | ⚠️ | +`--session`; 移除 `--dry-run`, `-y` |

---

## 十一、Browser Storage 命令

| PRD 命令 | 当前代码 | 状态 | 差异说明 |
|----------|---------|------|---------|
| `browser session-storage list\|get\|set\|delete\|clear --session <SID> --tab <TID>` | 无 (`storage get\|set\|remove\|clear\|list [--session]`) | ❌ | 全新顶级子命令 |
| `browser local-storage list\|get\|set\|delete\|clear --session <SID> --tab <TID>` | 无 (同上) | ❌ | 全新顶级子命令 |
| — | `browser storage get\|set\|remove\|clear\|list [--session]` | 🗑️ | 拆分为 session-storage + local-storage |

---

## 十二、需删除的 Browser 命令（无 PRD 对应）

| 当前代码 | 状态 | 说明 |
|---------|------|------|
| `browser batch [--file] [--delay]` | 🗑️ | PRD 未定义 |
| `browser fingerprint rotate [--os] [--screen]` | 🗑️ | PRD 未定义 |
| `browser emulate <device>` | 🗑️ | PRD 未定义 |
| `browser fetch <url> [--format] [--max-tokens] [--timeout] [--lite]` | 🗑️ | PRD 未定义 |
| `browser switch-frame <target>` | 🗑️ | PRD: iframe 默认展开 |

---

## 十三、Global Flags 对照

| PRD | 当前代码 | 状态 |
|-----|---------|------|
| `--timeout <ms>` | 无全局，各命令自带 | ❌ 需新增为全局 |
| `--json` | `--json` | ✅ |
| — | `--browser-path` | 🗑️ |
| — | `--cdp` | 🗑️ |
| — | `-P, --profile` | 🗑️ |
| — | `-S, --session` | 🗑️ 移到命令级 |
| — | `--headless` | 🗑️ 移到 config/start |
| — | `--stealth` | 🗑️ |
| — | `--stealth-os` | 🗑️ |
| — | `--stealth-gpu` | 🗑️ |
| — | `--api-key` | 🗑️ 移到 config/env |
| — | `--browser-mode` | 🗑️ 移到 config/start |
| — | `--extension` (deprecated) | 🗑️ |
| — | `--extension-port` (deprecated) | 🗑️ |
| — | `-v, --verbose` | 🗑️ |
| — | `--block-images` | 🗑️ |
| — | `--block-media` | 🗑️ |
| — | `--no-animations` | 🗑️ |
| — | `--auto-dismiss-dialogs` | 🗑️ |
| — | `--session-tag` | 🗑️ |
| — | `--rewrite-urls` | 🗑️ |
| — | `--wait-hint` | 🗑️ |
| — | `--camofox` | 🗑️ |
| — | `--camofox-port` | 🗑️ |
| — | `--no-daemon` | 🗑️ |
| — | `--auto-connect` | 🗑️ |

---

## 统计汇总

| 类别 | 数量 |
|------|------|
| ✅ 已实现且一致 | 5 (search, get, setup, help, --version) |
| ⚠️ 已实现需调整 | 27 |
| ❌ 未实现 | 26 |
| 🗑️ 需删除的顶级命令 | 7 (sources, act, config, profile, extension, daemon, app) |
| 🗑️ 需删除的 browser 子命令 | 18 |
| 🗑️ 需删除的 global flags | 22 |
