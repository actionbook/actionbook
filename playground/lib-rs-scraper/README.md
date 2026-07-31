# lib.rs Scraper

爬取 [lib.rs](https://lib.rs) 网站的 Rust crate 信息。

**选择器来源**: [Actionbook](https://actionbook.app) 验证数据

## 功能

| 命令 | 描述 | 输出文件 |
|------|------|----------|
| `homepage` | 爬取首页所有分类 | `homepage.json` |
| `category <url>` | 爬取分类下的 crate 列表 | `category-{name}.json` |
| `detail <name>` | 爬取单个 crate 详情 | `crate-{name}.json` |
| `batch <file>` | 批量爬取多个 crate | `batch-results.json` |

## 安装

```bash
cd playground/lib-rs-scraper
pnpm install
npx playwright install chromium
```

## 使用

### 1. 爬取首页分类

```bash
node scraper.js homepage
```

输出 (`homepage.json`):

```json
{
  "totalCrates": 217184,
  "categories": [
    {
      "title": "Rust patterns",
      "url": "https://lib.rs/rust-patterns",
      "description": "Shared solutions for particular situations...",
      "totalCrates": 6794,
      "featuredCrates": [
        { "name": "bitflags", "url": "/crates/bitflags", "description": "..." }
      ]
    }
  ],
  "scrapedAt": "2026-01-23T..."
}
```

### 2. 爬取分类页

```bash
# 默认爬取前 10 页
node scraper.js category https://lib.rs/rust-patterns

# 指定页数
node scraper.js category https://lib.rs/async 5
```

输出 (`category-rust-patterns.json`):

```json
[
  {
    "name": "bitflags",
    "url": "https://lib.rs/crates/bitflags",
    "description": "A macro to generate structures which behave like bitflags",
    "version": "2.10.0",
    "stable": true,
    "downloads": 41410871,
    "labels": ["no-std"],
    "keywords": ["bitmask", "flags"]
  }
]
```

### 3. 爬取 crate 详情

```bash
node scraper.js detail tokio
```

输出 (`crate-tokio.json`):

```json
{
  "name": "tokio",
  "description": "An event-driven, non-blocking I/O platform...",
  "labels": [],
  "latestVersion": "1.43.0",
  "publishDate": "Jan 15, 2025",
  "versions": [
    { "version": "1.43.0", "date": "Jan 15, 2025" }
  ],
  "downloads": 89000000,
  "categoryRanking": 1,
  "dependentCrates": 45000,
  "license": "MIT",
  "packageSize": "1.2MB",
  "sloc": "~50000 SLoC",
  "authors": ["Carl Lerche", "..."],
  "categories": ["Asynchronous", "Network programming"],
  "keywords": ["async", "futures", "io", "non-blocking"],
  "links": {
    "librs": "https://lib.rs/crates/tokio",
    "apiDocs": "https://docs.rs/tokio",
    "github": "https://github.com/tokio-rs/tokio"
  },
  "dependencies": [
    { "name": "bytes", "version": "^1.0.0", "optional": false }
  ],
  "devDependencies": [...],
  "features": ["full", "rt", "rt-multi-thread", "net", "io-util"],
  "readmeHtml": "...",
  "scrapedAt": "2026-01-23T..."
}
```

### 4. 批量爬取

创建 `crates-to-scrape.json`:

```json
["tokio", "serde", "anyhow", "thiserror", "tracing"]
```

运行:

```bash
node scraper.js batch crates-to-scrape.json
```

## NPM Scripts

```bash
pnpm run scrape:homepage    # 爬取首页
pnpm run scrape:category    # 爬取分类 (需要参数)
pnpm run scrape:detail      # 爬取详情 (需要参数)
pnpm run scrape:batch       # 批量爬取 (需要参数)
```

## 注意事项

- 请遵守 lib.rs 的使用条款
- 爬虫内置 1-1.5 秒的请求间隔
- 批量爬取时建议分批进行，避免过大压力

## 选择器参考

所有选择器来自 Actionbook，已验证可用：

- 首页: `https://lib.rs/` (Action ID)
- 分类页: `https://lib.rs/{category}`
- 详情页: `https://lib.rs/crates/{crate_name}`
