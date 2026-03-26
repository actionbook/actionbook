
## 核心原则

### 产品原则

- 无状态接口，有状态运行时
CLI 面向 agent 的接口是完全无状态的——每条命令都通过 --session 和 --tab 显式寻址，自包含，不依赖任何前置命令的副作用。但 daemon 本身是有状态的，负责持有 CDP 连接池和 session/tab 注册表。关键区分是：agent 不需要追踪任何状态，daemon 替它管着。
- 绝对路径式寻址，类比文件系统
文档中的核心类比：人类在 IDE 里先打开文件再编辑，agent 直接用 write(full_path, content)。所有 per-tab 命令都必须带 --session + --tab，缺了就报错，没有隐式的"当前 tab"概念。这消除了全局锁，也让多 tab 并行操作成为一等公民。
- 为 LLM 消费者而非人类设计
输出默认是紧凑文本而非 JSON，因为 LLM 读 token 更高效。每条响应都带 [session tab] url 前缀，让 agent 始终知道上下文。短 ID（s0、t3）代替 UUID，把寻址从 40+ token 压缩到 3-4 token。
- 错误即引导
每个错误响应都包含 hint 字段，告诉 agent 下一步该做什么。比如 SESSION_NOT_FOUND 会提示 run browser launch。这减少了 agent 在错误恢复上浪费的 token。
- 类型化协议，一次做对
v2 用类型化的 Action 枚举替代 v1 的 raw CDP 透传（serde_json::Value），每个 action 都有明确的参数和返回类型。协议带版本号，v1/v2 在同一个 UDS socket 上共存，渐进迁移。
- 干净断裂，不背包袱
不做向后兼容。browser switch、session active、active_page_id 全部删除。在早期开源项目阶段，设计正确性优先于保护存量用户。


### 架构原则


- Harness:
    - 将记忆“物理化”（对应持久化记忆）：不要指望 Agent 记住之前的规划。
        - 有用的原则：设计约束：架构约束，系统约束，沉淀到文档化的知识体系中，作为 SystemPrompt(Memory)包括产品和设计，整理到 CLAUDE.md / AGENT.md 中
        - 必须强制 Agent 把当前进度、功能列表（Feature List）和架构准则写进代码仓库的文件里（如 progress.txt, features.json）。每次开新局，Agent 第一步就是读这些物理文件来恢复记忆。
    - 将规范“机械化/多角色化”（对应严格的架构约束）：不要在 Prompt 里哀求 Agent 遵循代码风格。要把架构约束写成 CI/CD 脚本和死板的 Linters，用编译器去无情地拒绝它；或者引入独立的“评估者 Agent（Evaluator）”，充当严厉的 QA，用多智能体对抗来倒逼质量。
        - 确定性的规则约束：自定义 linter、CI、结构化约束（比如依赖顺序）
        - 测试约束
        - 多 Agent: 不同角色分工，独立交叉配合（设计、开发、Review）
    - 将验证“工具化”（对应客观的闭环验证）：提供测试及环境。强迫它一次只做一个小功能（Incremental Progress），跑通了才能进行下一步
        - 可执行和验证的环境：本地，远程，etc
- 模块的依赖顺序约束：禁止依赖翻转，上层模块依赖底层模块，不能产生反向的依赖
- TDD: 测试驱动。每个 task
    design-review -> TEST Dev -> DEV and Test Paas -> Review(Local code review -> PR Review)
    - design-review
    - Dev
    - Review:
        - /code-review: claude skills
        - codex-cli-review: 

### 开发

- 核心 Review 流程
    - 根据 PRD review, 判断是否本次 PRD 修改范围
    - 根据核心原则 review，判断是否满足设计原则
- TDD: 严格按测试驱动，保证最终结果。

## 历史的问题
- deamon-mode 和 no-deamon-mode 并存：预期默认都是 daemon-mode
- session 管理：远程 session/本地 session 状态管理问题
- 