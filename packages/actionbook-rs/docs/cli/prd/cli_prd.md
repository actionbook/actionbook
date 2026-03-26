
- Design
- CEO Plan

Config & Env
[api]
# ACTIONBOOK_BASE_URL
base_url
# ACTIONBOOK_API_KEY
api_key

[browser]
# ACTIONBOOK_BROWSER_MODE
mode = "local" | "extension" | "cloud"  # 默认是 local
# ACTIONBOOK_BROWSER_HEADLESS
headless = true | false                 # 默认是 false

# for "local"
# ACTIONBOOK_BROWSER_EXECUTABLE_PATH
executable_path     # 本地 chrome 的路径，如果这项没有，则在执行指令时去发现
# ACTIONBOOK_BROWSER_PROFILE_NAME
profile_name        # profile 名称，默认为 actionbook

# for "extension"
// 这块没了解细节，需要再设计下

# for "cloud"
// 待确认：这个似乎暂时不需要配置，就是传 wss cdp endpoint 对吧？

Non-browser commands
actionbook search      # 和现在一样
actionbook get         # 和现在一样
actionbook setup       # 和现在一样

# TODO: 通用命令
actionbook help
actionbook --version
Browser commands
定义
- <selector> 可以代表  ref、css selector 或 xpath
- <coordinates> 格式为 x,y
Global Flags
- --timeout <ms>，超时时间
- --json，JSON 输出（默认是纯文本输出）
Lifecycle (Session management)
actionbook browser start # 新起一个 session

Optional flags
    --mode <local|extension|cloud>
    --headless            # 这个 session 是否是 headless mode
    --profile             # 这个 session 使用的 profile
    --open-url            # 打开浏览器时直接访问这个 url
    --cdp-endpoint        # 指定时，不再启动一个浏览器，而是连接到这个 cdp endpoint
    --header <KEY:VALUE>  # 仅当 cdp_endpoint 时生效，连接时传递 header
actionbook browser list-sessions
actionbook browser close --session <SID>
actionbook browser restart --session <SID>
actionbook browser status --session <SID>
Tab management (require --session)
actionbook browser list-tabs --session <SID>

# new-tab 和 open 是 alias，--new-window 则这个 tab 会在新窗口中打开
actionbook browser new-tab <url> --session SID [--new-window] [--window WID]
actionbook browser open <url> --session SID [--new-window] [--window WID]

actionbook browser close-tab --session <SID> --tab <TID>
iFrame
考虑 iframe 的问题，最好的是默认展开内容，这样 AI 处理时就是直接当成这个 tab 的一个部分来处理，而不需要在每个操作里面传 frame id。
这里需要技术调研一下，如果不行的话，就得再设计一下针对 iFrame 的管理和操作方式。
Navigation (require --session and --tab)
actionbook browser goto <url> --session <SID> --tab <TID>
actionbook browser back --session <SID> --tab <TID>
actionbook browser forward --session <SID> --tab <TID>
actionbook browser reload --session <SID> --tab <TID>

# open 是 new-tab 的 alias
actionbook browser open <url> --session SID [--new-window] [--window WID]
Observation (require --session and --tab)
actionbook browser snapshot --session <SID> --tab <TID>

Optional flags:
    --interactive # 仅包含可交互元素
    --cursor      # 额外包含「鼠标/焦点可交互」的自定义元素，比如带 cursor:pointer、onclick、tabindex 的 div/span 等
    --compact     # 压缩输出，去掉空的结构节点
    --depth <n>   # 限制树的最大深度，超过这个缩进层级的节点不再渲染
    --selector <sel>  # 用 selector 把 snapshot 限定到某个子树
actionbook browser screenshot <path> --session <SID> --tab <TID>

Optional flags:
    --full        # 截整页，不只是当前 viewport
    --annotate    # 给截图叠加编号标签，标出可交互元素，编号 [N] 对应 ref @eN
    --screenshot-quality <0-100> # JPEG 质量，只对 jpeg 生效
    --screenshot-format <png|jpeg> # 图片格式
    --selector <sel> # 用 selector 把 screenshot 限定到某个子区域
# 将当前页面保存为 PDF，类似于打印导出
actionbook browser pdf <path> --session <SID> --tab <TID>
actionbook browser title --session <SID> --tab <TID>
actionbook browser url --session <SID> --tab <TID>
actionbook browser viewport --session <SID> --tab <TID>
actionbool browser query <one|all|nth <n>|count> <query_str>
    --session <SID>
    --tab <TID>

# 这里的 query_str 目前支持：
- 标准 css_selector，例如 .item, #some_id, .c > .a > input[name=b] 等
- 扩展语法，这里参考 jQuery，包括下面这些，很多的后续逐步补充
    - :visible
    - :contains(...)
    - :has(...)
    - :enabled
    - :disabled
    - :checked
actionbook browser html <selector> --session <SID> --tab <TID>
actionbook browser text <selector>
    --session <SID> --tab <TID>
    [--mode <raw|readability>]
