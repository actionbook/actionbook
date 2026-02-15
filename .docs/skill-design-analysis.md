# Skill 设计哲学分析：Context not Control

## 核心结论

通过对 Playwright CLI（微软官方）、Context7（Upstash）和当前 Actionbook skill 的对比分析，核心设计哲学可以总结为：

> **Context not Control** — Skill 的职责是向 AI Agent 提供准确、结构化的上下文信息，而不是规定 Agent 必须如何行动。

---

## 一、官方 Skill 设计哲学剖析

### 1.1 Playwright CLI Skill

**文件**: `skills/playwright-cli/SKILL.md`
**来源**: https://github.com/microsoft/playwright-cli

#### 设计特征

| 特征 | 具体表现 |
|------|---------|
| **极简的 description** | `"Automates browser interactions for web testing, form filling, screenshots, and data extraction."` — 一句话说清楚能力边界 |
| **工具约束而非流程约束** | `allowed-tools: Bash(playwright-cli:*)` — 只限制可用工具，不限制使用方式 |
| **命令字典而非操作脚本** | 列出所有命令和参数，但不规定执行顺序 |
| **Snapshot 模式** | 每个命令返回页面状态快照 + 元素引用 (e.g., `e15`)，Agent 自己决定下一步 |
| **零 workflow 规定** | 没有 "MUST"、"ALWAYS"、"NEVER" 这样的控制性语言 |
| **引用而非内联** | 详细参考放在独立文件中，Skill 本身保持精简 |

#### 核心哲学体现

```
Playwright CLI Skill 的角色：

    ❌ "你必须先 open，再 snapshot，再 click"
    ✅ "这些命令可用：open, snapshot, click, fill, type..."
       "每个命令返回快照，快照中有元素引用"
       "你可以用引用来操作元素"
```

Playwright CLI 把自己定位为一本**命令字典**——Agent 需要什么，翻到对应章节即可。它相信 Agent 有足够的能力根据上下文做出正确决策。

### 1.2 Context7 Skill

**文件**: `plugins/claude/context7/skills/documentation-lookup/SKILL.md`
**来源**: https://github.com/upstash/context7

#### 设计特征

| 特征 | 具体表现 |
|------|---------|
| **精准的激活条件** | description 明确列出框架名（React, Vue, Next.js, Prisma...）提高匹配率 |
| **两步工具链** | 只有 `resolve-library-id` → `query-docs` 两个工具 |
| **4 步引导而非 N 步脚本** | Resolve → Select → Fetch → Use，每步只说"做什么"不说"怎么做" |
| **三层调用模型** | 自动（Skill 自动触发）→ 手动（`/context7:docs`）→ 隔离（spawn agent） |
| **Guidelines 而非 Rules** | "Be specific"、"Prefer official sources" — 是建议，不是命令 |

#### 核心哲学体现

```
Context7 Skill 的角色：

    ❌ "当用户问 React 问题时，你必须先调用 resolve-library-id，
        然后选择 benchmark 分数最高的..."
    ✅ "这里有两个工具：resolve-library-id 和 query-docs"
       "resolve 找到库 ID，query 拿到文档"
       "建议：传完整问题作为 query 效果更好"
```

Context7 把自己定位为一个**文档查找服务**——只负责"给你需要的文档"，不管你怎么用这些文档。

---

## 二、当前 Actionbook Skill 问题诊断

### 2.1 主 Skill (`skills/actionbook/SKILL.md`)

**评价：设计较好，但仍有控制倾向。** 约 170 行，结构清晰。

#### 做得好的地方

- `search` 和 `get` 的两步工具链清晰
- 查询构造指南（query construction）作为 context 提供价值
- Fallback 策略说明合理
- 引用（references/）分离了详细内容

#### 存在的问题

