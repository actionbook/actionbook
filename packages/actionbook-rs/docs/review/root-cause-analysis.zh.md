# actionbook-rs 根因分析：第一性原理

**日期：** 2026-03-25
**方法：** 5 轮交叉讨论（Claude 架构分析 + OpenAI Codex 对抗性分析 + 综合审视 + Bug 历史验证）
**Token 总消耗：** ~10.5M

---

## 三轮讨论过程

| 轮次 | 参与者 | 主题 | 核心洞察 |
|------|--------|------|---------|
| R1-A | Claude Agent | 架构根因分析 | 4 个根因：分布式状态、隐含分层、Daemon 可选、文件系统 IPC |
| R1-B | Codex | 第一性原理分析 | 4 个根因：无状态拥有者、无信任边界、无领域模型、所有权竞争 |
| R2 | Codex | 交叉审视与挑战 | 统一为 3 个根因；发现两者遗漏的产品边界、增长拐点、文档自欺 |
| R3 | 综合 | 最终模型 | 本文档 |

---

## 核心发现：一句话总结

> **系统把两个不同的产品上下文（受管浏览器 vs 用户浏览器）硬塞进一个 `BrowserMode`，然后在没有建立"谁拥有状态、谁拥有权限、什么是合法状态转换"这些基础定义的情况下，用文件系统作为多进程协调总线，逐步演化出了 daemon、multi-session、extension bridge 等并发需求。20 个 CUE issue 是这个根本失误的自然产物。**

---

## 三个根因

### 根因 1：错误的产品抽象 — 两个执行上下文伪装成一个

```
                    ┌───────────────────────────────┐
                    │        actionbook browser      │
                    │     （被建模为单一产品概念）      │
                    └───────────┬───────────────────┘
                                │
            ┌───────────────────┼───────────────────┐
            ▼                                        ▼
   ┌─────────────────┐                    ┌─────────────────┐
   │   Isolated       │                    │   Extension      │
   │                  │                    │                  │
   │ ✓ 可复现         │                    │ ✓ 真实登录态      │
   │ ✓ CI/Headless    │                    │ ✓ 用户 Cookie    │
   │ ✓ 多 Profile     │                    │ ✗ 无 Profile     │
   │ ✓ 多 Session     │                    │ ✗ 无 Multi-session│
   │ ✓ Daemon 管理    │                    │ ✗ 走 Bridge 不走  │
   │ ✓ CDP 直连       │                    │   Daemon         │
   └─────────────────┘                    └─────────────────┘
         不同的                                  不同的
     ├ Trust model                          ├ Trust model
     ├ Lifecycle                            ├ Lifecycle
     ├ Auth mechanism                       ├ Auth mechanism
     ├ State ownership                      ├ State ownership
     └ UX promise                           └ UX promise
```

**本质问题：** 这不是"同一产品的两种 transport"，而是**两种完全不同的执行上下文**，各自拥有不同的信任模型、生命周期、认证机制、状态所有权和 UX 承诺。

**证据：**
- `--profile` 在 extension 模式下是硬错误（`browser.rs:843`）
- Multi-session 只支持 isolated 模式
- Extension bridge 有自己的认证体系（Origin header）与 daemon 的 UDS 完全不同
- Extension 模式不走 daemon，走 bridge lifecycle

**级联效应：**
- 33 个错误变体中，很多是因为两种模式共享 error enum 但语义不同
- 用户困惑（R4："我该选哪个模式？"）不是 UI 文案问题，是产品概念本身混淆
- `run()` 函数前 75 行全在做模式判断和分支 — 如果两个模式是独立概念，这些分支就不会存在

**如果纠正：** `Isolated` 和 `Extension` 应该是 `BrowserBackend` trait 下的两个独立实现，各自拥有完整的 state management、auth、lifecycle，只在命令接口层统一。

---

### 根因 2：错误的权威抽象 — 无人拥有状态、权限和生命周期

```
当前状态所有权图（实际）：

     CLI 进程 A ──┐
                  │  竞争写入
     CLI 进程 B ──┤──→ ~/.actionbook/sessions/{profile}.json
                  │
     Daemon ──────┘

     CLI ──→ 启动 Daemon（ensure_daemon）
     Daemon ──→ 写 PID 文件、绑 socket
     CLI ──→ 读 PID 文件、连 socket
     另一个 CLI ──→ 也启动 Daemon？（无锁！）

正确的所有权图（应有）：

     CLI ──→ Daemon（唯一权威）──→ State
              │
              ├─ 拥有 session 状态（唯一写者）
              ├─ 拥有 browser 连接（唯一持有者）
              ├─ 拥有 process identity（可验证）
              └─ 发放 capability token（可撤销）
```

