use serde::{Deserialize, Serialize};

/// 本地划线：按「书 + 章 + 段落序号」保存，完全在设备上，不依赖书源。
///
/// 与 fqapp 的 `ReaderUnderline` 同一粒度：只记段落序号与段落文本，渲染时按
/// 文本匹配回 DOM；正文因换源/排版变化导致序号漂移时，仍能靠文本找到位置。
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct Highlight {
    pub book_url: String,
    pub book_name: String,
    pub book_author: String,
    pub chapter_index: i32,
    pub chapter_url: String,
    pub chapter_title: String,
    /// 段落在本章正文中的序号（从 0 开始）。
    pub block_index: i32,
    /// 段落纯文本，用于渲染时做二次匹配。
    pub text: String,
    /// 用户写的笔记，可为空。
    pub note: String,
    pub created_at: i64,
}

impl Highlight {
    /// 同一段的划一是同一条记录：再次划线是覆盖，不是新增。
    pub fn key(&self) -> String {
        format!(
            "{}\u{1f}{}\u{1f}{}",
            self.book_url, self.chapter_index, self.block_index
        )
    }
}