actionbook browser value <selector> --session <SID> --tab <TID>
actionbook browser attr <selector> <name> --session <SID> --tab <TID>
actionbook browser attrs <selector> --session <SID> --tab <TID>
actionbook browser box <selector> --session <SID> --tab <TID>
actionbook browser styles <selector> [names...] --session <SID> --tab <TID>

# describe 是描述这个元素的摘要信息
actionbook browser describe <selector> --session <SID> --tab <TID>
    [--nearby]

# state 会返回这个元素的 visible/enabled/checked/focused/editable/selected 这几个状态
actionbook browser state <selector> --session <SID> --tab <TID>

# inspect-point 建议是配合 screenshot 使用，这个可以加到 skills 中
actionbook browser inspect-point <coordinates> --session <SID> --tab <TID>
    [--parent-depth <n>]

actionbook browser logs console
    --session <SID> --tab <TID>
    [--level <level[,level...]>]
    [--tail <n>]
    [--since <id>]
    [--clear]
actionbook browser logs errors
    --session <SID> --tab <TID>
    [--source <file>]
    [--tail <n>]
    [--since <id>]
    [--clear]
Interaction (require --session and --tab)
actionbook browser click <selector|coordinates>
    --session <SID> --tab <TID>
    [--new-tab]    # 如果目标元素有 href，就在新 tab 里打开链接，而不是当前页跳转
    [--button <left|right|middle>]
    [--count <n>]
actionbook browser type <selector> <text>
    --session <SID> --tab <TID>
actionbook browser fill <selector> <text>
    --session <SID> --tab <TID>
actionbook browser select <selector> <value>
    --session <SID> --tab <TID>
    [--by-text]    # 使用显示的选项文本来匹配选择
actionbook browser hover <selector>
    --session <SID> --tab <TID>
actionbook browser focus <selector>
    --session <SID> --tab <TID>
actionbook browser press <key-or-chord>
    --session <SID> --tab <TID>
actionbook browser drag <selector> <selector|coordinates>
    --session <SID> --tab <TID>
    [--button <left|right|middle>]
actionbook browser upload <selector> <file...>
    --session <SID> --tab <TID>
    
actionbook browser eval <code>
    --session <SID> --tab <TID>

# 视觉的处理方式
actionbook browser mouse-move <coordinates>
    --session <SID> --tab <TID>
actionbook browser cursor-position

# 滚动窗口
actionbook browser scroll up|down|left|right <pixels>
    --session <SID> --tab <TID>
    [--container <selector>]
actionbook browser scroll top|bottom
    --session <SID> --tab <TID>
    [--container <selector>]
actionbook browser scroll into-view <selector>
    --session <SID> --tab <TID>
    [--align <start|center|end|nearest>]
Waiting (require --session and --tab)
actionbook browser wait element <selector> --session <SID> --tab <TID> --timeout <ms>
actionbook browser wait navigation --session <SID> --tab <TID> --timeout <ms>
actionbook browser wait network-idle <selector> --session <SID> --tab <TID> --timeout <ms>
actionbook browser wait condition <expression> --session <SID> --tab <TID> --timeout <ms>
Cookies (require --session)
注意：Cookies 是和 session 相关的
actionbook browser cookies list
    --session <SID>
    [--domain <domain>]
actionbook browser cookies get <name> --session <SID>
actionbook browser cookies set <name> <value> --session <SID> [every cookies params]
actionbook browser cookies delete <name> --session <SID>
actionbook browser cookies clear
    --session <SID>
    [--domain <domain>]
    
Storage (require --session and --tab)
actionbook browser session-storage|local-storage list
    --session <SID> --tab <TID>
actionbook browser session-storage|local-storage get <key>
    --session <SID> --tab <TID>
actionbook browser session-storage|local-storage set <key> <value>
    --session <SID> --tab <TID>
actionbook browser session-storage|local-storage delete <key>
    --session <SID> --tab <TID>
actionbook browser session-storage|local-storage clear <key>
    --session <SID> --tab <TID>
App commands
TODO: app commands 目前是只能操作 electron app，所以应该是 browser commands 的一个子集。这个后面再补充。
返回值
return value
Todo
[x] 每个命令的返回值，需要确定
[x] Interaction 更多命令支持
  [x] drag



以下是返回值的设计
摘要

本文定义新版 Actionbook CLI 的返回值协议，覆盖两种输出模式：

- 默认文本输出：给人类和 LLM 直接阅读
- --json 输出：给 agent、SDK、脚本稳定消费
设计目标：

- 所有命令返回结构统一
- 会话类命令始终显式携带 session_id 和 tab_id
- 默认文本输出也具备稳定格式，不再只是“尽量可读”
- 这是全新版本协议，不兼容旧版返回格式
设计原则

1. --json 一律使用统一 envelope。
2. 文本输出一律使用固定头部格式。
3. 会话上下文是一级概念，不隐藏在日志里。
4. 非会话命令不返回 context。
5. session_id 支持语义化命名。
6. tab_id 使用短 ID，适合人和 LLM 在终端里复用。
ID 约定
session_id
- 类型：字符串
- 来源：可由 actionbook browser start --set-session-id <SID> 指定
- 要求：
  - 对人类和 LLM 有语义
  - 在当前 CLI 生命周期和持久化状态里可唯一定位一个 session