| 问题 | 具体位置 | 分析 |
|------|---------|------|
| **Browser Commands 不属于这个 Skill** | 第 97-136 行 | `search/get` 是数据查询，`browser` 是自动化执行，两者是不同的关注点。混在一起违背单一职责原则 |
| **Examples 过于 prescriptive** | 第 138-158 行 | 示例实际上是一个完整脚本，隐含了 "你应该按这个顺序执行" 的控制意图 |
| **Query construction 过度指导** | 第 43-73 行 | 表格 + 规则 + 示例三重指导，实际上 Agent 完全有能力自己构造 query |

### 2.2 Active Research Skill (`skills/active-research/SKILL.md`)

**评价：严重的 Control 倾向。** 约 620 行，是典型的反面教材。

#### 核心问题

| 问题 | 严重性 | 说明 |
|------|--------|------|
| **硬编码选择器** | 🔴 严重 | 直接在 Skill 中列出 `#terms-0-field`、`#classification-computer_science` 等 40+ 选择器。这些是 `actionbook get` API 应该返回的数据，不应该写死在 Skill 中 |
| **强制性控制语言** | 🔴 严重 | "MUST USE"、"ALWAYS"、"NEVER use WebFetch/WebSearch"、"CRITICAL" — 到处都是命令式约束 |
| **10 步刚性流程** | 🔴 严重 | 从 "Plan Search Strategy" 到 "Close Browser" 规定了 10 个必须按顺序执行的步骤。这不是 Skill，这是脚本 |
| **完整的 JSON 模板** | 🟡 中等 | 60+ 行的 json-ui 模板代码内联在 Skill 中，浪费 context token |
| **中文写作规范** | 🟡 中等 | 80+ 行中文质量规范不属于 Skill 关注点，应放在单独的 reference 中 |
| **arXiv 特化** | 🟡 中等 | 一个通用的 "deep research" skill 却有大量 arXiv 特定逻辑，违反抽象层级 |

#### 数据佐证

```
主 Skill (actionbook):    ~170 行  ← 可接受
Active Research Skill:    ~620 行  ← 远超合理范围
Playwright CLI Skill:     ~120 行  ← 官方标杆
Context7 Skill:           ~50  行  ← 官方标杆
```

#### 最严重的反模式：硬编码选择器

```markdown
# active-research SKILL.md 第 126-131 行：

| arXiv Advanced Search | `arxiv.org:/search/advanced:default` |
  **40+ selectors**: field select, term input, category checkboxes...
| ar5iv paper | `ar5iv.labs.arxiv.org:/html/{paper_id}:default` |
  `h1.ltx_title_document`, `div.ltx_authors`, `div.ltx_abstract`...
```

这完全否定了 Actionbook 的核心价值主张——"Agent 不需要事先知道页面结构，通过 API 实时获取"。把选择器写死在 Skill 中意味着：

1. **选择器过期了 Skill 必须手动更新** — 而 API 返回的数据是由后端维护的
2. **Agent 不再调用 search/get** — 因为答案已经硬编码了，浪费了整套 MCP 工具链
3. **不可扩展** — 新增一个网站就要改 Skill 文件

---

## 三、"Context not Control" 设计哲学详解

### 3.1 定义

| | Context（上下文） | Control（控制） |
|---|---|---|
| **本质** | 信息 | 指令 |
| **角色** | 地图 | GPS 导航 |
| **Agent 的自由度** | 高——Agent 根据信息自行决策 | 低——Agent 按步骤执行 |
| **适应性** | 高——信息本身不依赖特定场景 | 低——脚本只适用于预设场景 |
| **维护成本** | 低——信息变化时更新一处 | 高——每个场景的脚本都要改 |

### 3.2 对照表

| Context（推荐） | Control（避免） |
|---|---|
| "这些命令可用：search, get, browser open/click/fill" | "你必须先 search，再 get，再 open，再 click" |
| "search 返回 area_id，get 返回选择器" | "ALWAYS 先调用 search，MUST 用返回的 area_id 调用 get" |
| "snapshot 提供当前页面的可访问性树" | "当选择器失败时 MUST 用 snapshot 重试" |
| "json-ui 支持这些组件：Section, Prose, Table..." | "你 MUST 使用 BrandHeader 开头、BrandFooter 结尾" |
| "中文写作参考规范见 references/chinese-style.md" | "CRITICAL: 中文 MUST 不是翻译，MUST 原创..." |