**本质问题：** 系统中没有**唯一的运行时权威**（single authority）。State、identity、authority 全部是 "ambient" 的 — 来自文件存在性、PID、Origin header、端口号、PATH 环境变量 — 而非来自可验证的 capability。

**Codex 的精确表述：**
> "系统靠文件、PID、Origin、端口去推断身份和所有权，于是文件成了 IPC，daemon/CLI/extension 成了竞争控制者。"

**证据链条：**

| 信任来源 | 用在哪里 | 为什么是错的 |
|----------|---------|------------|
| 文件存在性 | `lifecycle.rs:86` — PID 文件存在 = daemon 活着 | PID 可重用，文件可残留 |
| Origin header | `extension_bridge.rs:636` — Origin 对 = 真 extension | 任何本地进程可伪造 |
| 端口号 | `auto_connect.rs:129` — 9222 响应 = Chrome | 任何进程可监听 |
| PATH | `native_messaging.rs:180` — which = 合法二进制 | PATH 可被污染 |
| PID 数字 | `lifecycle.rs:316` — kill(pid,0) = 进程存活 | PID 重用后杀错进程 |
| stdout | `extension.rs:69` — 打印 token | 终端录制可捕获 |

**级联效应：**
- 安全问题（R1 全部 7 个）都是 ambient trust 的直接后果
- 可靠性问题（R3 #1-2 PID 竞态、#2 重复 daemon、#5-6 状态覆盖）都是无 authority 的后果
- 架构重复（R2 #3-4 sanitize/is_pid_alive 重复）是因为没有 `core` 权威模块

**如果纠正：** 引入 capability-based security model：
1. Daemon 是唯一 authority，发放可验证的 session token
2. 所有 CLI→Daemon 通信通过 token 认证
3. Extension bridge 使用 native messaging 签发的一次性密钥
4. 文件系统只做持久化，不做协调

---

### 根因 3：错误的语义抽象 — 无跨层共享的领域模型和状态机

```
当前的"类型系统"：

    profile_name: &str  ← 到处传递的裸字符串
    session_name: &str  ← 另一个裸字符串
    cdp_url: String     ← 又一个裸字符串

    SessionState {       }  ← CLI 的视图（session.rs:29）
    SessionInfo  {       }  ← Daemon 的视图（server.rs:173）
    sanitize_name()         ← 3 份不同实现
    is_pid_alive()          ← 2 份不同实现
    ActionbookError::Other(String)  ← 69 处使用的逃生舱

应有的"类型系统"：

    ValidatedProfileName   ← newtype，构造时验证，只有一个验证点
    SessionHandle          ← 包含 profile + session + version
    CdpEndpoint            ← 包含 URL + 认证信息 + 可达性状态

    SessionState           ← 唯一类型，带 version 字段
    ActionbookError        ← 按恢复策略分类，非按实现层分类
```

**本质问题：** 系统的核心概念（profile、session、browser mode、error）没有被建模为**可执行的类型和状态机**。它们只是字符串和布尔值在 `if/match` 中流动。

**Codex 的关键洞察 — 错误分类轴错了：**

当前 `ActionbookError` 按**实现层**分类：
```
CdpError / DaemonError / ExtensionError / IoError / NetworkError / Other
```

应按**调用者恢复策略**分类：
```
Retryable(reason)  / UserActionRequired(what_to_do) / Fatal(cause) / NotApplicable(why)
```

`Other(String)` 被使用 69 次 — 它不是错误变体，是**错误系统投降的标志**。

**级联效应：**
- DRY 违规（R2 #3, #4, #8）是因为没有 `core` 模块定义共享类型
- SessionInfo/SessionState 漂移（R2 #5）是因为同一概念有两个定义
- 263 个 `unwrap()`（R2 #6）是因为没有 Result 传播的文化 — 来自缺少明确的错误模型
- 产品混淆（R4 #2, #4）是因为 profile/session/mode 在用户面前也只是字符串

---

## 增长拐点分析

这个架构不是"从第一天就错"。Codex 在第二轮中通过 git 历史识别了三个关键拐点：

