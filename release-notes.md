# Simple Happy v1.2.0

插件生态版本 —— 音乐模块补齐 MusicFree 的插件导入方式，并打通第三方插件的运行时依赖。

## 🎯 本版本亮点

### 🎵 插件导入（对齐 MusicFree）
- **从网络安装**：粘贴 `.js` 直链或 `.json` 订阅清单（`{plugins:[{url}]}`），批量安装并逐条汇总结果
- **插件订阅源**：保存多个订阅地址，一键更新全部；`.js` / `.json` 后缀校验与去重
- **失败可见**：安装/更新失败会列出每个地址与原因，不再只弹一条错误
- **兼容性校验**：解析插件的 `appVersion`，按 npm semver 常用区间校验宿主版本，不兼容时给出可读原因
- **列表管理**：插件排序（上移/下移）、卸载全部（保留订阅）、复制插件地址、查看插件说明
- **本地批量安装**：文件选择支持多选

### 🔌 第三方插件运行时兼容
- 内置 MusicFree 注入的常用库：`axios`、`crypto-js`（MD5 / SHA256 / HmacSHA256 / Base64）、`qs`、`he`、`dayjs`、`big-integer`
- 修正定时器调度：插件用 `Promise.race([请求, 超时])` 取链时不再被超时抢先判失败
- axios 响应补 `request.responseURL`，修复读取重定向地址的插件

### ✅ 实测结果

对第三方插件站（hebijunge，57 个插件）全量验证：

| 项目 | 结果 |
|---|---|
| 订阅清单安装 | 57 / 57 成功 |
| 搜索（QQ / 酷我 / 网易云 / 酷狗） | 正常返回结果 |
| 取流（128k） | 酷我 4/4、网易云 4/4、QQ 3/4（失败项为版权限制） |

> 音乐数据来自用户自行订阅的第三方插件；本项目不内置、不存储、不分发音源。

## 📦 产物

| 文件 | 说明 |
|------|------|
| `Simple Happy_1.2.0_x64-setup.exe` | NSIS 安装包（首次启动自动写入书源种子） |
| `simple-happy-portable-v1.2.0-win-x64.zip` | 便携包（解压即运行，自带 `data/reader.db` 种子） |

## 🔧 使用第三方音源

1. 打开「音乐 → 插件 → 订阅」
2. 添加订阅地址（例如 `https://hebijunge.github.io/musicfree-plugins/plugins.json`）
3. 点「更新全部订阅」，等待插件安装完成
4. 回到搜索页选择平台，即可搜索与播放

## 📝 许可与依赖

- **本项目**：MIT License
- **上游二次开发来源**：Legado（MIT）、reader（MIT）、reader-rust（MIT）、MusicFree（MIT）。详见 [NOTICE.md](NOTICE.md)

## 🔧 已知限制

1. **AES 加密的插件接口暂不支持**：crypto-js 目前实现 MD5 / SHA256 / HmacSHA256，个别插件若使用 AES 会给出明确报错。
2. **插件更新仅支持 .js / .json**：返回其它格式的更新地址会提示无法解析。
3. **抖音搜索**：2026 年起强制登录（status_code=2483），匿名搜索返回可读错误提示。

## 🙏 致谢

感谢 Legado / reader / reader-rust / MusicFree / simple_live 以及所有 Rust / Tauri / Vue 开源生态贡献者。
