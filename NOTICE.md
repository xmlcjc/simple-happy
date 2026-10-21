# NOTICE — 二次开发来源声明

Simple Happy v1.0.0 基于以下开源项目进行二次开发。本文件列出各上游项目的名称、仓库地址、许可证类型与 Simple Happy 对其的使用方式。

所有上游项目的许可证条款均已在各自仓库中发布，Simple Happy 在此遵守其许可条款。

---

## 1. Legado（阅读）

- **仓库**：[Rimchars/legado](https://github.com/Rimchars/legado)（原 [gedoor/legado](https://github.com/gedoor/legado)）
- **许可证**：MIT License
- **使用方式**：
  - **书源格式**：Simple Happy 的书源 JSON 结构（`bookSourceUrl` / `bookSourceName` / `searchUrl` / `ruleSearch` / `ruleBookInfo` / `ruleToc` / `ruleContent` 等）完全兼容 Legado。
  - **规则语法**：`@css:` / `@json:` / `@xpath:` / `@regex:` / `js:` 五种规则前缀语义与 Legado 保持一致，JS 规则由本地 rquickjs 引擎执行。
  - **备份格式**：ZIP 备份与 Legado 互通，书架、书源、RSS、书签、净化规则与阅读进度可双向恢复。
  - **阅读体验**：主题系统、排版选项等交互参考 Legado。

## 2. reader

- **仓库**：[hectorqin/reader](https://github.com/hectorqin/reader)
- **许可证**：MIT License
- **使用方式**：
  - 桌面端 UI 骨架（Vue 3 + Tauri）、状态管理结构、书源管理界面的交互模式。

## 3. reader-rust

- **仓库**：[givenge/reader-rust](https://github.com/givenge/reader-rust)
- **许可证**：MIT License
- **使用方式**：
  - Rust 本地核心的整体架构（API 层 / Service 层 / Parser 层 / Storage 层 / Crawler 层分离）。
  - SQLx 嵌入迁移机制、自定义 `reader://` 协议、Tauri IPC 命令组织。
  - 书源规则解析引擎（CSS / JSONPath / XPath / 正则 / JavaScript）的基础实现。

## 4. MusicFree

- **仓库**：[maotoumao/MusicFreeDesktop](https://github.com/maotoumao/MusicFreeDesktop)
- **许可证**：MIT License
- **使用方式**：
  - **插件协议**：Simple Happy 的音乐插件宿主兼容 MusicFree 的 CommonJS 插件协议：
    ```js
    module.exports = {
      platform: '平台名',
      version: '1.0',
      supportedSearchType: ['song', 'singer', 'album', 'playlist'],
      async search(query, page, type) { ... },
      async getMediaSource(item, quality) { ... },
      async getLyric(item) { ... },
    };
    ```
  - 宿主实现：每插件独占一个 OS 线程（rquickjs Runtime/Context 非 Send），独立 current-thread tokio runtime 做 HTTP 桥，避免与主循环 runtime 复用导致 block_on 挂死。
  - 用户自行导入的具体插件版权由插件作者决定，使用前请自查。

---

## 本地补丁过的第三方依赖（vendor/）

| 路径 | 上游 | 补丁说明 |
|------|------|---------|
| `vendor/pdf-extract` | [jrmuizel/pdf-extract](https://github.com/jrmuizel/pdf-extract) | 0.12.0 遇到无 ToUnicode 的 GBK-EUC-H 等 Type0 编码会 panic，补了 GBK/Big5 系 CMap 的近似解码（恒等字节映射 + encoding_rs 逐码位解码当伪 ToUnicode 表） |
| `vendor/tao` | [tauri-apps/tao](https://github.com/tauri-apps/tao) | tao 0.35.3 在 Windows 上有「最大化后进无边框全屏会露出任务栏高度黑边」bug，上游 tauri-apps/tao#1353 已合并但未随任何 release 发布；补丁只改了 `platform_impl/windows/event_loop.rs` 里 WM_NCCALCSIZE 的一行 |

两个 vendor 补丁均保持上游许可证条款（`pdf-extract` MIT、`tao` MIT）。

---

## 直播模块的上游技术

| 模块 | 来源 | 说明 |
|------|------|------|
| 斗鱼签名 JS | 提取自 Legado 生态的 simple_live 项目 | 运行 rquickjs 执行混淆 JS 算 `ub98484234(rid,did,tt)` 签名 |
| 抖音 a_bogus / wbi / webmssdk | 提取自 Legado 生态的 simple_live 项目 | a_bogus 走 SM3 混淆 JS（rquickjs strict=false），wbi 带 wts_mid_key 签名，webmssdk 生成弹幕 WS 所需的 frontierSign |
| hls.js / flv.js | npm 依赖（MIT） | WebView2 内直接播放 m3u8 / flv |
| tokio-tungstenite | Rust 依赖（MIT/Apache-2.0） | 直播弹幕 WebSocket 长连接 |
| native-tls | Rust 依赖（MIT/Apache-2.0） | Windows 斗鱼弹幕 TLS 套件不兼容 rustls，单独用 Schannel |

---

## 致谢

感谢以上开源项目的维护者和贡献者，没有他们的工作 Simple Happy 无法在相对短的时间内从零搭起。也感谢 Legado/MusicFree 社区中维护插件与书源的朋友们，让用户的实际使用体验成为可能。
