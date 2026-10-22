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
      appVersion: m.appVersion || '',
      description: m.description || '',
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
    manifest.app_version = manifest.app_version.trim().to_string();
    manifest.description = manifest.description.trim().to_string();
    if manifest.platform.is_empty() {
        return Err(AppError::BadRequest(
            "插件缺少 platform 字段，不是有效的 MusicFree 插件".to_string(),
        ));
    }
    check_app_version(&manifest)?;
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
        app_version: text("appVersion"),
        description: text("description"),
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

/// 宿主当前兼容的 MusicFree 版本。
///
/// 插件 `appVersion` 声明的是 **MusicFree** 对宿主 API 的要求，不是 Simple Happy 的
/// 版本号。这里用本项目已对齐的上游版本参与比较，避免拿 `1.1.0` 去比 `^0.6.0`
/// 这类区间而误判。
pub const HOST_COMPAT_VERSION: &str = "0.6.2";

/// 校验插件声明的宿主版本要求。插件不写 `appVersion` 视为不限制。
fn check_app_version(manifest: &PluginManifest) -> Result<(), AppError> {
    if manifest.app_version.is_empty()
        || version_satisfies(HOST_COMPAT_VERSION, &manifest.app_version)
    {
        return Ok(());
    }
    Err(AppError::BadRequest(format!(
        "插件要求宿主版本 {}，当前兼容版本 {}，请更新 Simple Happy 或更换插件",
        manifest.app_version, HOST_COMPAT_VERSION
    )))
}

/// npm semver 的常用子集：`*`/空、`>=` `>` `<=` `<` `=` `^` `~`、`1.2.x`，
/// 空格表示 AND，`||` 表示 OR。
pub fn version_satisfies(host: &str, requirement: &str) -> bool {
    let requirement = requirement.trim();
    if requirement.is_empty() {
        return true;
    }
    requirement.split("||").any(|group| {
        let tokens = requirement_tokens(group);
        // 无法解析的区间按「不限制」处理，宁放行也不误拒。
        tokens.is_empty() || tokens.iter().all(|(op, version)| match_requirement(host, op, version))
    })
}

/// 提取 `[(op, version)]`，兼容 `>= 0.6.0` 这种操作符与版本号之间带空格的写法。
fn requirement_tokens(group: &str) -> Vec<(String, String)> {
    static RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let re = RE.get_or_init(|| {
        regex::Regex::new(r"(>=|<=|>|<|=|\^|~)?\s*([0-9xX*]+(?:\.[0-9xX*]+)*)").expect("valid regex")
    });
    re.captures_iter(group)
        .filter_map(|caps| {
            let version = caps.get(2)?.as_str().to_string();
            Some((caps.get(1).map(|m| m.as_str()).unwrap_or("").to_string(), version))
        })
        .collect()
}

fn numeric_parts(value: &str) -> Vec<u64> {
    value
        .split('.')
        .map(|part| {
            part.chars()
                .take_while(|ch| ch.is_ascii_digit())
                .collect::<String>()
                .parse::<u64>()
                .unwrap_or(0)
        })
        .collect()
}

fn match_requirement(host: &str, op: &str, version: &str) -> bool {
    if version.is_empty() {
        return true;
    }
    if version.contains(['x', 'X', '*']) {
        return wildcard_matches(host, version);
    }
    let cmp = super::version_cmp(host, version);
    match op {
        ">=" => cmp >= 0,
        ">" => cmp > 0,
        "<=" => cmp <= 0,
        "<" => cmp < 0,
        "^" => cmp >= 0 && caret_compatible(host, version),
        "~" => cmp >= 0 && tilde_compatible(host, version),
        _ => cmp == 0,
    }
}

/// `^0.6.0` → minor 相同；`^1.2.0` → major 相同。
fn caret_compatible(host: &str, version: &str) -> bool {
    let host = numeric_parts(host);
    let want = numeric_parts(version);
    match (want.first().copied().unwrap_or(0), want.get(1).copied().unwrap_or(0)) {
        (0, 0) => host.get(2) == want.get(2),
        (0, _) => host.get(1) == want.get(1),
        _ => host.first() == want.first(),
    }
}

/// `~1.2.3` → major.minor 相同；`~1` → major 相同。
fn tilde_compatible(host: &str, version: &str) -> bool {
    let host = numeric_parts(host);
    let want = numeric_parts(version);
    if want.len() >= 2 {
        host.first() == want.first() && host.get(1) == want.get(1)
    } else {
        host.first() == want.first()
    }
}

/// 处理 `1.2.x` / `1.x` 这类通配区间。
fn wildcard_matches(host: &str, pattern: &str) -> bool {
    let host = numeric_parts(host);
    pattern.split('.').enumerate().all(|(index, part)| {
        is_wildcard_segment(part) || host.get(index).copied().unwrap_or(0) == part.parse::<u64>().unwrap_or(0)
    })
}

fn is_wildcard_segment(part: &str) -> bool {
    part.eq_ignore_ascii_case("x") || part == "*"
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

    #[test]
    fn satisfies_common_semver_ranges() {
        // 宿主兼容版本 0.6.2
        assert!(version_satisfies("0.6.2", ""));
        assert!(version_satisfies("0.6.2", "*"));
        assert!(version_satisfies("0.6.2", ">=0.5.0"));
        assert!(version_satisfies("0.6.2", ">= 0.5.0"));
        assert!(version_satisfies("0.6.2", "^0.6.0"));
        assert!(version_satisfies("0.6.2", "~0.6.0"));
        assert!(version_satisfies("0.6.2", "0.6.2"));
        assert!(version_satisfies("0.6.2", "0.6.x"));
        assert!(version_satisfies("0.6.2", ">=0.5.0 <0.7.0"));
        assert!(version_satisfies("0.6.2", "0.4.0 || >=0.6.0"));

        assert!(!version_satisfies("0.6.2", ">=0.7.0"));
        assert!(!version_satisfies("0.6.2", "^0.5.0"));
        assert!(!version_satisfies("0.6.2", "~0.7.0"));
        assert!(!version_satisfies("0.6.2", "<0.6.0"));
        assert!(!version_satisfies("0.6.2", ">=0.6.3"));
    }

    #[test]
    fn rejects_incompatible_app_version() {
        let err = parse_manifest(
            r#"
            module.exports = {
              platform: "未来音源",
              version: "1.0.0",
              appVersion: ">=9.0.0",
            };
            "#,
        )
        .unwrap_err();
        assert!(err.to_string().contains("宿主版本"));
    }

    #[test]
    fn accepts_compatible_app_version_and_description() {
        let manifest = parse_manifest(
            r#"
            module.exports = {
              platform: "正常音源",
              version: "1.0.0",
              appVersion: "^0.6.0",
              description: "示例插件",
            };
            "#,
        )
        .expect("compatible plugin");
        assert_eq!(manifest.app_version, "^0.6.0");
        assert_eq!(manifest.description, "示例插件");
    }
}
