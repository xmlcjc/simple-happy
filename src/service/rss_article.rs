//! RSS 正文提取。
//!
//! 很多订阅源的 `content` 只是摘要(末尾挂一个"查看全文"链接), 直接展示会让
//! 用户被迫跳浏览器。这里把原网页抓下来, 按正文密度挑出主体容器, 去掉导航/
//! 广告/脚本, 再把相对地址补成绝对地址, 交给前端按阅读器样式渲染。
//!
//! 只用 `scraper` + 正则做轻量清洗, 不追求完美还原: 目标是"文章能完整读",
//! 图片能显示, 不把整页导航塞进阅读区。

use once_cell::sync::Lazy;
use regex::{Captures, Regex};
use scraper::{Html, Selector};
use url::Url;

/// 常见的正文容器选择器(按优先级尝试, 最终按文本密度打分)。
const CANDIDATE_SELECTORS: &[&str] = &[
    "article",
    "[itemprop=articleBody]",
    // 微信公众号 / 知乎 / 少数派 / IT之家 等常见正文容器。
    "#js_content",
    ".Post-RichText",
    ".rich_media_content",
    ".article__content",
    ".post_content",
    ".article-content",
    ".article_content",
    ".post-content",
    ".entry-content",
    ".entry",
    ".markdown-body",
    ".blog-content",
    ".article-body",
    ".news-content",
    ".detail-content",
    "#article",
    "#content",
    ".content",
    ".article",
    ".post",
    "main",
];

static SCRIPT_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?is)<script\b[^>]*>.*?</script>").unwrap()
});
static STYLE_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?is)<style\b[^>]*>.*?</style>").unwrap());
/// 需要整段删除的噪音标签(Rust 正则不支持反向引用, 每种标签一条正则)。
const NOISE_TAGS: &[&str] = &[
    "noscript", "iframe", "form", "object", "canvas", "svg", "nav", "aside", "footer",
    "header", "button",
];
static NOISE_RES: Lazy<Vec<Regex>> = Lazy::new(|| {
    NOISE_TAGS
        .iter()
        .filter_map(|tag| {
            Regex::new(&format!(r"(?is)<{tag}\b[^>]*>.*?</{tag}>")).ok()
        })
        .collect()
});
static VOID_NOISE_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?is)<(link|meta|input|base|embed|source)\b[^>]*/?>").unwrap()
});
static COMMENT_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?s)<!--.*?-->").unwrap());
static EVENT_ATTR_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r#"(?i)\s+on[a-z]+\s*=\s*("[^"]*"|'[^']*')"#).unwrap());
static STYLE_ATTR_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r#"(?i)\s+style\s*=\s*("[^"]*"|'[^']*')"#).unwrap());
static DATA_SRC_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r#"(?i)\bdata-(?:src|original|actualsrc)\s*=\s*("[^"]*"|'[^']*')"#).unwrap());
static PLACEHOLDER_SRC_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r#"(?i)\s+src\s*=\s*"(?:data:image/[^"]{0,120}|[^"]*(?:blank|loading|transparent|placeholder)[^"]*)"#,
    )
    .unwrap()
});
static URL_ATTR_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"(?i)\b(href|src|poster)\s*=\s*("([^"]*)"|'([^']*)')"#).unwrap()
});
static TITLE_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?is)<title\b[^>]*>(.*?)</title>").unwrap());
static OG_TITLE_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"(?is)<meta[^>]+property\s*=\s*["']og:title["'][^>]+content\s*=\s*["']([^"']*)["']"#)
        .unwrap()
});

