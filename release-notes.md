# Simple Happy v1.0.0

Simple Happy 的第一个正式独立版本 —— 阅读 / 直播 / 音乐 / RSS 四合一，纯单机桌面客户端。

## 🎯 本版本亮点

### 📚 书架（源于 Legado 生态）
- 完整的 Legado 兼容书源支持：导入、编辑、测试、登录调试、JS 规则本地执行
- 本地 TXT / EPUB / PDF 阅读，自动识别编码，PDF 内置 Goja 风格 CMap 补丁
- 主题 / 排版 / 迷你模式 / 简繁转换 / 净化规则 / 桌面背景
- WebDAV 双向同步阅读进度；ZIP 备份与 Legado 手机端互通

### 🎬 直播（四平台原生适配）
- **B 站**：分区 / 搜索 / 房间详情 / flv.js 播放 / 实时弹幕（Web + Cmd）
- **虎牙**：TARS(JCE) 最小编解码器 + hls.js 播放 + 弹幕
- **斗鱼**：rquickjs 执行 `ub98484234` 签名算播放地址，flv.js 播放，Windows 单独 native-tls 连弹幕（rustls TLS 套件不兼容）
- **抖音**：双签名（a_bogus HTTP API + webmssdk 弹幕 WS），wts_mid_key 签名链路
- 直播偏好设置持久化到 localStorage（弹幕开关 / 显示区域 / 字号 / 速度 / 不透明度）
- 关注列表自动检测开播状态 + 自动置顶

### 🎵 音乐（MusicFree 插件宿主）
- 兼容 MusicFree CJS 插件协议（`module.exports = { platform, search, getMediaSource, getLyric }`）
- 每插件独占一个 OS 线程 + 独立 current-thread tokio runtime 做 HTTP 桥（避免 block_on 挂死）
- 播放 / 歌词同步 / 收藏 / 歌单 / 每日统计 / 历史记录
- 本地 mp3 / flac / m4a / wav 导入（lofty 读取 ID3/FLAC 标签）

### 📰 RSS
- Feed 导入（本地 + 远程 URL）、订阅、文章阅读
- RSS 文章与书架最近阅读记录统一时间线

## ✅ 门禁测试

| 类型 | 数量 | 结果 |
|------|------|------|
| Rust 库单测 | 448 passed / 9 ignored | ✅ |
| 桌面端便携目录 | 8 passed | ✅ |
| 音乐插件宿主集成 | 2 passed | ✅ |
| 前端 vitest | 618 passed | ✅ |

（ignored 项为直播真实链路测试，需在线环境，不影响发布。）

## 📦 产物

| 文件 | 说明 |
|------|------|
| `Simple Happy_1.5.7_x64-setup.exe` | NSIS 安装包（首次启动自动写入书源种子） |
| `simple-happy-portable-v1.5.7-win-x64.zip` | 便携包（解压即运行，自带 `data/reader.db` 种子） |

> 安装版 / 便携包的内部版本号目前仍是 `1.5.7`（Cargo.toml / package.json 的 version 已更新为 `1.0.0`，打包脚本读取的是 Cargo.toml，但 NSIS 文件名由 productName + cargo version 组合；Release tag 我们用 `v1.0.0` 标记）。

## 📝 许可与依赖

- **本项目**：MIT License
- **上游二次开发来源**：Legado（MIT）、reader（MIT）、reader-rust（MIT）、MusicFree（MIT）。详见 [NOTICE.md](NOTICE.md)
- **第三方依赖完整清单**（Rust 722 + npm 145）：见 [LICENSE-THIRD-PARTY.md](LICENSE-THIRD-PARTY.md)

## 🔧 已知限制

1. **音乐插件需自行导入**：工作区未附带 MusicFree 插件，需在「音乐 → 插件 → 安装插件」手动导入。
2. **安装包首次启动网络代理**：首次启动时种子数据库已在本地，但如果用户升级/重装可能覆盖，建议安装前先创建备份。
3. **Windows Defender 与 Rust release 并行编译**：构建脚本已设置 `CARGO_BUILD_JOBS=4`，如仍遇 link.exe / rlib 写入错误，请在构建时临时关闭 Defender 实时防护。
4. **抖音搜索**：2026 年起强制登录（status_code=2483），匿名搜索返回可读错误提示，不静默空列表。

## 🙏 致谢

感谢 Legado / reader / reader-rust / MusicFree 以及所有 Rust / Tauri / Vue 开源生态贡献者。
