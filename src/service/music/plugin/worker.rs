//! 单个音乐插件的 JS 工作线程。
//!
//! rquickjs 的 `Runtime`/`Context` 不是 `Send`, 不能跨线程移动, 因此每个插件
//! 独占一个 OS 线程: 线程内持有 QuickJS 运行时, 主线程只通过与 mpsc 通道
//! 提交任务。插件死循环/崩溃只会拖住它自己的线程, 不影响应用主线程。
//!
//! 插件的 `fetch`/`XMLHttpRequest` 由 [`shim`](super::shim) 注入, 最终回到
//! Rust 侧的 [`http_bridge`], 复用项目统一的代理与超时配置。

use std::collections::HashMap;
use std::time::Duration;

use rquickjs::context::EvalOptions;
use rquickjs::{Context, Function, Runtime};
use serde_json::{json, Value};
use tokio::sync::{mpsc, oneshot};

use crate::crawler::fetcher::{self, HttpMethod, RequestSpec};
use crate::crawler::http_client::HttpClient;
use crate::error::error::AppError;

use super::shim;

/// 单次 JS 调用允许的微任务上限, 防止插件写出永不结束的 Promise 链。
const MAX_PENDING_JOBS: usize = 20_000;
const HTTP_TIMEOUT: Duration = Duration::from_secs(15);
const MAX_RESPONSE_BYTES: usize = 8 * 1024 * 1024;

pub struct Job {
    pub method: String,
    pub args: String,
    pub reply: oneshot::Sender<Result<String, String>>,
}

pub struct PluginWorker {
    pub platform: String,
    sender: mpsc::UnboundedSender<Job>,
}

impl PluginWorker {
    /// 创建线程并加载插件。初始化在子线程内完成, 本函数只负责建通道,
    /// 因此不会阻塞调用方; 真正的加载错误会在第一次调用时返回。
    pub fn spawn(
        platform: &str,
        source: &str,
        http: HttpClient,
        user_variables: HashMap<String, String>,
    ) -> Self {
        let (sender, mut receiver) = mpsc::unbounded_channel::<Job>();
        let platform_name = platform.to_string();
        let source = source.to_string();
        std::thread::Builder::new()
            .name(format!("music-plugin-{platform}"))
            .spawn(move || {
                let runtime = match tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                {
                    Ok(runtime) => runtime,
                    Err(_) => {
                        reply_all(&mut receiver, "创建插件网络运行时失败");
                        return;
                    }
                };
                let init = WorkerState::new(&source, http, &user_variables);
                loop {
                    let job = match runtime.block_on(receiver.recv()) {
                        Some(job) => job,
                        None => break,
                    };
                    let result = match &init {
                        Ok(state) => state.run(&job.method, &job.args),
                        Err(err) => Err(err.clone()),
                    };
                    let _ = job.reply.send(result);
                }
            })
            .ok();
        Self {
            platform: platform_name,
            sender,
        }
    }

    /// 提交一次插件调用, 由调用方负责超时控制。
    pub fn submit(
        &self,
        method: &str,
        args: &Value,
    ) -> Result<oneshot::Receiver<Result<String, String>>, AppError> {
        let (reply, receiver) = oneshot::channel();
        self.sender
            .send(Job {
                method: method.to_string(),
                args: args.to_string(),
                reply,
            })
            .map_err(|_| AppError::BadRequest("插件线程已退出".to_string()))?;
        Ok(receiver)
    }
}

fn reply_all(receiver: &mut mpsc::UnboundedReceiver<Job>, message: &str) {
    while let Ok(job) = receiver.try_recv() {
        let _ = job.reply.send(Err(message.to_string()));
    }
}

struct WorkerState {
    /// Context 借用 Runtime, 必须同生命周期保存; 字段本身不被直接使用。
    _runtime: Runtime,
    context: Context,
}