/// 从网页 HTML 里提取正文。
///
/// 返回 `(标题, 正文 HTML)`。提取不到正文时正文为空串, 由前端决定是否回退到
/// 内嵌网页面板。
pub fn extract_article(html: &str, page_url: &str) -> (String, String) {
    let document = Html::parse_document(html);
    let title = extract_title(&document, html);
    let base = Url::parse(page_url).ok();

    let mut best: Option<(usize, String)> = None;
    for selector in CANDIDATE_SELECTORS {
        let Ok(parsed) = Selector::parse(selector) else { continue };
        for element in document.select(&parsed) {
            let inner = element.inner_html();
            let score = score_html(&inner);
            if score == 0 {
                continue;
            }
            if best.as_ref().map(|(top, _)| score > *top).unwrap_or(true) {
                best = Some((score, inner));
            }
        }
    }

    // 正文容器命中但内容偏少(站点把正文包在多层 div / 使用非标准 class)时,
    // 再拿 body 兜底并取文本更多的一份; 容器本身够长就直接用, 避免把整页导航
    // 塞进阅读区。
    let body_html = Selector::parse("body")
        .ok()
        .and_then(|selector| {
            document
                .select(&selector)
                .next()
                .map(|element| element.inner_html())
        })
        .unwrap_or_default();

    let mut cleaned = best
        .map(|(_, inner)| sanitize(&inner, base.as_ref()))
        .unwrap_or_default();
    if plain_text_len(&cleaned) < 300 && !body_html.is_empty() {
        let from_body = sanitize(&body_html, base.as_ref());
        if plain_text_len(&from_body) > plain_text_len(&cleaned) {
            cleaned = from_body;
        }
    }
    if plain_text_len(&cleaned) < 60 {
        return (title, String::new());
    }
    (title, cleaned)
}

fn extract_title(document: &Html, html: &str) -> String {
    let h1 = Selector::parse("h1").expect("h1 selector");
    if let Some(element) = document.select(&h1).next() {
        let text = element.text().collect::<String>().trim().to_string();
        if !text.is_empty() {
            return text;
        }
    }
    let og = OG_TITLE_RE
        .captures(html)
        .and_then(|caps| caps.get(1).map(|m| m.as_str().trim().to_string()))
        .unwrap_or_default();
    if !og.is_empty() {
        return og;
    }
    TITLE_RE
        .captures(html)
        .and_then(|caps| caps.get(1).map(|m| m.as_str().trim().to_string()))
        .unwrap_or_default()
}

/// 文本长度 - 链接文本长度 + 图片权重; 用于挑正文容器。
fn score_html(html: &str) -> usize {
    let document = Html::parse_fragment(html);
    let text = document.root_element().text().collect::<String>();
    let text_len = text.trim().chars().count();
    if text_len == 0 {
        return 0;
    }
    let link_text: usize = match Selector::parse("a") {
        Ok(selector) => document
            .select(&selector)
            .map(|element| element.text().collect::<String>().chars().count())
            .sum(),
        Err(_) => 0,
    };
    let images = Selector::parse("img")
        .ok()
        .map(|selector| document.select(&selector).count())
        .unwrap_or(0);
    (text_len.saturating_sub(link_text)) + images * 120
}

fn sanitize(raw: &str, base: Option<&Url>) -> String {
    let mut text = COMMENT_RE.replace_all(raw, "").into_owned();
    text = SCRIPT_RE.replace_all(&text, "").into_owned();
    text = STYLE_RE.replace_all(&text, "").into_owned();
    for noise in NOISE_RES.iter() {
        text = noise.replace_all(&text, "").into_owned();
    }
    text = VOID_NOISE_RE.replace_all(&text, "").into_owned();
    text = EVENT_ATTR_RE.replace_all(&text, "").into_owned();
    text = STYLE_ATTR_RE.replace_all(&text, "").into_owned();
    // 懒加载: 去掉占位 src, 再把 data-src 提成 src, 否则图片全是占位图。
    text = PLACEHOLDER_SRC_RE.replace_all(&text, "").into_owned();
    text = DATA_SRC_RE
        .replace_all(&text, |caps: &Captures| {
            format!(" src={}", &caps[1])
        })
        .into_owned();
    text = URL_ATTR_RE
        .replace_all(&text, |caps: &Captures| {
            let attr = &caps[1];
            let raw_url = caps
                .get(3)
                .or_else(|| caps.get(4))
                .map(|m| m.as_str())
                .unwrap_or_default();
            let quote = if caps.get(3).is_some() { '"' } else { '\'' };
            let resolved = resolve_url(raw_url, base);
            match resolved {
                Some(url) => format!("{attr}={quote}{url}{quote}"),
                // `javascript:` / `#anchor` 等不可用地址: 保留但置空, 避免点击执行脚本。
                None => format!("{attr}={quote}#{quote}"),
            }
        })
        .into_owned();
    text.trim().to_string()
}