### 3.3 为什么 Control 是反模式

1. **Agent 比脚本更聪明**。LLM Agent 可以根据上下文灵活决策，硬编码流程反而限制了它的能力。一个"必须按 10 步执行"的 Skill 在遇到步骤 3 失败时不知道该怎么办，但一个拥有充分 context 的 Agent 可以自主发明替代方案。

2. **Context token 有限**。一个 620 行的 Skill 每次激活都会消耗大量 token，其中大部分是硬编码数据和控制指令。而通过 MCP 工具按需获取的数据只在需要时才占用 token。

3. **违背 Actionbook 自身的价值主张**。Actionbook 的核心价值是"让 Agent 不需要事先学习页面结构"。但 active-research Skill 把 40+ 选择器写死在 Skill 中，等于让 Agent 事先学习了页面结构。

4. **维护负担**。当 arXiv 改版了，需要同时更新 API 数据库 AND Skill 文件。这违背了"单一数据源"原则。

---

## 四、优化方案

### 4.1 总体原则

```
优化前: Skill = 数据 + 工具 + 流程 + 模板 + 规范 (620 行 all-in-one)
优化后: Skill = 能力描述 + 工具引导 + 引用索引 (~80 行)
        References = 模板、规范、详细指南 (按需加载)
        MCP Tools = 数据查询 (按需调用)
```

### 4.2 主 Skill 优化：拆分关注点

**当前状态**: actionbook skill 同时包含 search/get 和 browser 命令

**优化方案**: 保持单一 Skill，但重新组织为"能力字典"模式

```markdown
---
name: actionbook
description: Provides pre-verified website interaction data (selectors, page structure)
  and browser automation commands. Use when the user needs to interact with any website.
---

## Capabilities

### Action Lookup (search → get)
- `actionbook search "<query>"` — find actions matching a task description
- `actionbook get "<area_id>"` — retrieve page structure with CSS selectors
- Results include: selectors, element types, allowed methods, health scores

### Browser Automation
- Navigation: open, goto, back, forward, reload, pages, switch, close
- Interaction: click, fill, type, select, hover, press
- Observation: text, snapshot, screenshot, wait

### Typical Pattern
1. `search` to find relevant actions for the target site
2. `get` to retrieve verified selectors
3. `browser` commands to execute using those selectors
4. `snapshot` as fallback when selectors are outdated

## References
- [command-reference.md](references/command-reference.md)
- [authentication.md](references/authentication.md)
```

**关键变化**:
- 移除 query construction 的详细表格（Agent 有能力自己构造好的 query）
- 将 "Typical Pattern" 从 prescriptive 改为 descriptive
- 移除内联的完整示例脚本
- browser 命令保留但只列出命令名，详细参数在 references 中

### 4.3 Active Research Skill 优化：从脚本到能力

**优化方案**：彻底重写，从 620 行缩减到 ~100 行

```markdown
---
name: active-research
description: Deep research and analysis tool. Generates comprehensive reports
  on any topic using web sources. Use when the user asks to research, analyze,
  investigate, or generate a report.
---

## What This Skill Provides

Multi-source research capability combining:
- **Actionbook search/get** — verified selectors for complex web forms
  (e.g., arXiv Advanced Search with field-specific filtering)
- **Actionbook browser** — navigate, interact, and extract content from any website
- **json-ui** — render structured JSON reports as HTML

## Research Strategy

Actionbook indexes complex web forms that plain web search cannot operate.
Use `actionbook search` to check if a target site has indexed selectors
before browsing — this enables field-specific form interactions
(e.g., searching arXiv by author, title, date range, subject category).

For unindexed sites, `actionbook browser snapshot` provides the live
accessibility tree for selector discovery.

## Report Generation

Write a JSON file following the `@actionbookdev/json-ui` schema, then render:

```bash
npx @actionbookdev/json-ui render report.json -o report.html
```

## References
- [json-ui-components.md](references/json-ui-components.md)  — component catalog
- [chinese-style.md](references/chinese-style.md)            — 中文写作规范
- [arxiv-patterns.md](references/arxiv-patterns.md)          — arXiv 研究模式
```