```
时间线：

Phase 1: 单进程 + 单 session + isolated CDP
├─ 文件做跨 CLI 重入虽然粗糙，但局部一致
├─ 无并发问题（只有一个进程写状态）
└─ 架构：OK

Phase 2: 2026-02-09 (7f32f83) Extension Bridge 引入
├─ 第二种 authority/trust model 塞进同一命令面
├─ Origin 认证、bridge lifecycle、token 管理
├─ 两个执行上下文开始混合
└─ 架构开始分裂 ← 拐点 1

Phase 3: 2026-03-17 (317269d) Daemon 设为 Unix 默认
├─ "旁路进程"变成事实上的核心路径
├─ 多进程并发写状态成为常态
├─ 但状态管理仍是单进程设计
└─ 并发问题爆发 ← 拐点 2

Phase 4: 2026-03-19 (99f3c36) Multi-session 引入
├─ session 文件从"缓存"变成"分布式控制平面"
├─ {profile}@{session}.json 格式引入
├─ daemon 需要 per-session routing
└─ 复杂度超过单文件心智模型极限 ← 拐点 3
```

**关键洞察：** 每个拐点都是在**不改变基础层（文件系统 IPC + 无 authority + 无领域模型）**的前提下，叠加了新的并发和信任需求。最终的 20 个问题是增量演化的累积债务。

---

## 文档自欺

第二轮 Codex 发现了一个更深层的问题：设计文档已经与实现脱节。

> 设计文档 `02-process.md:206` 声称"会话状态 = 文件锁 + 原子写入"
> 但实现中 `session.rs:320` 是裸 `read_to_string`，`session.rs:354` 是裸 `fs::write`
> 没有文件锁，没有 CAS，没有 version

**含义：** 问题不仅是"代码坏了"，而是 **architecture 已经不是约束，而是事后描述**。修复代码不够 — 需要重新确立"文档定义约束，实现满足约束"的工程纪律。

---

## Rust 特性分析

第二轮 Codex 指出：**Rust 不是罪魁，反而让这些问题更不该发生。**

| Rust 特性 | 本应帮助的地方 | 实际发生的 |
|-----------|-------------|----------|
| Sum types (enum) | 定义 BrowserMode 为正交类型 | 退化为 `if cli.extension` 布尔分支 |
| Ownership | 强制状态的唯一拥有者 | 多进程绕过 ownership 用文件系统共享 |
| Result 传播 | 优雅错误处理链 | 263 个 unwrap() + 69 个 Other(String) |
| Newtype pattern | ValidatedProfileName 等安全类型 | 到处传裸 `&str` |
| Trait | BrowserBackend 统一接口 | router.rs 有 trait 定义但 9 个 dead_code |

**结论：** Rust 的类型系统本可以在编译期防止大部分问题。团队选择了绕过 Rust 的安全机制（裸字符串、unwrap、文件 IPC），而非利用它们。这不是语言限制，是设计选择。

---

## 统一症状地图

```
根因 1（产品抽象错误）
├─ R4: Extension vs Isolated 用户困惑
├─ R4: --profile 在 extension 下报错
├─ R4: 33 个错误变体混合两种模式语义
├─ R2: browser.rs run() 前 75 行全在做模式分支
└─ R1: Extension bridge 和 Daemon 的认证体系完全不同

根因 2（权威抽象错误）
├─ R1 #1: UDS 无认证（ambient trust）
├─ R1 #2: Origin 认证可伪造（ambient trust）
├─ R1 #3: 路径遍历（ambient path trust）
├─ R1 #5: PID 信任（ambient PID trust）
├─ R1 #6: Token 泄漏（无 secret boundary）
├─ R1 #7: PATH 劫持（ambient PATH trust）
├─ R3 #1: PID 竞态（无 authority 验证）
├─ R3 #2: 重复 daemon（无单实例 authority）
├─ R3 #3: 僵尸 daemon（无 failure authority）
├─ R3 #5-6: 状态竞争写入（无 write authority）
├─ R3 #7: 自动发现信任任何回环服务（ambient port trust）
└─ R3 #11: 每命令 WebSocket（无 connection authority）

根因 3（语义抽象错误）
├─ R2 #1: SessionManager 上帝对象（无模块化领域模型）
├─ R2 #2: browser.rs 上帝函数（无领域分层）
├─ R2 #3: 3x sanitize_name（无 canonical validation）
├─ R2 #4: 2x is_pid_alive（无 shared core）
├─ R2 #5: SessionInfo/SessionState 漂移（无 canonical state type）
├─ R2 #6: 263 个 unwrap()（无 error propagation culture）
├─ R2 #7: 65 个 dead_code（trait 定义了但没用）
├─ R4 #3: 错误信息不可操作（按实现层分类，非恢复策略）
└─ R4 #4: Profile vs Session 命名混淆（概念在代码中无清晰定义）
```