impl WorkerState {
    fn new(
        source: &str,
        http: HttpClient,
        user_variables: &HashMap<String, String>,
    ) -> Result<Self, String> {
        let runtime = Runtime::new().map_err(|e| format!("创建插件运行时失败: {e}"))?;
        runtime.set_max_stack_size(4 * 1024 * 1024);
        runtime.set_memory_limit(64 * 1024 * 1024);
        let context = Context::full(&runtime).map_err(|e| format!("创建插件上下文失败: {e}"))?;

        // HTTP 桥使用独立的 current-thread runtime: 它只被 `block_on` 驱动,
        // 不与接收任务的主循环 runtime 共用, 避免在同一线程上嵌套驱动。
        let http_runtime = std::sync::Arc::new(
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .map_err(|e| format!("创建网络运行时失败: {e}"))?,
        );
        let http_for_js = http.clone();
        let runtime_for_js = http_runtime.clone();
        context
            .with(|ctx| -> Result<(), rquickjs::Error> {
                let bridge = Function::new(ctx.clone(), move |payload: String| -> String {
                    runtime_for_js.block_on(http_bridge(&http_for_js, &payload))
                })?;
                ctx.globals().set("__mf_http", bridge)?;
                Ok(())
            })
            .map_err(|e| format!("注入网络桥失败: {e}"))?;

        // 垫片 + CommonJS 包装。插件顶层抛错时记录到 __mf_boot_error,
        // 由这里翻译成可读错误返回给前端。
        // 参数顺序对齐 MusicFree: 插件通过 env.userVariables 读取用户在设置里
        // 填写的登录凭据等变量。
        let variables_json = serde_json::to_string(user_variables).unwrap_or_else(|_| "{}".into());
        let boot = format!(
            r#"{shim}
globalThis.__mf_user_variables = {variables};
try {{
  globalThis.__mf_plugin = (function () {{
    var module = {{ exports: {{}} }};
    var exports = module.exports;
    var __mf_vars = globalThis.__mf_user_variables || {{}};
    var env = {{
      userVariables: __mf_vars,
      getUserVariables: function () {{ return __mf_vars; }},
      appVersion: "1.0.0",
      os: "windows",
      lang: "zh-CN"
    }};
    (function (require, __musicfree_require, module, exports, console, env, URL, process) {{
{source}
    }})(require, require, module, exports, console, env, URL, process);
    return module.exports;
  }})();
}} catch (e) {{
  globalThis.__mf_boot_error = String((e && e.message) || e);
}}
"#,
            shim = shim::PRELUDE,
            variables = variables_json,
            source = source
        );
        context
            .with(|ctx| {
                let mut options = EvalOptions::default();
                options.strict = false;
                ctx.eval_with_options::<(), _>(boot.as_bytes(), options)
            })
            .map_err(|e| format!("插件脚本无法加载: {e}"))?;

        let boot_error: Option<String> = context
            .with(|ctx| ctx.globals().get::<_, Option<String>>("__mf_boot_error"))
            .map_err(|e| format!("读取插件加载状态失败: {e}"))?;
        if let Some(err) = boot_error {
            return Err(translate_plugin_error(&err));
        }

        // 运行时要活到线程结束, 与 Context 一起保存在 WorkerState 中。
        Ok(Self { _runtime: runtime, context })
    }

