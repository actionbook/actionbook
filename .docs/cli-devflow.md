# Actionbook CLI — 标准功能开发工作流

> 源自 snapshot transform (PR #348) 和 cursor detection (PR #353) 的实践总结。
>
> **SPEC**: [`packages/cli-v2/docs/api-reference.md`](../packages/cli-v2/docs/api-reference.md) — 所有命令的 JSON/Text 输出合约、参数定义、错误码均以此为准。

---

分支， 基于 release/1.0.0 ， 合并也是到这个

## Phase 0: 分析对齐

- 读 PRD / SPEC（`packages/cli-v2/docs/api-reference.md`）确认合约
- 对比参考实现（agent-browser / actionbook-cli-rs）分析差异、优劣: 代码在 
  - agent-browser: /Users/junliangfeng/Documents/code/next/thirds/browser-cli/agent-browser
  - actionbook-cli-rs /Users/junliangfeng/Documents/code/next/thirds/browser-cli/actionbook-cli-rs
- 和用户确认方案，输出简洁 plan
- **输出**: 明确的修改文件列表 + 关键设计决策

## Phase 1: 写测试（TDD）

- **E2E 测试**（`tests/e2e/snapshot.rs`）：基于 SPEC 定义端到端行为
  - JSON 信封结构
  - Text 输出格式
  - Error path（SESSION_NOT_FOUND, TAB_NOT_FOUND 等）
  - Flag 组合（--interactive --compact, --cursor 等）
- **UT 合约测试**（`src/browser/observation/snapshot_transform.rs`）：
  - 定义核心函数的输入 → 输出契约
  - 当前实现应 **fail**（TDD red phase）

### 断言三原则

1. **验证值，不只验证类型** — `context.url` 包含目标 domain，不只是 `is_string()`
2. **严格白名单** — `data` keys 精确匹配 spec（`format`, `content`, `nodes`, `stats`），禁止多余字段
3. **否定基线** — 无 flag 时无 ref → 有 flag 时有 ref，形成对照组

### 验收标准

- 编译通过
- 合约测试预期失败，非合约测试全部通过

## Phase 2: 测试 review


```
code-reviewer agent （基于 /code-review:code-review） ←  审查逻辑正确性、覆盖度、convention,对抗性审查，找 edge case、弱断言、合约漏洞
```


### 重点检查

- Fixture 是否真的测了目标逻辑（不是碰巧通过）
- Edge case 覆盖（空输入、重复、组合 flag、已有 ARIA role 重叠）
- 断言强度（`assert_eq` 比内容，不只比 `len()`）

### 输出

- 修复所有发现直到测试合约正确
- 合约测试仍然 fail（red phase）

## Phase 3: 实现

实现代码使所有测试通过。

### 验收标准

```bash
cargo fmt -- --check             # 格式
cargo clippy --all-targets -- -D warnings  # lint（注意 --all-targets 包括测试）
cargo test --lib                 # 所有 UT 通过
```

## Phase 4: 双 Review agent 实现

并行启动两个 reviewer
  - claude code: 基于 /code-review:code-review
  - code cli: 启动做技术 review。


### 典型问题类型

| 类型 | 示例 |
|------|------|
| Filter 顺序 | `--interactive --cursor` 组合丢节点 |
| Error path 清理 | DOM 属性未在错误路径清理 |
| Silent failure | `.ok()` 吞掉错误无提示 |
| 死代码 | 函数存在但从未调用 |
| 安全 | 名字未转义导致树注入 |

### 输出

- 修复所有发现
- 67+ UT 全通过，clippy clean

## Phase 5: 真实浏览器验证

**UT 无法替代此步骤。**

```bash
cargo build --release
# 杀旧 daemon
kill $(ps aux | grep 'actionbook __daemon' | grep -v grep | awk '{print $2}')
# 启新 daemon + 测试
./target/release/actionbook browser start --headless --open-url https://example.com --json
./target/release/actionbook browser snapshot --session <SID> --tab <TID> --json | jq
./target/release/actionbook browser snapshot --session <SID> --tab <TID>  # text 模式
```

### 对比验证

- 和 agent-browser 对比同一页面输出
- 检查：StaticText 是否保留、refs 是否 e1-based、url/title 是否正确

### 发现的 Bug → 立即补测试

每个在浏览器验证中发现的 bug，必须：
1. 修复代码
2. 添加对应 E2E/UT 测试（防止回归）
3. 反思为什么之前的测试没抓到

## Phase 6: PR → CI → Review Comments

```bash
git push -u origin <branch>
gh pr create --base release/1.0.0 --title "..." --body "..."
```

### CI 配置

- **PR**: 只跑 Lint + Unit Tests（快速反馈）
- **Merge 到 main/release**: 跑 Lint + Unit Tests + E2E（完整验证）

### PR Review Comments

提交 PR 后启动定时轮询，直到 CI 全绿 + review comments 全部处理：

```bash
# 定时检查 CI 状态 + review comments（每 2 分钟一次，直到全部解决）
while true; do
  echo "=== CI Status ==="
  gh pr checks <PR_NUMBER>

  echo "=== Review Comments ==="
  gh api repos/{owner}/{repo}/pulls/<PR_NUMBER>/comments \
    --jq '.[] | "\(.id) [\(.user.login)] \(.path):\(.line // "N/A") — \(.body[:80])"'

  echo "=== Pending Reviews ==="
  gh pr view <PR_NUMBER> --json reviews \
    --jq '.reviews[] | select(.state != "COMMENTED" and .state != "APPROVED") | "\(.author.login): \(.state)"'

  sleep 120
done
```

处理流程：

1. **CI 失败** — 读日志定位原因 → 修复 → commit → push
2. **有新 review comment**：
   - **修复的**: 修改代码 → commit → push → 在 comment 下回复修复内容和 commit hash
   - **不修复的**: 在 comment 下回复原因（如"有意为之"、"不在本 PR 范围"等）
3. **每次 push 后重新等 CI** — 确认修复没有引入回归
4. **全部条件满足后合并**: CI 全绿 + review comments 全部回复 + 无 blocking review

---

## 核心原则

1. **测试先行，review 先于实现** — Phase 1-2 在 Phase 3 之前
2. **每个 bug 修复必须伴随测试** — 无测试的修复等于没修
3. **双 review 不可省略** — 两个独立视角互补，单一 review 覆盖率不足
4. **真实浏览器验证不可用 UT 替代** — CDP 行为、daemon 生命周期、格式化路径只有 E2E 能覆盖

---

## 教训记录

| 教训 | 来源 |
|------|------|
| `is_string()` 不够，要验证值 | snapshot url=chrome://newtab/ |
| data 要白名单，不能有多余字段 | __ctx_url 泄漏到 data |
| text formatter 要有对应分支 | snapshot text 无 body |
| RefCache::default() 和 ::new() 要一致 | ref 从 e0 开始 |
| TAB_NOT_FOUND context 要 null tab_id | E2E 测到 |
| interactive filter 和 cursor 的顺序 | codex challenge 发现 |
| DOM 属性清理要用 nonce | PR review comment |
| compact 顺序影响 depth gap | code-reviewer 发现 |
| 非默认 profile close 时删、默认 profile 保留 | codex-connector review comment |
| registry lock 内不做慢 I/O（cdp.close、child.wait） | codex challenge 发现 |
| kill_and_reap_option 要 take() 防 Drop 双杀 | codex challenge 发现 |