---

## 修复策略：从根因到行动

### 不要做什么

1. **不要一个一个修 20 个 CUE issue** — 它们会互相打架
2. **不要只强制 daemon** — Extension 模式不走 daemon，强制它不能统一系统
3. **不要只做文件锁** — 锁住一个错误的状态模型还是错的

### 应该做什么（按根因倒推）

**Phase 0：建立 `core` 模块（修复根因 3）— 2-3 天**
```
src/core/
├── profile.rs      ValidatedProfileName（唯一验证点）
├── session.rs      SessionMetadata（唯一状态类型）
├── process.rs      is_pid_alive（唯一实现）
├── error.rs        按恢复策略分类的错误类型
└── mod.rs
```
所有现有代码的 `sanitize_name`、`is_pid_alive`、`SessionInfo`/`SessionState` 都指向 core。

**Phase 1：分离两个执行上下文（修复根因 1）— 3-5 天**
```
src/backend/
├── trait.rs        BrowserBackend trait（统一命令接口）
├── isolated.rs     IsolatedBackend（CDP 直连 + Daemon）
└── extension.rs    ExtensionBackend（Bridge + Native Messaging）
```
`run()` 变成：先选 backend，然后通过 trait 统一分发。不再有 `if cli.extension` 散落。

**Phase 2：建立唯一 authority（修复根因 2）— 1-2 周**
- Isolated 模式：Daemon 是唯一 authority
  - 拥有 session 状态（唯一写者）
  - 拥有 browser 连接（唯一持有者）
  - 发放 token 给 CLI
  - CLI 是无状态客户端
- Extension 模式：Bridge 是唯一 authority
  - 拥有 tab 绑定
  - 通过 native messaging 密钥认证
  - CLI 是无状态客户端
- 文件系统只做持久化，不做协调

**Phase 3：重组文件结构（根因 3 的物理实现）— 1 周**
- `session.rs` 5,846 行 → 6-8 个子模块
- `browser.rs` 6,709 行 → 5-6 个子模块
- 65 个 dead_code 审计删除
- 263 个 unwrap() 替换

---

## 结语

Codex 在第二轮给出的收束最为精确：

> **A 讲的是"控制平面分裂"，B 讲的是"语义边界分裂"；真正的总根因是，产品先把不同 execution context 混成了一个"browser"，随后实现又没有建立 authority 和 state machine 去兜住它。如果不先把这三层切开，修文件锁、补 enum、甚至强制 daemon，都会只是把错误中心化，而不是把错误消灭。**

---

## 附录：3 月 Bug Fix 历史验证（第 4-5 轮讨论）

### 方法

对 2026-03-01 到 2026-03-25 的 25 个 bug fix commit 进行独立分析（Claude Agent + Codex），逐个判定每个 bug 是**架构必然产物**还是**偶然实现失误**，并验证是否支持 3 个根因。

### 验证统计

| 指标 | Claude Agent | Codex | 共识 |
|------|-------------|-------|------|
| 架构必然的 bug 占比 | 72%（18/25）| 76%（19/25）| **~74%** |
| RC1（产品抽象）验证率 | 52%（13/25）| 类似 | **强验证** |
| RC2（权威抽象）验证率 | 64%（16/25）| 最强 | **最强验证** |
| RC3（语义抽象）验证率 | 52%（13/25）| 类似 | **强验证** |
| 多根因叠加占比 | 68%（17/25）| — | **根因共振** |

### 根因修订建议

两个分析者都建议修订原有根因表述：

**RC1 扩大化：** 不仅是 "Isolated vs Extension"，而是至少 5 种执行上下文的混合：
```
本地 CDP → 远端 CDP(WSS) → Daemon-owned CDP → Extension Bridge → Electron App
```
Bug #3（liveness probe 429 on 单连接远端）、#11-12（remote open 反复修）、#21（Electron app 3 个 commit 修一轮）都证明了 RC1 的范围应扩展为 **"Wrong Execution-Context Abstraction"**。

