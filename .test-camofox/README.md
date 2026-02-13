# Camoufox Integration Testing

这个目录包含 Camoufox 集成的测试资源和脚本。

## 快速开始

### 1. 启动 Camoufox 服务器（终端1）

```bash
cd .test-camofox
./start-server.sh
```

保持这个终端运行。

### 2. 运行快速测试（终端2）

```bash
cd .test-camofox
./quick-test.sh
```

这会测试：
- ✓ 服务器连接
- ✓ actionbook-rs 编译
- ✓ 后端选择
- ✓ 单元测试
- ✓ REST API 调用

## 文件说明

| 文件 | 说明 |
|------|------|
| `start-server.sh` | 启动 Camoufox 服务器（端口 9377） |
| `quick-test.sh` | 快速测试脚本（验证核心功能） |
| `TEST_GUIDE.md` | 完整测试指南（详细步骤） |
| `package.json` | npm 依赖配置 |

## 测试场景

### 场景1: 基础功能测试（当前可用）

```bash
# 1. 启动服务器
./start-server.sh

# 2. 运行快速测试
./quick-test.sh
```

### 场景2: 手动 API 测试

```bash
# 创建标签页
curl -X POST http://localhost:9377/tabs \
  -H "Content-Type: application/json" \
  -d '{"userId":"test","sessionKey":"test","url":"https://example.com"}'

# 获取快照
curl http://localhost:9377/tabs/<tab-id>/snapshot | jq .
```

### 场景3: CLI 测试（Phase 4 后可用）

```bash
cd ../packages/actionbook-rs

# 使用 --camofox 标志
cargo run -- --camofox browser open https://example.com

# 使用配置文件
cargo run -- --profile test-camofox browser open https://example.com
```

## 当前实现状态

### ✅ 已完成
- Phase 1: 基础设施（类型、客户端、会话、快照解析）
- Phase 2: 路由器（BrowserDriver 多后端支持）

### ⏳ 待实现
- Phase 3: 选择器解析增强
- Phase 4: 命令集成（browser.rs 路由）
- Phase 5: 完整测试和验证

## 配置示例

创建测试配置：

```bash
mkdir -p ~/.actionbook
cat > ~/.actionbook/config.toml <<'EOF'
[browser]
backend = "camofox"

[browser.camofox]
port = 9377
user_id = "test-user"
session_key = "default"

[profiles.camofox-test]
backend = "camofox"
EOF
```

## 故障排查

### 问题: 服务器无法启动

```bash
# 检查端口占用
lsof -i :9377

# 杀死占用进程
kill $(lsof -Pi :9377 -sTCP:LISTEN -t)
```

### 问题: 编译错误

```bash
cd ../packages/actionbook-rs
cargo clean
cargo build --release
```

### 问题: 测试失败

```bash
# 运行详细测试
cd ../packages/actionbook-rs
cargo test --lib camofox -- --nocapture

# 运行集成测试（需要服务器运行）
cargo test --lib camofox -- --ignored --nocapture
```

## 下一步

1. **完成 Phase 3-4**: 集成 BrowserDriver 到命令处理
2. **测试所有命令**: goto, click, type, screenshot
3. **反爬虫验证**: 测试 bot.sannysoft.com, CreepJS
4. **性能测试**: 对比 CDP vs Camoufox 延迟

## 有用的命令

```bash
# 查看 actionbook 配置
cat ~/.actionbook/config.toml

# 检查编译
cd ../packages/actionbook-rs && cargo check

# 运行特定测试
cargo test --lib camofox::client

# 查看详细日志
RUST_LOG=debug cargo run -- --camofox browser open https://example.com
```

## 参考资料

- [完整测试指南](./TEST_GUIDE.md)
- [Camoufox 文档](https://github.com/daijro/camoufox)
- [实现计划](../.docs/camofox-integration-phase1-summary.md)