/// 相对地址补全为绝对地址; 不安全协议返回 None。
fn resolve_url(raw: &str, base: Option<&Url>) -> Option<String> {
    let value = raw.trim();
    if value.is_empty() || value.starts_with('#') {
        return None;
    }
    let lower = value.to_ascii_lowercase();
    if lower.starts_with("javascript:") || lower.starts_with("data:") || lower.starts_with("mailto:") {
        return None;
    }
    if lower.starts_with("//") {
        return Some(format!("https:{value}"));
    }
    if lower.starts_with("http://") || lower.starts_with("https://") {
        return Some(value.to_string());
    }
    let base = base?;
    base.join(value).ok().map(|url| url.to_string())
}

fn plain_text_len(html: &str) -> usize {
    let tags = Regex::new(r"(?s)<[^>]+>").expect("tag regex");
    tags.replace_all(html, " ").chars().filter(|c| !c.is_whitespace()).count()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_article_body_and_absolute_urls() {
        let html = r#"
            <html><head><title>示例文章</title></head>
            <body>
              <nav>首页 登录 注册</nav>
              <article>
                <h1>正文标题</h1>
                <p>这是第一段正文内容，长度足够用来触发正文判定，不应该包含导航。</p>
                <p>这是第二段正文内容，继续补充一些文字，保证正文长度超过阈值。</p>
                <p>这是第三段正文内容，用来让提取器稳定地选中 article 容器。</p>
                <img data-src="/images/cover.png" src="data:image/gif;base64,R0lGODlhAQABAAAAACw=" />
                <a href="/next">查看全文</a>
                <script>alert(1)</script>
              </article>
              <footer>版权信息</footer>
            </body></html>
        "#;
        let (title, body) = extract_article(html, "https://example.com/post/1");
        assert_eq!(title, "正文标题");
        assert!(body.contains("第一段正文内容"));
        assert!(body.contains("https://example.com/images/cover.png"));
        assert!(body.contains("https://example.com/next"));
        assert!(!body.contains("<script"));
        assert!(!body.contains("版权信息"));
        assert!(!body.contains("首页 登录"));
    }

    #[test]
    fn falls_back_to_empty_when_page_has_no_content() {
        let html = "<html><body><nav>导航</nav></body></html>";
        let (_, body) = extract_article(html, "https://example.com/");
        assert!(body.is_empty());
    }

    #[test]
    fn rejects_javascript_links() {
        let html = r#"<html><body><article><p>正文正文正文正文正文正文正文正文正文正文正文正文正文正文正文正文正文正文正文正文正文正文正文正文正文正文</p><a href="javascript:void(0)">点我</a></article></body></html>"#;
        let (_, body) = extract_article(html, "https://example.com/");
        assert!(!body.contains("javascript:"));
    }

    /// 真实网页冒烟测试: `cargo test --lib rss_article -- --ignored`。
    #[tokio::test]
    #[ignore]
    async fn extracts_real_page() {
        let url = "https://www.ruanyifeng.com/blog/2024/11/weekly-issue-327.html";
        let html = reqwest::get(url).await.expect("fetch").text().await.expect("body");
        let (title, body) = extract_article(&html, url);
        assert!(!title.is_empty());
        assert!(body.len() > 1000, "extracted too little: {}", body.len());
        assert!(!body.contains("<nav"));
        assert!(!body.contains("<script"));
    }
}