示例：
- research-google
- github-login-debug
- airbnb-form-fill
tab_id
- 类型：短字符串
- 格式：t1, t2, t3, ...
- 范围：在单个 session 内唯一
- 说明：
  - tab_id 是对外 ID
  - 底层浏览器原生 tab handle 如果需要，可额外通过 native_tab_id 暴露
  - 不允许用户手动命名 tab
可选内部 ID
如果底层桥接需要，JSON 可附带：
- native_tab_id
- native_window_id
这些字段仅用于桥接和调试，不作为主要引用 ID。
JSON 总体协议
所有 --json 返回统一为：
{
  "ok": true,
  "command": "browser.snapshot",
  "context": {
    "session_id": "research-google",
    "tab_id": "t1",
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
顶层字段
字段
类型
必填
说明
ok
boolean
是
命令是否成功
command
string
是
规范化命令名，如 browser.snapshot
context
object
否
仅会话命令返回
data
object/null
是
业务返回值
error
object/null
是
错误信息
meta
object
是
元信息
context
仅适用于绑定浏览器上下文的命令。
{
  "session_id": "research-google",
  "tab_id": "t1",
  "window_id": "w1",
  "url": "https://google.com",
  "title": "Google"
}

字段规则：
- session_id：会话命令必带
- tab_id：tab 级命令必带
- window_id：仅多窗口场景返回
- url / title：当前上下文已知时返回
`error`
{
  "code": "ELEMENT_NOT_FOUND",
  "message": "Element not found: #submit",
  "retryable": false,
  "details": {
    "selector": "#submit"
  }
}

`meta`

{
  "duration_ms": 123,
  "warnings": [],
  "pagination": {
    "page": 1,
    "page_size": 10,
    "total": 42,
    "has_more": true
  },
  "truncated": false
}

规则：

- 所有耗时信息统一进 meta.duration_ms
- 所有告警统一进 meta.warnings
- 分页信息统一进 meta.pagination
- 截断信息统一进 meta.truncated
- 不允许把这些字段散落到 data 顶层
文本输出总体协议

非会话命令

首行不带上下文前缀，直接输出结果标题或正文。

示例：

3 results
1. github.com:/login:default
2. github.com:/signup:default
3. github.com:/settings/profile:default

会话命令

session 级命令

格式：

[<session_id>]
<body>

示例：

[research-google]
ok browser.start
mode: local
status: running

tab 级命令

格式：

[<session_id> <tab_id>] <url>
<body>

示例：

[research-google t1] https://google.com
<snapshot output here>
文本输出通用规则

- 第一行永远是上下文头
- 第二行开始输出业务内容
- 不混入调试日志
- 不输出“下一步建议”“提示语”“彩色符号”这类非结构化噪音
- 读取类命令直接输出正文
- 动作类命令用 ok <command> 起头
- 失败统一输出 error <CODE>: <message>
非会话命令返回定义

以下命令不返回 context：

- search
- get
- setup
- help
- --version
命令级返回定义

1.`actionbook search`

JSON
{
  "ok": true,
  "command": "search",
  "data": {
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
  },
  "meta": {
    "duration_ms": 85,
    "warnings": [],
    "pagination": {
      "page": 1,
      "page_size": 10,
      "total": 1,
      "has_more": false
    },
    "truncated": false
  }
}

文本

1 result
1. google.com:/login:default
   Google Login
   score: 0.98
   https://google.com/login

2.`actionbook get`

JSON

{
  "ok": true,
  "command": "get",
  "data": {
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
  },
  "meta": {
    "duration_ms": 66,
    "warnings": [],
    "pagination": null,
    "truncated": false
  }
}

文本

google.com:/login:default
https://google.com/login

Login page

[email] input
description: Email input
css: #identifierId
methods: fill, type, focus

3.`actionbook setup`

Setup 进入交互式页面，参考现有版本实现
4.`actionbook help`

JSON

{
  "ok": true,
  "command": "help",
  "data": "help text here",
  "meta": {
    "duration_ms": 3,
    "warnings": [],
    "pagination": null,
    "truncated": false
  }
}

文本

actionbook browser <subcommand>

start      Start or attach a browser session
list-tabs  List tabs in a session
snapshot   Capture accessibility snapshot

5.`actionbook --version`

JSON

{
  "ok": true,
  "command": "version",
  "data": "1.0.0",
  "meta": {
    "duration_ms": 1,
    "warnings": [],
    "pagination": null,
    "truncated": false
  }
}

文本

1.0.0

6.`actionbook browser start`

语义

- 创建或附着一个 session
- 可通过 --set-session-id 指定 session_id
- 可选自动打开初始 tab
JSON

{
  "ok": true,
  "command": "browser.start",
  "context": {
    "session_id": "research-google",
    "tab_id": "t1",
    "url": "https://google.com",
    "title": "Google"
  },
  "data": {
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
  },
  "meta": {
    "duration_ms": 842,
    "warnings": [],
    "pagination": null,
    "truncated": false
  }
}

文本

[research-google t1] https://google.com
ok browser.start
mode: local
status: running
title: Google

7.`actionbook browser list-sessions`

JSON

{
  "ok": true,
  "command": "browser.list-sessions",
  "data": {
    "total_sessions": 1,
    "sessions": [
      {
        "session_id": "research-google",
        "mode": "local",
        "status": "running",
        "headless": false,
        "tabs_count": 2
      }
    ],
  },
  "meta": {
    "duration_ms": 9,
    "warnings": [],
    "pagination": null,
    "truncated": false
  }
}

文本

1 session
[research-google]
status: running
tabs: 2

8.`actionbook browser status`

JSON

{
  "ok": true,
  "command": "browser.status",
  "context": {
    "session_id": "research-google"
  },
  "data": {
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
  },
  "meta": {
    "duration_ms": 14,
    "warnings": [],
    "pagination": null,
    "truncated": false
  }
}

文本

[research-google]
status: running
mode: local
tabs: 2

9.`actionbook browser close`

JSON

{
  "ok": true,
  "command": "browser.close",
  "context": {
    "session_id": "research-google"
  },
  "data": {
    "session_id": "research-google",
    "status": "closed",
    "closed_tabs": 2
  },
  "meta": {
    "duration_ms": 102,
    "warnings": [],
    "pagination": null,
    "truncated": false
  }
}

文本

[research-google]
ok browser.close
closed_tabs: 2

10.`actionbook browser restart`

JSON

{
  "ok": true,
  "command": "browser.restart",
  "context": {
    "session_id": "research-google",
    "tab_id": "t1"
  },
  "data": {
    "session": {
      "session_id": "research-google",
      "mode": "local",
      "status": "running",
      "headless": false,
      "tabs_count": 1
    },
    "reopened": true
  },
  "meta": {
    "duration_ms": 650,
    "warnings": [],
    "pagination": null,
    "truncated": false
  }
}

文本

[research-google t1]
ok browser.restart
status: running

11.`actionbook browser list-tabs`

JSON

{
  "ok": true,
  "command": "browser.list-tabs",
  "context": {
    "session_id": "research-google"
  },
  "data": {
    "total_tabs": 1
    "tabs": [
      {
        "tab_id": "t1",
        "url": "https://google.com",
        "title": "Google",
        "native_tab_id": 391
      }
    ],
  },
  "meta": {
    "duration_ms": 11,
    "warnings": [],
    "pagination": null,
    "truncated": false
  }
}

文本

[research-google]
1 tab
[t1] Google
https://google.com

12.`actionbook browser new-tab` / `actionbook browser open`

JSON

{
  "ok": true,
  "command": "browser.new-tab",
  "context": {
    "session_id": "research-google",
    "tab_id": "t2",
    "url": "https://example.com",
    "title": "Example Domain"
  },
  "data": {
    "tab": {
      "tab_id": "t2",
      "url": "https://example.com",
      "title": "Example Domain",
      "native_tab_id": 392
    },
    "created": true,
    "new_window": false
  },
  "meta": {
    "duration_ms": 138,
    "warnings": [],
    "pagination": null,
    "truncated": false
  }
}

文本

[research-google t2] https://example.com
ok browser.new-tab
title: Example Domain

13.`actionbook browser close-tab`

JSON

{
  "ok": true,
  "command": "browser.close-tab",
  "context": {
    "session_id": "research-google",
    "tab_id": "t2"
  },
  "data": {
    "closed_tab_id": "t2",
  },
  "meta": {
    "duration_ms": 45,
    "warnings": [],
    "pagination": null,
    "truncated": false
  }
}

文本

[research-google t2]
ok browser.close-tab

14.`actionbook browser goto` / `back` / `forward` / `reload`

JSON

{
  "ok": true,
  "command": "browser.goto",
  "context": {
    "session_id": "research-google",
    "tab_id": "t1",
    "url": "https://google.com/search?q=actionbook",
    "title": "actionbook - Google Search"
  },
  "data": {
    "kind": "goto",
    "requested_url": "https://google.com/search?q=actionbook",
    "from_url": "https://google.com",
    "to_url": "https://google.com/search?q=actionbook",
    "title": "actionbook - Google Search"
  },
  "meta": {
    "duration_ms": 221,
    "warnings": [],
    "pagination": null,
    "truncated": false
  }
}

文本

[research-google t1] https://google.com/search?q=actionbook
ok browser.goto
title: actionbook - Google Search

15.`actionbook browser snapshot`

JSON

{
  "ok": true,
  "command": "browser.snapshot",
  "context": {
    "session_id": "research-google",
    "tab_id": "t1",
    "url": "https://google.com",
    "title": "Google"
  },
  "data": {
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
  },
  "meta": {
    "duration_ms": 41,
    "warnings": [],
    "pagination": null,
    "truncated": false
  }
}

文本

[research-google t1] https://google.com
- textbox "Search" [ref=e1]
- button "Google Search" [ref=e2]

规则：

- 文本模式下正文直接输出 snapshot 内容
- 不加额外说明文字
- 如有截断，仅在 meta.truncated 标记；文本模式可在末尾单独附一行 truncated: true
16.`actionbook browser screenshot`

JSON

{
  "ok": true,
  "command": "browser.screenshot",
  "context": {
    "session_id": "research-google",
    "tab_id": "t1",
    "url": "https://google.com",
    "title": "Google"
  },
  "data": {
    "artifact": {
      "path": "/tmp/google.png",
      "mime_type": "image/png",
      "bytes": 183920
    }
  },
  "meta": {
    "duration_ms": 77,
    "warnings": [],
    "pagination": null,
    "truncated": false
  }
}

文本

[research-google t1] https://google.com
ok browser.screenshot
path: /tmp/google.png

17.`actionbook browser pdf`

与 screenshot 相同，只是：

- command = "browser.pdf"
- artifact.mime_type = "application/pdf"
文本：

[research-google t1] https://google.com
ok browser.pdf
path: /tmp/google.pdf

18.`actionbook browser title` / `url` / `viewport`

JSON

{
  "ok": true,
  "command": "browser.title",
  "context": {
    "session_id": "research-google",
    "tab_id": "t1",
    "url": "https://google.com",
    "title": "Google"
  },
  "data": {
    "value": "Google"
  },
  "meta": {
    "duration_ms": 4,
    "warnings": [],
    "pagination": null,
    "truncated": false
  }
}

文本

[research-google t1] https://google.com
Google

url 使用：

{ "value": "<current_url>" }

viewport 使用：

{ "width": 1440, "height": 900 }

文本：

[research-google t1] https://google.com
1440x900

19.`actionbook browser query`

用途：query 是一个“带基数约束的元素查询命令”，不是单纯的 DOM 列表接口。

命令形态：

actionbook browser query one <query_str> --session <SID> --tab <TID>
actionbook browser query all <query_str> --session <SID> --tab <TID>
actionbook browser query nth <n> <query_str> --session <SID> --tab <TID>
actionbook browser query count <query_str> --session <SID> --tab <TID>

其中：

- one：要求恰好匹配 1 个元素
- all：返回全部匹配元素
- nth <n>：返回第 n 个匹配元素，n 为 1-based
- count：只返回匹配数量
query_str 使用扩展 CSS / jQuery 风格语法，当前至少支持：

- 标准 CSS selector
- :visible
- :contains(...)
- :has(...)
- :enabled
- :disabled
- :checked
返回的单个匹配元素统一为：

{
  "selector": ".item:nth-of-type(1)",
  "tag": "div",
  "text": "Item A",
  "visible": true,
  "enabled": true
}

mode 语义

`one`

- 成功条件：恰好 1 个匹配
- 0 个匹配：返回 ELEMENT_NOT_FOUND
- 多于 1 个匹配：返回 MULTIPLE_MATCHES
成功 JSON：

{
  "ok": true,
  "command": "browser.query",
  "context": {
    "session_id": "research-google",
    "tab_id": "t1",
    "url": "https://example.com"
  },
  "data": {
    "mode": "one",
    "query": ".item",
    "count": 1,
    "item": {
      "selector": ".item:nth-of-type(1)",
      "tag": "div",
      "text": "Item A",
      "visible": true,
      "enabled": true
    }
  },
  "meta": {
    "duration_ms": 12,
    "warnings": [],
    "pagination": null,
    "truncated": false
  }
}

成功文本：

[research-google t1] https://example.com
1 match
selector: .item:nth-of-type(1)
text: Item A

多匹配错误 JSON：

{
  "ok": false,
  "command": "browser.query",
  "context": {
    "session_id": "research-google",
    "tab_id": "t1",
    "url": "https://example.com"
  },
  "data": null,
  "error": {
    "code": "MULTIPLE_MATCHES",
    "message": "Query mode 'one' requires exactly 1 match, found 3",
    "retryable": false,
    "details": {
      "query": ".item",
      "count": 3,
      "sample_selectors": [
        ".item:nth-of-type(1)",
        ".item:nth-of-type(2)",
        ".item:nth-of-type(3)"
      ]
    }
  },
  "meta": {
    "duration_ms": 12,
    "warnings": [],
    "pagination": null,
    "truncated": false
  }
}

多匹配错误文本：

[research-google t1] https://example.com
error MULTIPLE_MATCHES: Query mode 'one' requires exactly 1 match, found 3

 `all`

- 始终返回列表
- 0 个匹配也视为成功，items = []
JSON：

{
  "ok": true,
  "command": "browser.query",
  "context": {
    "session_id": "research-google",
    "tab_id": "t1",
    "url": "https://example.com"
  },
  "data": {
    "mode": "all",
    "query": ".item",
    "count": 3,
    "items": [
      {
        "selector": ".item:nth-of-type(1)",
        "tag": "div",
        "text": "Item A",
        "visible": true,
        "enabled": true
      }
    ]
  },
  "meta": {
    "duration_ms": 12,
    "warnings": [],
    "pagination": null,
    "truncated": false
  }
}

文本：

[research-google t1] https://example.com
3 matches
1. .item:nth-of-type(1)
   Item A

 `nth <n>`

- n 为 1-based
- 成功条件：1 <= n <= count
- 若 n 超出范围，返回 INDEX_OUT_OF_RANGE
JSON：

{
  "ok": true,
  "command": "browser.query",
  "context": {
    "session_id": "research-google",
    "tab_id": "t1",
    "url": "https://example.com"
  },
  "data": {
    "mode": "nth",
    "query": ".item",
    "index": 2,
    "count": 3,
    "item": {
      "selector": ".item:nth-of-type(2)",
      "tag": "div",
      "text": "Item B",
      "visible": true,
      "enabled": true
    }
  },
  "meta": {
    "duration_ms": 12,
    "warnings": [],
    "pagination": null,
    "truncated": false
  }
}

文本：

[research-google t1] https://example.com
match 2/3
selector: .item:nth-of-type(2)
text: Item B

 `count`

- 只返回匹配数量
- 永远不返回元素详情
JSON：

{
  "ok": true,
  "command": "browser.query",
  "context": {
    "session_id": "research-google",
    "tab_id": "t1",
    "url": "https://example.com"
  },
  "data": {
    "mode": "count",
    "query": ".item",
    "count": 3
  },
  "meta": {
    "duration_ms": 12,
    "warnings": [],
    "pagination": null,
    "truncated": false
  }
}

文本：

[research-google t1] https://example.com
3

20. 读取类命令

覆盖：

- html
- text
- value
- attr
- attrs
- box
- styles
统一 JSON 结构：

{
  "ok": true,
  "command": "browser.text",
  "context": {
    "session_id": "research-google",
    "tab_id": "t1",
    "url": "https://example.com"
  },
  "data": {
    "target": {
      "selector": "#title"
    },
    "value": "Example Domain"
  },
  "meta": {
    "duration_ms": 7,
    "warnings": [],
    "pagination": null,
    "truncated": false
  }
}

文本规则：首行上下文，后面直接输出值。

#### `text`

[research-google t1] https://example.com
Example Domain

#### `html`

[research-google t1] https://example.com
<h1 id="title">Example Domain</h1>

#### `attr`

[research-google t1] https://example.com
https://google.com

#### `attrs`

正文输出 key-value 列表。

#### `box`

[research-google t1] https://example.com
x: 10
y: 20
width: 120
height: 32

#### `styles`

正文输出请求到的 style 名和值。

21.`actionbook browser describe`

作用：返回元素的规则化摘要，用于快速理解“这是什么元素”，不调用 LLM。

默认只描述目标元素自身；指定 --nearby 时，额外返回一层极浅的邻近上下文，用于消歧。

摘要只允许基于以下现成信号生成：

- DOM tag
- ARIA role
- accessible name
- label / text / placeholder / value
- 关键属性，如 type、href
- 当前状态，如 disabled、checked、selected
不做额外语义推断，不输出诸如“primary”“main CTA”“关键按钮”这类需要模型理解的词。

摘要拼装优先级：

1. role：ARIA role 优先，否则回退到 native tag 映射
2. name：accessible name 优先，否则依次回退到 label、text、placeholder、value
3. qualifiers：补充必要属性和状态，但不扩写成长句


--nearby 的上下文约束：

- 只返回 1 层，不递归
- parent 最多 1 个
- previous_sibling / next_sibling 最多各 1 个
- children 最多 3 个
- 仅返回有名字、文本或可交互意义的节点
- 邻近节点也必须使用同样的 deterministic summary，不返回整段 HTML 或完整 subtree

JSON

{
  "ok": true,
  "command": "browser.describe",
  "context": {
    "session_id": "research-google",
    "tab_id": "t1",
    "url": "https://google.com"
  },
  "data": {
    "target": {
      "selector": "button[type=submit]"
    },
    "summary": "button \"Google Search\"",
    "role": "button",
    "name": "Google Search",
    "tag": "button",
    "attributes": {
      "type": "submit"
    },
    "state": {
      "visible": true,
      "enabled": true
    },
    "nearby": null
  },
  "meta": {
    "duration_ms": 9,
    "warnings": [],
    "pagination": null,
    "truncated": false
  }
}

文本

[research-google t1] https://google.com
button "Google Search"

 `--nearby` JSON

{
  "ok": true,
  "command": "browser.describe",
  "context": {
    "session_id": "research-google",
    "tab_id": "t1",
    "url": "https://google.com"
  },
  "data": {
    "target": {
      "selector": "button[type=submit]"
    },
    "summary": "button \"Edit\"",
    "role": "button",
    "name": "Edit",
    "tag": "button",
    "attributes": {
      "type": "button"
    },
    "state": {
      "visible": true,
      "enabled": true
    },
    "nearby": {
      "parent": "listitem \"John Smith\"",
      "previous_sibling": "text \"John Smith\"",
      "next_sibling": null,
      "children": []
    }
  },
  "meta": {
    "duration_ms": 11,
    "warnings": [],
    "pagination": null,
    "truncated": false
  }
}

 `--nearby` 文本

[research-google t1] https://google.com
button "Edit"
parent: listitem "John Smith"
previous_sibling: text "John Smith"

22.`actionbook browser state`

JSON

{
  "ok": true,
  "command": "browser.state",
  "context": {
    "session_id": "research-google",
    "tab_id": "t1",
    "url": "https://google.com"
  },
  "data": {
    "target": {
      "selector": "#search"
    },
    "state": {
      "visible": true,
      "enabled": true,
      "checked": false,
      "focused": true,
      "editable": true,
      "selected": false
    }
  },
  "meta": {
    "duration_ms": 6,
    "warnings": [],
    "pagination": null,
    "truncated": false
  }
}

文本

[research-google t1] https://google.com
visible: true
enabled: true
checked: false
focused: true
editable: true
selected: false

23.`actionbook browser inspect-point`

JSON

{
  "ok": true,
  "command": "browser.inspect-point",
  "context": {
    "session_id": "research-google",
    "tab_id": "t1",
    "url": "https://google.com"
  },
  "data": {
    "point": {
      "x": 420,
      "y": 310
    },
    "element": {
      "role": "button",
      "name": "Google Search",
      "selector": "input[name=btnK]"
    },
    "parents": [],
    "screenshot_path": null
  },
  "meta": {
    "duration_ms": 18,
    "warnings": [],
    "pagination": null,
    "truncated": false
  }
}

文本

[research-google t1] https://google.com
button "Google Search"
selector: input[name=btnK]
point: 420,310

24. 日志类命令

覆盖：

- browser logs console
- browser logs errors
JSON

{
  "ok": true,
  "command": "browser.logs.console",
  "context": {
    "session_id": "research-google",
    "tab_id": "t1",
    "url": "https://example.com"
  },
  "data": {
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
  },
  "meta": {
    "duration_ms": 5,
    "warnings": [],
    "pagination": null,
    "truncated": false
  }
}

文本

[research-google t1] https://example.com
1 log
info 1710000000000 app.js App mounted

25. 动作类命令

覆盖：

- click
- hover
- focus
- press
- drag
- mouse-move
- scroll
统一 JSON：

{
  "ok": true,
  "command": "browser.click",
  "context": {
    "session_id": "research-google",
    "tab_id": "t1",
    "url": "https://google.com",
    "title": "Google"
  },
  "data": {
    "action": "click",
    "target": {
      "selector": "button[type=submit]"
    },
    "changed": {
      "url_changed": false,
      "focus_changed": true
    }
  },
  "meta": {
    "duration_ms": 14,
    "warnings": [],
    "pagination": null,
    "truncated": false
  }
}

文本

[research-google t1] https://google.com
ok browser.click
target: button[type=submit]

特殊规则：

- 若 click 导致跳转，context.url 必须更新为跳转后 URL
- 若 scroll 有位移，data.changed.scroll_changed = true
- 若 press 是 chord，target 可省略，改为 keys
26. 输入类命令

覆盖：

- type
- fill
- select
- upload
统一 JSON：

{
  "ok": true,
  "command": "browser.fill",
  "context": {
    "session_id": "research-google",
    "tab_id": "t1",
    "url": "https://google.com"
  },
  "data": {
    "action": "fill",
    "target": {
      "selector": "textarea[name=q]"
    },
    "value_summary": {
      "text_length": 10
    }
  },
  "meta": {
    "duration_ms": 12,
    "warnings": [],
    "pagination": null,
    "truncated": false
  }
}

文本

[research-google t1] https://google.com
ok browser.fill
target: textarea[name=q]
text_length: 10

特殊规则：

 `select`

- value_summary = { value, by_text?: boolean }
 `upload`

- value_summary = { files: ["/abs/path/a.pdf"], count: 1 }
27.`actionbook browser eval`

JSON

{
  "ok": true,
  "command": "browser.eval",
  "context": {
    "session_id": "research-google",
    "tab_id": "t1",
    "url": "https://example.com"
  },
  "data": {
    "value": 42,
    "type": "number",
    "preview": "42"
  },
  "meta": {
    "duration_ms": 8,
    "warnings": [],
    "pagination": null,
    "truncated": false
  }
}

文本

[research-google t1] https://example.com
42

28. 等待类命令

覆盖：

- wait element
- wait navigation
- wait network-idle
- wait condition
统一 JSON：

{
  "ok": true,
  "command": "browser.wait.element",
  "context": {
    "session_id": "research-google",
    "tab_id": "t1",
    "url": "https://example.com"
  },
  "data": {
    "kind": "element",
    "satisfied": true,
    "elapsed_ms": 182,
    "observed_value": {
      "selector": "#loaded"
    }
  },
  "meta": {
    "duration_ms": 182,
    "warnings": [],
    "pagination": null,
    "truncated": false
  }
}

文本

[research-google t1] https://example.com
ok browser.wait.element
elapsed_ms: 182

29. Cookies

覆盖：

- cookies list
- cookies get
- cookies set
- cookies delete
- cookies clear
#### `cookies list` JSON

{
  "ok": true,
  "command": "browser.cookies.list",
  "context": {
    "session_id": "research-google"
  },
  "data": {
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
  },
  "meta": {
    "duration_ms": 6,
    "warnings": [],
    "pagination": null,
    "truncated": false
  }
}

文本

[research-google]
1 cookie
SID .google.com /

其他 cookies 命令

- cookies get：data = { "item": { ...cookie } }
- cookies set：data = { "action": "set", "affected": 1, "domain": ".google.com" }
- cookies delete：data = { "action": "delete", "affected": 1 }
- cookies clear：data = { "action": "clear", "affected": 5, "domain": ".google.com" }
30. Storage

覆盖：

- session-storage list/get/set/delete/clear
- local-storage list/get/set/delete/clear
 `list` JSON

{
  "ok": true,
  "command": "browser.local-storage.list",
  "context": {
    "session_id": "research-google",
    "tab_id": "t1",
    "url": "https://example.com"
  },
  "data": {
    "storage": "local",
    "items": [
      {
        "key": "theme",
        "value": "dark"
      }
    ]
  },
  "meta": {
    "duration_ms": 4,
    "warnings": [],
    "pagination": null,
    "truncated": false
  }
}

文本

[research-google t1] https://example.com
1 key
theme=dark

其他 storage 命令

- get：data = { "storage": "local", "item": { "key": "theme", "value": "dark" } }
- set：data = { "storage": "local", "action": "set", "affected": 1 }
- delete：data = { "storage": "local", "action": "delete", "affected": 1 }
- clear：data = { "storage": "local", "action": "clear", "affected": 3 }
错误协议

所有失败返回：

{
  "ok": false,
  "command": "browser.click",
  "context": {
    "session_id": "research-google",
    "tab_id": "t1",
    "url": "https://google.com"
  },
  "data": null,
  "error": {
    "code": "ELEMENT_NOT_FOUND",
    "message": "Element not found: button[type=submit]",
    "retryable": false,
    "details": {
      "selector": "button[type=submit]"
    }
  },
  "meta": {
    "duration_ms": 3012,
    "warnings": [],
    "pagination": null,
    "truncated": false
  }
}

文本失败格式：

[research-google t1] https://google.com
error ELEMENT_NOT_FOUND: Element not found: button[type=submit]

推荐错误码

- INVALID_ARGUMENT
- SESSION_NOT_FOUND
- TAB_NOT_FOUND
- FRAME_NOT_FOUND
- ELEMENT_NOT_FOUND
-  MULTIPLE_MATCHES
- INDEX_OUT_OF_RANGE
- TIMEOUT
- NAVIGATION_FAILED
- EVAL_FAILED
- ARTIFACT_WRITE_FAILED
- UNSUPPORTED_OPERATION
- INTERNAL_ERROR
兼容与边界规则

- 非会话命令必须省略 context
- 会话命令只要已经定位到 session，就必须返回 context.session_id
- tab 命令只要已经定位到 tab，就必须返回 context.tab_id
- data 必须始终存在；失败时为 null
- error 必须始终存在；成功时为 null
- command 使用规范化名字，不使用原始 argv 字符串
- 文本输出不允许混入实现细节日志
- 默认文本输出和 JSON 语义必须一致
测试场景

基础一致性

- search/get/setup/help/version 不返回 context
- browser 命令在适用时总返回 session_id
- tab 级命令总返回 tab_id
ID 规则

- browser start --session-name research-google 返回 session_id = "research-google"
- 在同一 session 内依次新建 tab，返回 t1, t2, t3
- native_tab_id 可变化，但 tab_id 对上层保持稳定
文本格式

- snapshot 的正文不夹杂提示语
- text/html/eval 正文直接输出值
- click/fill/upload/wait 第二行固定以 ok <command> 开头
- 错误时固定为 error <CODE>: <message>
JSON 结构

- 所有命令都有 ok/command/data/error/meta
- 分页信息只出现在 meta.pagination
- 截断信息只出现在 meta.truncated
Query 语义

- browser query one 仅在恰好 1 个匹配时成功
- browser query one 在匹配数大于 1 时返回 MULTIPLE_MATCHES
- browser query one 在匹配数为 0 时返回 ELEMENT_NOT_FOUND
- browser query nth <n> 使用 1-based 索引
- browser query nth <n> 在 n > count 时返回 INDEX_OUT_OF_RANGE
- browser query all 在 0 个匹配时仍成功，返回空数组
- browser query count 在 0 个匹配时仍成功，仅返回 count
跳转与上下文更新

- goto/click/back/forward/reload 成功后更新 context.url
- 页面 title 已知时同步更新 context.title
默认结论

本版本先采纳以下默认方案：

- session_id 为语义化字符串，支持用户指定
- tab_id 为会话内短 ID tN
- native_tab_id 仅作为可选调试字段
- 非会话命令省略 context
- JSON 与文本输出都属于正式 contract
- 统一 envelope，不延续旧版返回格式