    fn run(&self, method: &str, args: &str) -> Result<String, String> {
        // 内部方法：读取插件声明的 userVariables 定义，不经过插件导出函数。
        if method == "__mf_user_variables" {
            return self.read_user_variables();
        }
        let raw = self
            .context
            .with(|ctx| -> Result<Option<String>, rquickjs::Error> {
                ctx.globals().set("__mf_method", method)?;
                ctx.globals().set("__mf_args", args)?;
                let script = r#"
globalThis.__mf_result = undefined;
(function () {
  try {
    var args = JSON.parse(globalThis.__mf_args);
    var fn = globalThis.__mf_plugin[globalThis.__mf_method];
    if (typeof fn !== 'function') {
      globalThis.__mf_result = JSON.stringify({ ok: false, error: '插件未实现 ' + globalThis.__mf_method });
      return;
    }
    Promise.resolve(fn.apply(globalThis.__mf_plugin, args)).then(
      function (value) { globalThis.__mf_result = JSON.stringify({ ok: true, data: value === undefined ? null : value }); },
      function (err) { globalThis.__mf_result = JSON.stringify({ ok: false, error: String((err && err.message) || err) }); }
    );
  } catch (e) {
    globalThis.__mf_result = JSON.stringify({ ok: false, error: String((e && e.message) || e) });
  }
})();
"#;
                let mut options = EvalOptions::default();
                options.strict = false;
                ctx.eval_with_options::<(), _>(script.as_bytes(), options)?;

                for _ in 0..MAX_PENDING_JOBS {
                    if !ctx.execute_pending_job() {
                        break;
                    }
                }

                ctx.globals().get::<_, Option<String>>("__mf_result")
            })
            .map_err(|e| format!("插件调用失败: {e}"))?;

        let raw = raw.ok_or_else(|| "插件未返回结果".to_string())?;
        let value: Value = serde_json::from_str(&raw).unwrap_or_else(|_| json!({}));
        if value.get("ok").and_then(Value::as_bool).unwrap_or(false) {
            Ok(serde_json::to_string(value.get("data").unwrap_or(&Value::Null)).unwrap_or_default())
        } else {
            Err(translate_plugin_error(
                value
                    .get("error")
                    .and_then(Value::as_str)
                    .unwrap_or("插件返回了未知错误"),
            ))
        }
    }

    /// 读取插件声明的 userVariables（[{key,name,hint}]），供设置界面渲染表单。
    fn read_user_variables(&self) -> Result<String, String> {
        self.context
            .with(|ctx| -> Result<String, rquickjs::Error> {
                let script = r#"
(function () {
  try {
    var list = (globalThis.__mf_plugin || {}).userVariables;
    return JSON.stringify(Array.isArray(list) ? list : []);
  } catch (e) { return "[]"; }
})()
"#;
                let mut options = EvalOptions::default();
                options.strict = false;
                ctx.eval_with_options::<String, _>(script.as_bytes(), options)
            })
            .map_err(|e| format!("读取插件变量失败: {e}"))
    }
}

/// 把插件异常翻译成可操作的中文提示。
fn translate_plugin_error(raw: &str) -> String {
    if raw.contains("require is not available") {
        return format!("该插件依赖 Node 内置模块，当前运行时不支持（{raw}）");
    }
    format!("插件执行失败：{raw}")
}