**关键变化**:

| 删除的内容 | 理由 |
|-----------|------|
| 40+ 硬编码选择器 | 这是 `actionbook get` 返回的数据，不属于 Skill |
| 10 步刚性流程 | Agent 应自行决定研究策略 |
| "MUST USE"、"NEVER use WebFetch" | Context not Control — 提供优势说明，让 Agent 自己选择 |
| 60 行 json-ui JSON 模板 | 移到 references/json-ui-components.md |
| 80 行中文写作规范 | 移到 references/chinese-style.md |
| arXiv 特定的代码示例 | 移到 references/arxiv-patterns.md |

### 4.4 新增 References 文件

将从 Skill 中移出的内容组织为按需加载的参考文件：

```
skills/
├── actionbook/
│   ├── SKILL.md                          # ~80 行，能力描述
│   └── references/
│       ├── command-reference.md           # 已有，保持
│       └── authentication.md             # 已有，保持
└── active-research/
    ├── SKILL.md                          # ~100 行，能力描述
    └── references/
        ├── json-ui-components.md         # 组件目录 + 模板
        ├── chinese-style.md              # 中文写作规范
        └── arxiv-patterns.md             # arXiv 研究模式和常用选择器
```

### 4.5 对照总结

| 维度 | 当前 | 优化后 |
|------|------|--------|
| actionbook SKILL.md | ~170 行（混合关注点） | ~80 行（纯能力描述） |
| active-research SKILL.md | ~620 行（脚本化） | ~100 行（能力描述 + 引用） |
| 硬编码选择器 | 40+ 选择器内联 | 零内联，通过 `actionbook get` 按需获取 |
| 控制性语言 | MUST / ALWAYS / NEVER / CRITICAL | Guidelines / Typically / Consider |
| 详细内容 | 全部内联 | 按需引用 (references/) |
| 流程规定 | 10 步刚性流程 | 模式描述（Agent 自行决策） |
| Token 消耗 | 高（每次激活加载 ~800 行） | 低（~180 行 + 按需引用） |

---

## 五、设计模式对照表

从三个官方/标杆 Skill 中提炼的设计模式：

| 设计模式 | Playwright CLI | Context7 | Actionbook（建议） |
|----------|---------------|----------|-------------------|
| **Skill 长度** | ~120 行 | ~50 行 | 80-100 行 |
| **Description** | 一句话能力边界 | 触发关键词列表 | 一句话 + 关键场景 |
| **工具呈现** | 命令字典 | 两步工具链 | search → get → browser 能力链 |
| **流程指导** | 无 | 4 步引导（非强制） | 模式描述（非强制） |
| **数据内联** | 无 | 无 | 无（通过 API 获取） |
| **详细文档** | 引用 | 独立 agent/command | references/ |
| **控制语言** | 无 | Guidelines | Guidelines |
| **Fallback** | 隐含（Agent 自行处理） | 无 | 模式描述 |

---

## 六、实施优先级

| 优先级 | 任务 | 影响 |
|--------|------|------|
| P0 | 从 active-research 中移除硬编码选择器 | 恢复 Actionbook 核心价值主张 |
| P0 | 将 active-research 的控制性语言改为引导性语言 | 对齐 Context not Control 哲学 |
| P1 | 将 json-ui 模板、中文规范、arXiv 模式移到 references/ | 降低 token 消耗 |
| P1 | 简化 actionbook 主 Skill 的 query construction 部分 | 减少不必要的 Control |
| P2 | 统一两个 Skill 的结构格式 | 一致性 |
| P2 | 考虑 layered invocation（auto + command + agent） | 参考 Context7 的三层模型 |