**RC3 扩大化：** 不仅缺领域模型，更缺**显式状态机**。Codex 在 diff 中发现大量修复本质是"补状态位"：
- `is_daemon_mode()` — 运行时模式没有显式状态
- `uses_local_http_endpoints()` — endpoint 拓扑没有类型
- `should_verify_connect_via_daemon()` — 验证策略没有建模
- `watch<bool>` 替换 `Notify` — 就绪状态从事件改为状态
- `known_page_id` — tab identity 从发现改为显式传递
- `pre-send vs post-send` — 请求生命周期没有状态机

应改为 **"Wrong Semantic + Lifecycle Abstraction"**。

**新增 RC4（连接拓扑无区分）：**
```
Local CDP（多连接、无 TTL、localhost 安全）
Remote WSS（单连接、有 TTL、需加密）
Extension Bridge（事件驱动、被动断开）
```
Bug #3、#9、#12、#13 揭示即使在同一 BrowserMode 内部，连接拓扑差异也导致系统性故障。建议新增或作为 RC1 子类。

### 增长轨迹验证

Bug 在时间线上**高度聚集**，与 3 个拐点完美对齐：

```
2026-03-17  daemon 默认化（317269d）
            └─ 当天连出 421e05b、eb6e3f2、53557c7 三个 daemon fix

2026-03-19  multi-session 上线（99f3c36）
            └─ 随后连出 89dbdfa、dc73758、d1147e6、164b18d、5813d08、0110bf9
               （6 个 session/tab routing fix）

2026-03-22  remote WSS 全面支持
            └─ 015b4e9、60a058f、94e65dd、b2d2048、f93a0bb、74abd9a
               （6 个 remote endpoint fix）
```

**结论：** Bug 不是均匀分布的。每次新功能跨越 authority/semantic 边界时，组合复杂度爆炸。这强烈支持原根因分析的核心论点——缺陷来自**基础层不支撑上层需求的演化**，而非随机实现失误。

### 最具说服力的 Bug

**Bug #4 (164b18d): Session detection/execution alignment mismatch**

两个分析者一致认为这是最能证明根因分析正确的 bug：
- `saved_session_state_for_reuse` 和 `ensure_session_state_for_cdp` 对 `-S default` 的语义理解不一致
- `Option<String>` 中 `None`（未指定）和 `Some("default")`（显式指定 default）类型相同但语义不同
- 这个 bug 是**架构不可能不产生的** — 只要 session identity 是裸字符串且 "default" 是 magic value，任何新代码路径都面临同样歧义

### 最令人意外的 Bug

**Bug #13 (53557c7): u64::MAX 溢出 Chrome CDP integer parser**

Rust 的 `u64::MAX` 在 JSON 序列化后超过 JavaScript `Number.MAX_SAFE_INTEGER`（2^53-1），Chrome CDP parser 静默截断，导致 daemon 永远匹配不到 `Target.attachToTarget` 响应而无限挂起。

深层原因：用哨兵值（u64::MAX）代替 `Option<u64>` 语义 — RC3 的精确验证。同时揭示了跨协议边界的类型安全问题。

### 最终修订的根因模型

```
┌─────────────────────────────────────────────────────────────────┐
│                    修订后的根因模型                                │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│  RC1: 错误的执行上下文抽象（扩大化）                               │
│  ├─ 原：Isolated vs Extension 两种模式混合                       │
│  └─ 修订：Local CDP / Remote WSS / Daemon CDP / Extension /     │
│           Electron 五种上下文 + 三种连接拓扑 被统一处理            │
│                                                                  │
│  RC2: 错误的权威抽象（不变，最强验证）                             │
│  ├─ 无单一 authority                                             │
│  └─ Ambient trust 代替 verifiable capability                    │
│                                                                  │
│  RC3: 错误的语义 + 生命周期抽象（扩大化）                          │
│  ├─ 原：无共享领域模型                                            │
│  └─ 修订：无共享领域模型 + 无显式状态机                            │
│           （ready/attached/pre-send/replaced 等状态都没建模）      │
│                                                                  │
│  验证强度：25 bug 中 74% 是架构必然，68% 涉及多根因共振            │
└─────────────────────────────────────────────────────────────────┘
```