/// 供 JS 调用的同步 HTTP 桥。请求走项目统一的 `HttpClient`(含代理与超时)。
async fn http_bridge(http: &HttpClient, payload: &str) -> String {
    let request: Value = match serde_json::from_str(payload) {
        Ok(value) => value,
        Err(err) => return json!({ "error": format!("请求参数无效: {err}") }).to_string(),
    };
    let url = request.get("url").and_then(Value::as_str).unwrap_or("").trim();
    if url.is_empty() {
        return json!({ "error": "请求地址为空" }).to_string();
    }
    let method = match request.get("method").and_then(Value::as_str).unwrap_or("GET") {
        "POST" => HttpMethod::POST,
        "PUT" => HttpMethod::PUT,
        "PATCH" => HttpMethod::PATCH,
        "DELETE" => HttpMethod::DELETE,
        "HEAD" => HttpMethod::HEAD,
        _ => HttpMethod::GET,
    };
    let headers = request
        .get("headers")
        .and_then(Value::as_object)
        .map(|map| {
            map.iter()
                .map(|(key, value)| {
                    let text = value
                        .as_str()
                        .map(str::to_string)
                        .unwrap_or_else(|| value.to_string());
                    (key.clone(), text)
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let body = request.get("body").and_then(Value::as_str).map(str::to_string);

    let spec = RequestSpec {
        url: url.to_string(),
        method,
        headers,
        body,
        retry: 0,
        timeout: Some(HTTP_TIMEOUT),
        ..Default::default()
    };
    let fetched = fetcher::fetch(http, spec).await;
    match fetched {
        Ok(response) => {
            let body = truncate(response.body, MAX_RESPONSE_BYTES);
            let headers = response
                .headers
                .into_iter()
                .map(|(key, value)| (key.to_lowercase(), Value::String(value)))
                .collect::<serde_json::Map<String, Value>>();
            json!({
                "url": response.url,
                "status": response.status,
                "body": body,
                "headers": Value::Object(headers),
            })
            .to_string()
        }
        Err(err) => json!({ "error": err.to_string() }).to_string(),
    }
}

/// 按字节上限截断响应, 避免单个插件把 64MB 内存吃满。
fn truncate(mut body: String, max_bytes: usize) -> String {
    if body.len() <= max_bytes {
        return body;
    }
    let mut end = max_bytes;
    while end > 0 && !body.is_char_boundary(end) {
        end -= 1;
    }
    body.truncate(end);
    body
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const FIXTURE_PLUGIN: &str = r#"
module.exports = {
  platform: "fixture",
  version: "1.0.0",
  supportedSearchType: ["music"],
  search: async function (query, page, type) {
    return { isEnd: true, data: [{ platform: "fixture", id: "1", title: "Hello " + query, artist: "T" }] };
  },
  getMediaSource: function (item, quality) {
    return { url: "https://example.com/a.mp3", headers: { Referer: "https://example.com" } };
  },
  getLyric: function (item) { return "[00:00.00]la"; },
  userVariables: [{ key: "token", name: "登录令牌", hint: "从网页版抓取" }],
  echoEnv: function () { return { token: env.userVariables.token || "" }; }
};
"#;

    #[test]
    fn truncates_on_char_boundary() {
        let body = "音乐".repeat(10);
        let cut = truncate(body.clone(), 7);
        assert!(cut.len() <= 7);
        assert!(body.starts_with(&cut));
    }

    #[test]
    fn translates_require_errors() {
        let message = translate_plugin_error("require is not available: fs");
        assert!(message.contains("Node"));
    }

    #[test]
    fn exposes_user_variables_through_env() {
        let http = HttpClient::new(5, None).expect("http client");
        let mut variables = HashMap::new();
        variables.insert("token".to_string(), "abc123".to_string());
        let worker = PluginWorker::spawn("fixture", FIXTURE_PLUGIN, http, variables);
        let runtime = tokio::runtime::Runtime::new().expect("runtime");

        let echoed = runtime
            .block_on(async {
                let receiver = worker.submit("echoEnv", &json!([])).expect("submit");
                tokio::time::timeout(Duration::from_secs(5), receiver).await
            })
            .expect("echo timeout")
            .expect("echo channel")
            .expect("echo result");
        let value: Value = serde_json::from_str(&echoed).expect("json");
        assert_eq!(value["token"], json!("abc123"));

        let definitions = runtime
            .block_on(async {
                let receiver = worker
                    .submit("__mf_user_variables", &json!([]))
                    .expect("submit");
                tokio::time::timeout(Duration::from_secs(5), receiver).await
            })
            .expect("definition timeout")
            .expect("definition channel")
            .expect("definition result");
        let list: Value = serde_json::from_str(&definitions).expect("json");
        assert_eq!(list[0]["key"], json!("token"));
        assert_eq!(list[0]["name"], json!("登录令牌"));
    }

    #[test]
    fn executes_async_plugin_functions() {
        let http = HttpClient::new(5, None).expect("http client");
        let worker = PluginWorker::spawn("fixture", FIXTURE_PLUGIN, http, HashMap::new());
        let runtime = tokio::runtime::Runtime::new().expect("runtime");

        let search = runtime
            .block_on(async {
                let receiver = worker
                    .submit("search", &json!(["world", 1, "music"]))
                    .expect("submit");
                tokio::time::timeout(Duration::from_secs(5), receiver).await
            })
            .expect("search timeout")
            .expect("search channel")
            .expect("search result");
        let value: Value = serde_json::from_str(&search).expect("json");
        assert_eq!(value["isEnd"], json!(true));
        assert_eq!(value["data"][0]["title"], json!("Hello world"));

        let lyric = runtime
            .block_on(async {
                let receiver = worker
                    .submit("getLyric", &json!([{ "platform": "fixture", "id": "1" }]))
                    .expect("submit");
                tokio::time::timeout(Duration::from_secs(5), receiver).await
            })
            .expect("lyric timeout")
            .expect("lyric channel")
            .expect("lyric result");
        assert!(lyric.contains("[00:00.00]"));
    }
}
