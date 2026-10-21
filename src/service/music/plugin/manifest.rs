//! 从插件源码中解析 MusicFree 插件元信息。
//!
//! 插件是 CommonJS 模块, 安装时只需要读取 `module.exports` 上的几个属性。
//! 为了不引入额外 JS 引擎, 直接复用项目已有的 rquickjs: 在独立 Runtime 中
//! 执行一次包装后的脚本, 再把元信息以 JSON 字符串带回 Rust。
//!
//! 注意: 这里**不做**完整的运行时垫片(见 `shim.rs`)。安装阶段只要求脚本
//! 能被求值到 `module.exports` 赋值完成; 若插件在顶层就 `require` Node
//! 模块, 会得到明确的「依赖 Node 模块」错误, 而不是静默失败。

use rquickjs::context::EvalOptions;
use rquickjs::{Context, Runtime};
use serde_json::Value;

use crate::error::error::AppError;

use crate::service::music::model::PluginManifest;

/// 单个插件脚本上限 5MB, 与计划一致。
pub const MAX_PLUGIN_BYTES: usize = 5 * 1024 * 1024;

/// 安装阶段使用的轻量全局垫片。只覆盖插件在**加载期**常见的全局变量,
/// 避免因 `global`/`process`/`console` 缺失而整包安装失败。
const PRELUDE: &str = r#"
var globalThis = globalThis || this;
var global = globalThis;
var window = globalThis;
var self = globalThis;
var process = { env: {}, platform: 'win32', version: '' };
var console = console || { log: function(){}, warn: function(){}, error: function(){}, info: function(){}, debug: function(){} };
function setTimeout(){ return 0; }
function clearTimeout(){}
function setInterval(){ return 0; }
function clearInterval(){}
function require(name){ throw new Error('require is not available: ' + name); }
"#;

/// 解析插件元信息。失败时返回可直接展示给用户的中文错误。
pub fn parse_manifest(source: &str) -> Result<PluginManifest, AppError> {
    if source.trim().is_empty() {
        return Err(AppError::BadRequest("插件内容为空".to_string()));
    }
    if source.len() > MAX_PLUGIN_BYTES {
        return Err(AppError::BadRequest(format!(
            "插件过大(超过 {} MB)",
            MAX_PLUGIN_BYTES / 1024 / 1024
        )));
    }

    let runtime = Runtime::new()
        .map_err(|e| AppError::Internal(anyhow::anyhow!("初始化插件运行时失败: {e}")))?;
    runtime.set_max_stack_size(4 * 1024 * 1024);
    let context = Context::full(&runtime)
        .map_err(|e| AppError::Internal(anyhow::anyhow!("初始化插件上下文失败: {e}")))?;

    let script = format!(
        r#"(function(){{
  try {{
    {prelude}
    var module = {{ exports: {{}} }};
    var exports = module.exports;
    (function(module, exports, require){{
{source}
    }})(module, exports, require);
    var m = module.exports || {{}};
    return JSON.stringify({{
      platform: m.platform || '',
      name: m.name || m.platform || '',
      version: m.version || '',
      author: m.author || '',
      srcUrl: m.srcUrl || '',
      supportedSearchType: Array.isArray(m.supportedSearchType) ? m.supportedSearchType : []
    }});
  }} catch (e) {{
    return '__JS_ERR__:' + (e && e.message ? e.message : String(e));
  }}
}})()"#,
        prelude = PRELUDE,
        source = source
    );

    let raw: String = context.with(|ctx| {
        // 打包产物普遍依赖 sloppy 语义, 与抖音签名脚本一致关闭严格模式。
        let mut options = EvalOptions::default();
        options.strict = false;
        ctx.eval_with_options(script.as_bytes(), options)
    })
    .map_err(|e| AppError::BadRequest(format!("插件脚本无法执行: {e}")))?;

    if let Some(err) = raw.strip_prefix("__JS_ERR__:") {
        return Err(AppError::BadRequest(plugin_error_message(err)));
    }

    let mut manifest: PluginManifest = serde_json::from_str(&raw)
        .map_err(|e| AppError::BadRequest(format!("插件元信息格式无效: {e}")))?;
    manifest.platform = manifest.platform.trim().to_string();
    manifest.name = manifest.name.trim().to_string();
    manifest.version = manifest.version.trim().to_string();
    manifest.author = manifest.author.trim().to_string();
    manifest.src_url = manifest.src_url.trim().to_string();
    if manifest.platform.is_empty() {
        return Err(AppError::BadRequest(
            "插件缺少 platform 字段，不是有效的 MusicFree 插件".to_string(),
        ));
    }
    Ok(manifest)
}

/// 把 JS 异常翻译成用户能理解的中文提示。
fn plugin_error_message(raw: &str) -> String {
    if raw.contains("require is not available") {
        return format!("该插件依赖 Node 内置模块，暂不支持安装（{raw}）");
    }
    if raw.contains("is not defined") {
        return format!("插件脚本缺少运行时环境（{raw}）");
    }
    format!("插件脚本执行失败：{raw}")
}

/// 从 `module.exports` 之外的字段兜底提取(部分插件用 `exports.xxx = ...`)。
pub fn manifest_from_value(value: &Value) -> PluginManifest {
    let text = |key: &str| value.get(key).and_then(Value::as_str).unwrap_or("").to_string();
    PluginManifest {
        platform: text("platform"),
        name: text("name"),
        version: text("version"),
        author: text("author"),
        src_url: text("srcUrl"),
        supported_search_type: value
            .get("supportedSearchType")
            .and_then(Value::as_array)
            .map(|list| {
                list.iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_commonjs_manifest() {
        let manifest = parse_manifest(
            r#"
            module.exports = {
              platform: "测试音源",
              version: "1.2.3",
              author: "tester",
              srcUrl: "https://example.com/plugin.js",
              supportedSearchType: ["music", "album"],
              search: function () { return { data: [], isEnd: true }; }
            };
            "#,
        )
        .expect("manifest should parse");
        assert_eq!(manifest.platform, "测试音源");
        assert_eq!(manifest.version, "1.2.3");
        assert_eq!(manifest.src_url, "https://example.com/plugin.js");
        assert_eq!(manifest.supported_search_type, vec!["music", "album"]);
    }

    #[test]
    fn rejects_plugin_without_platform() {
        let err = parse_manifest("module.exports = {};").unwrap_err();
        assert!(err.to_string().contains("platform"));
    }

    #[test]
    fn reports_node_require_dependency() {
        let err = parse_manifest("var fs = require('fs'); module.exports = {};").unwrap_err();
        assert!(err.to_string().contains("Node"));
    }
}
