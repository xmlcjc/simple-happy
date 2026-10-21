# Simple Happy

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/Rust-1.80%2B-orange)](https://www.rust-lang.org/)
[![Tauri](https://img.shields.io/badge/Tauri-v2-blue)](https://v2.tauri.app/)
[![Vue](https://img.shields.io/badge/Vue-3-42b883)](https://vuejs.org/)

> 一款**本地优先**的桌面客户端：阅读、直播、音乐、RSS 四合一，数据全部保存在自己的设备里，没有后端、没有账号、没有广告。

Simple Happy 基于 Rust + Tauri v2 + Vue 3 构建，支持 Windows、macOS、Linux。核心设计目标是让所有功能都以单机方式运行——书源、直播间、音乐插件、RSS 订阅都是用户自己配置的来源，应用本身不提供、不存储、不分发任何版权内容。

项目源自 [Legado（阅读）](https://github.com/Rimchars/legado) 的桌面移植线 [reader](https://github.com/hectorqin/reader) 与 [reader-rust](https://github.com/givenge/reader-rust)，并在此基础上扩展了四平台直播、MusicFree 音乐插件宿主等模块。详见 [NOTICE.md](NOTICE.md)。

> [!IMPORTANT]
> Simple Happy 不内置任何书籍、直播源、音乐或 RSS 内容。用户需自行导入拥有合法使用权的书源、MusicFree 插件、RSS 订阅与本地文件。

---

## ✨ 功能矩阵

| 模块 | 能力 |
|------|------|
| 📚 **书架** | Legado 书源 + 本地 TXT/EPUB/PDF、书源导入/编辑/测试、多源搜索、章节缓存、主题/排版、WebDAV 同步 |
| 🎬 **直播** | B 站 / 虎牙 / 斗鱼 / 抖音 四平台、分区列表/搜索/房间详情、WebView2 + hls.js / flv.js 播放、实时弹幕 + 飘屏弹幕层、关注列表自动检测开播状态 |
| 🎵 **音乐** | MusicFree CJS 插件宿主（每插件独立 OS 线程 + rquickjs）、搜索/播放/歌词同步、收藏/歌单/播放历史、本地 mp3/flac/m4a 导入（lofty 元数据） |
| 📰 **RSS** | Feed 导入与订阅、文章阅读、与书架最近阅读记录统一时间线 |
| 🛠️ **工具** | 备份/恢复（兼容 Legado ZIP + JSON）、净化规则、替换规则、桌面背景、全局老板键、窗口状态持久化 |

### 书架界面
![书架](desktop/image/1.1.png)

### 阅读界面
![阅读](desktop/image/3.1.png)

### 直播界面（示例）
![直播](desktop/image/5.png)

---

## 🚀 快速开始

1. 前往 [GitHub Releases](https://github.com/xmlcjc/simple-happy/releases) 下载当前系统对应的安装包或便携包。
2. **便携包**首次启动即带有内置书源种子（仅用于快速体验）；**安装包**首次启动若数据目录为空，会自动复制随包分发的种子数据库。
3. 进入「书架 → 书源」查看/管理已导入的书源；在「音乐 → 插件」处导入自己的 MusicFree 插件（如 `xxeMusic.js` 等）。
4. 直播模块无需额外配置，打开顶部「直播」tab 即可浏览 B 站 / 虎牙 / 斗鱼 / 抖音的分区。
5. RSS 订阅在「主页 → RSS 源」中导入。

---

## 🧱 技术栈

| 层 | 技术 |
|----|------|
| **桌面壳** | Tauri v2（各平台原生 WebView：Windows WebView2 / macOS WKWebView / Linux WebKitGTK） |
| **前端** | Vue 3 + TypeScript + Pinia + Vue Router + Vite 8 |
| **媒体** | hls.js / flv.js（直播）、echarts（统计）、pdfjs-dist（PDF）、opencc-js（简繁转换） |
| **本地核心** | Rust + Tokio + SQLx + SQLite + reqwest（rustls）+ rquickjs（书源/音乐插件 JS）+ quick-xml / scraper / jsonpath_lib（规则解析） |
| **直播弹幕** | tokio-tungstenite（WebSocket），Windows 斗鱼弹幕单独 native-tls（TLS 套件原因） |
| **元数据** | lofty（本地音乐 ID3/FLAC 标签）、pdf-extract（PDF 文本提取） |
| **插件宿主** | 每插件独占 OS 线程（Runtime/Context 非 Send）+ 独立 current-thread tokio runtime 做 HTTP 桥（避免 block_on 挂死） |

### 主要目录

| 目录 | 说明 |
|------|------|
| `frontend/` | Vue 3 桌面界面、状态管理、前端单元测试 |
| `src/api/` | Tauri 命令（book / live / music / rss 等）、自定义协议 `reader://` |
| `src/app/` | 应用配置、启动流程、窗口状态持久化 |
| `src/service/` | 书架、直播（四平台适配 + 弹幕）、音乐（插件宿主 + 曲库）、RSS、听书等业务逻辑 |
| `src/parser/` | CSS / JSONPath / XPath / 正则 / JavaScript 五合一解析引擎 |
| `src/crawler/` | HTTP 客户端、请求头规则、Cookie 存储 |
| `src/storage/` | SQLite + sqlx 嵌入迁移（0001~0011）、文件缓存、数据目录 |
| `desktop/src-tauri/` | Tauri 入口、托盘、单实例、NSIS 安装脚本、平台适配 |
| `scripts/` | 桌面端构建脚本 |
| `vendor/` | 本地补丁过的第三方依赖（`pdf-extract` 补 GBK CMap、`tao` 修 Windows 全屏黑边） |
| `seed/` | 便携包使用的书源/RSS 种子数据库（不入库，仅 Release 打包时注入） |

---

## 🏗️ 构建与开发

环境要求：**Node.js 22+**、**Rust 1.80+**、Tauri v2 对应平台的 WebView 运行时。

```bash
# 前端
cd frontend
npm install
npm run dev       # Vite 热更新
npm run build     # 生产构建
npm test          # vitest 单元测试

# 桌面端（Windows）
cd desktop
npm install
npm run dev       # 同时启动 Vite + Tauri 窗口
npm run build     # release 编译 + NSIS 安装包 + 便携包
```

Windows 本地执行前建议：

```powershell
. D:\build\env.ps1       # 设置 JAVA_HOME / PATH
$env:CL = "/utf-8"       # rustc 用 UTF-8 输出
$env:CARGO_BUILD_JOBS = "4"  # 避免杀毒软件并行 race
```

产物位于 `desktop/dist/`：
- `<productName>_<ver>_x64-setup.exe` —— NSIS 安装包
- `simple-happy-portable-v<ver>-win-x64.zip` —— 便携包（含种子数据库）

### 测试

```bash
cargo test -j 4                   # Rust 库 400+ 单测
cargo test -p reader-desktop      # 桌面端便携目录逻辑
cargo test --test music_plugin    # 音乐插件宿主集成测试（axum mock upstream）
cd frontend && npm test           # 前端 600+ vitest 用例
```

---

## 📄 开源依赖声明

Simple Happy 基于若干 MIT / Apache-2.0 / BSD / ISC 许可的开源项目构建。完整的第三方依赖清单与许可证类型见 [LICENSE-THIRD-PARTY.md](LICENSE-THIRD-PARTY.md)。

重要的二次开发来源：
- **Legado（阅读）** —— 书源格式、书架数据结构、RSS、备份兼容（MIT）
- **reader / reader-rust** —— Tauri + Rust + Vue 的桌面移植骨架（MIT）
- **MusicFree** —— 音乐插件协议 `module.exports = { platform, search, getMediaSource, getLyric }`，Simple Happy 为其实现了独立的 Rust 宿主（MIT）

详见 [NOTICE.md](NOTICE.md)。

---

## 🔒 隐私与合规

- **纯单机**：没有账号系统、没有服务端、没有遥测。书架、阅读进度、书源配置、音乐收藏全部保存在本机 SQLite + 文件目录。
- **用户即来源**：书源、音乐插件、RSS 订阅均由用户自行配置和导入；应用不内置、不推荐、不维护任何第三方来源。
- **网络直连**：书源搜索、直播流、音乐源、RSS 都是桌面端直接请求对应来源，走用户自己的系统代理。
- **合规提醒**：请确保自行导入的书源、音乐插件、RSS、本地文件以及通过本应用访问的内容**均已获得合法授权**，版权与合规责任由用户自行承担。如任何权利人认为本项目相关内容侵犯了其合法权益，请通过 Issues 联系维护者。

---

## 🧑‍🤝‍🧑 参与项目

欢迎提 Issues、PR。提交前请：

1. 跑全部门禁：`cargo test` + `npm test` + `npm run build`。
2. 新增依赖时同步更新 `LICENSE-THIRD-PARTY.md`。
3. 不要在 issue / PR 中贴出 Cookie、Token、私人书源或包含个人数据的备份文件。

---

## 📄 许可

本项目基于 [MIT License](LICENSE) 开源。
