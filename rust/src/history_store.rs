use anyhow::{anyhow, Context, Result};
use chrono::{Local, NaiveDateTime};
use html_escape::{encode_double_quoted_attribute, encode_text};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Component, Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub id: String,
    pub ts: String,
    pub prompt: String,
    pub images: Vec<String>,
}

pub struct HistoryStore {
    base_dir: PathBuf,
    max_active_entries: usize,
    history_json_path: PathBuf,
    history_html_path: PathBuf,
    images_root: PathBuf,
}

impl HistoryStore {
    pub const MAX_IMAGE_BYTES: usize = 20 * 1024 * 1024;
    const ALLOWED_EXTENSIONS: [&'static str; 4] = [".png", ".jpg", ".jpeg", ".webp"];

    pub fn new(base_dir: PathBuf, max_active_entries: usize) -> Result<Self> {
        let resolved_max = if max_active_entries == 0 {
            300
        } else {
            max_active_entries
        };
        let store = Self {
            history_json_path: base_dir.join("history.json"),
            history_html_path: base_dir.join("History.html"),
            images_root: base_dir.join("images"),
            base_dir,
            max_active_entries: resolved_max,
        };
        store.ensure_files()?;
        Ok(store)
    }

    pub fn history_html_path(&self) -> &Path {
        &self.history_html_path
    }

    pub fn append_history(&mut self, prompt: &str) -> Result<HistoryEntry> {
        let cleaned = prompt.trim();
        if cleaned.is_empty() {
            return Err(anyhow!("prompt is empty"));
        }

        let mut entries = self.read_entries(&self.history_json_path)?;
        let now = Local::now();
        let entry_id = self.next_entry_id(now.naive_local(), &entries);
        let entry = HistoryEntry {
            id: entry_id,
            ts: now.format("%Y-%m-%d %H:%M:%S").to_string(),
            prompt: cleaned.to_string(),
            images: Vec::new(),
        };

        entries.push(entry.clone());
        let kept_entries = self.rotate_if_needed(entries)?;
        self.write_entries(&self.history_json_path, &kept_entries)?;
        Ok(entry)
    }

    pub fn delete_history(&mut self, history_id: &str) -> Result<bool> {
        let history_id = history_id.trim();
        if history_id.is_empty() {
            return Ok(false);
        }

        let Some((target_path, entries, _)) = self.find_entry_container(history_id)? else {
            return Ok(false);
        };

        let filtered: Vec<HistoryEntry> = entries
            .into_iter()
            .filter(|entry| entry.id.trim() != history_id)
            .collect();
        self.write_entries(&target_path, &filtered)?;
        Ok(true)
    }

    pub fn update_history_prompt(&mut self, history_id: &str, prompt: &str) -> Result<bool> {
        let cleaned = prompt.trim();
        if cleaned.is_empty() {
            return Err(anyhow!("prompt is empty"));
        }

        let Some((target_path, mut entries, index)) = self.find_entry_container(history_id)? else {
            return Ok(false);
        };

        entries[index].prompt = cleaned.to_string();
        self.write_entries(&target_path, &entries)?;
        Ok(true)
    }

    pub fn append_image(
        &mut self,
        history_id: &str,
        source_name: &str,
        content: &[u8],
    ) -> Result<String> {
        let ext = Path::new(source_name)
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| format!(".{}", e.to_lowercase()))
            .ok_or_else(|| anyhow!("unsupported file extension"))?;

        if !Self::ALLOWED_EXTENSIONS
            .iter()
            .any(|allowed| *allowed == ext)
        {
            return Err(anyhow!("unsupported file extension"));
        }

        if content.len() > Self::MAX_IMAGE_BYTES {
            return Err(anyhow!("file size exceeds 20MB"));
        }

        let Some((target_path, mut entries, index)) = self.find_entry_container(history_id)? else {
            return Err(anyhow!("history id not found"));
        };

        let now = Local::now();
        let month_dir = self
            .images_root
            .join(now.format("%Y").to_string())
            .join(now.format("%m").to_string());
        fs::create_dir_all(&month_dir)
            .with_context(|| format!("failed to create images dir: {}", month_dir.display()))?;

        let rel_path = self.next_image_rel_path(now.naive_local(), &month_dir, &ext);
        let abs_path = self.base_dir.join(&rel_path);
        fs::write(&abs_path, content)
            .with_context(|| format!("failed to write image: {}", abs_path.display()))?;

        entries[index].images = vec![path_to_posix(&rel_path)];
        self.write_entries(&target_path, &entries)?;
        Ok(path_to_posix(&rel_path))
    }

    pub fn read_image_blob(&self, image_path: &str) -> Result<(Vec<u8>, &'static str)> {
        let cleaned = image_path.trim();
        if cleaned.is_empty() {
            return Err(anyhow!("image path is empty"));
        }

        let rel_path = Path::new(cleaned);
        if rel_path.is_absolute() {
            return Err(anyhow!("absolute image path is not allowed"));
        }
        if rel_path
            .components()
            .any(|part| matches!(part, Component::ParentDir | Component::CurDir))
        {
            return Err(anyhow!("invalid image path"));
        }

        let normalized = path_to_posix(rel_path);
        if !normalized.starts_with("images/") {
            return Err(anyhow!("image path is out of scope"));
        }

        let abs_path = self.base_dir.join(rel_path);
        let bytes = fs::read(&abs_path)
            .with_context(|| format!("failed to read image: {}", abs_path.display()))?;
        Ok((bytes, image_content_type(rel_path)))
    }

    pub fn regenerate_html(&self, server_port: u16) -> Result<()> {
        let entries = self.read_entries(&self.history_json_path)?;
        let archive_date_keys = self.collect_archive_date_keys()?;

        let content = self.build_history_html(
            &entries,
            "Prompt History",
            true,
            true,
            server_port,
            &archive_date_keys,
        );
        fs::write(&self.history_html_path, content).with_context(|| {
            format!("failed to write html: {}", self.history_html_path.display())
        })?;

        for date_key in archive_date_keys {
            let archive_json = self.archive_json_path(&date_key);
            let archive_entries = if archive_json.exists() {
                self.read_entries(&archive_json)?
            } else {
                Vec::new()
            };
            let archive_content = self.build_history_html(
                &archive_entries,
                &format!("Prompt History Archive {}", date_key),
                true,
                true,
                server_port,
                &[],
            );
            let archive_html = self.archive_html_path(&date_key);
            fs::write(&archive_html, archive_content)
                .with_context(|| format!("failed to write html: {}", archive_html.display()))?;
        }

        Ok(())
    }

    fn ensure_files(&self) -> Result<()> {
        fs::create_dir_all(&self.base_dir)
            .with_context(|| format!("failed to create base dir: {}", self.base_dir.display()))?;
        fs::create_dir_all(&self.images_root).with_context(|| {
            format!(
                "failed to create images dir: {}",
                self.images_root.display()
            )
        })?;

        if !self.history_json_path.exists() {
            fs::write(&self.history_json_path, "[]").with_context(|| {
                format!(
                    "failed to init history file: {}",
                    self.history_json_path.display()
                )
            })?;
            return Ok(());
        }

        match self.read_entries(&self.history_json_path) {
            Ok(entries) => self.write_entries(&self.history_json_path, &entries),
            Err(_) => {
                let now_tag = Local::now().format("%Y%m%d_%H%M%S").to_string();
                let backup = self
                    .base_dir
                    .join(format!("history.broken.{}.json", now_tag));
                fs::rename(&self.history_json_path, backup).with_context(|| {
                    format!(
                        "failed to backup broken history: {}",
                        self.history_json_path.display()
                    )
                })?;
                fs::write(&self.history_json_path, "[]").with_context(|| {
                    format!(
                        "failed to reset history file: {}",
                        self.history_json_path.display()
                    )
                })
            }
        }
    }

    fn archive_json_path(&self, date_key: &str) -> PathBuf {
        self.base_dir.join(format!("History_{}.json", date_key))
    }

    fn archive_html_path(&self, date_key: &str) -> PathBuf {
        self.base_dir.join(format!("History_{}.html", date_key))
    }

    fn rotate_if_needed(&self, entries: Vec<HistoryEntry>) -> Result<Vec<HistoryEntry>> {
        let overflow = entries.len() as isize - self.max_active_entries as isize;
        if overflow <= 0 {
            return Ok(entries);
        }

        let split_at = usize::try_from(overflow).unwrap_or(0);
        let moving = entries[..split_at].to_vec();
        let kept = entries[split_at..].to_vec();

        let mut grouped: BTreeMap<String, Vec<HistoryEntry>> = BTreeMap::new();
        for entry in moving {
            let date_key = self.date_key_from_entry(&entry);
            grouped.entry(date_key).or_default().push(entry);
        }

        for (date_key, items) in grouped {
            let json_path = self.archive_json_path(&date_key);
            let existing = if json_path.exists() {
                self.read_entries(&json_path)?
            } else {
                Vec::new()
            };

            let mut merged_by_id: BTreeMap<String, HistoryEntry> = BTreeMap::new();
            for entry in existing.into_iter().chain(items.into_iter()) {
                merged_by_id.insert(entry.id.clone(), entry);
            }

            let merged: Vec<HistoryEntry> = merged_by_id.into_values().collect();
            self.write_entries(&json_path, &merged)?;
        }

        Ok(kept)
    }

    fn find_entry_container(
        &self,
        history_id: &str,
    ) -> Result<Option<(PathBuf, Vec<HistoryEntry>, usize)>> {
        let mut sources = vec![self.history_json_path.clone()];
        sources.extend(self.list_archive_json_paths()?);

        for source in sources {
            if !source.exists() {
                continue;
            }
            let entries = self.read_entries(&source)?;
            if let Some((index, _)) = entries
                .iter()
                .enumerate()
                .find(|(_, entry)| entry.id.trim() == history_id)
            {
                return Ok(Some((source, entries, index)));
            }
        }

        Ok(None)
    }

    fn list_archive_json_paths(&self) -> Result<Vec<PathBuf>> {
        let mut paths = Vec::new();
        for item in fs::read_dir(&self.base_dir)
            .with_context(|| format!("failed to list base dir: {}", self.base_dir.display()))?
        {
            let item = item?;
            let path = item.path();
            let Some(file_name) = path.file_name().and_then(|v| v.to_str()) else {
                continue;
            };
            if !file_name.starts_with("History_") || !file_name.ends_with(".json") {
                continue;
            }
            let date_key = file_name
                .strip_prefix("History_")
                .and_then(|v| v.strip_suffix(".json"))
                .unwrap_or_default();
            if date_key.len() == 8 && date_key.chars().all(|ch| ch.is_ascii_digit()) {
                paths.push(path);
            }
        }
        paths.sort_by(|a, b| b.cmp(a));
        Ok(paths)
    }

    fn date_key_from_entry(&self, entry: &HistoryEntry) -> String {
        if entry.id.len() >= 8 && entry.id.chars().take(8).all(|ch| ch.is_ascii_digit()) {
            return entry.id[..8].to_string();
        }

        let digits: String = entry.ts.chars().filter(|ch| ch.is_ascii_digit()).collect();
        if digits.len() >= 8 {
            return digits[..8].to_string();
        }

        Local::now().format("%Y%m%d").to_string()
    }

    fn collect_archive_date_keys(&self) -> Result<Vec<String>> {
        let mut keys = Vec::new();
        for path in self.list_archive_json_paths()? {
            let Some(file_name) = path.file_name().and_then(|v| v.to_str()) else {
                continue;
            };
            if let Some(date_key) = file_name
                .strip_prefix("History_")
                .and_then(|v| v.strip_suffix(".json"))
            {
                if date_key.len() == 8 && date_key.chars().all(|ch| ch.is_ascii_digit()) {
                    keys.push(date_key.to_string());
                }
            }
        }
        keys.sort_by(|a, b| b.cmp(a));
        Ok(keys)
    }

    fn read_entries(&self, source: &Path) -> Result<Vec<HistoryEntry>> {
        let raw_text = fs::read_to_string(source)
            .with_context(|| format!("failed to read json: {}", source.display()))?;
        let raw: Value = serde_json::from_str(&raw_text)
            .with_context(|| format!("failed to parse json: {}", source.display()))?;

        let Some(array) = raw.as_array() else {
            return Err(anyhow!("json is not an array: {}", source.display()));
        };

        let mut normalized = Vec::new();
        for item in array {
            let Some(obj) = item.as_object() else {
                continue;
            };

            let entry_id = obj
                .get("id")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .trim()
                .to_string();
            let ts = obj
                .get("ts")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .trim()
                .to_string();
            let prompt = obj
                .get("prompt")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .trim()
                .to_string();

            let mut images = Vec::new();
            if let Some(raw_images) = obj.get("images").and_then(Value::as_array) {
                for value in raw_images {
                    if let Some(path) = value.as_str().map(str::trim).filter(|v| !v.is_empty()) {
                        images.push(path.to_string());
                    }
                }
            }
            if images.len() > 1 {
                if let Some(last) = images.last().cloned() {
                    images = vec![last];
                }
            }

            if entry_id.is_empty() || ts.is_empty() || prompt.is_empty() {
                continue;
            }

            normalized.push(HistoryEntry {
                id: entry_id,
                ts,
                prompt,
                images,
            });
        }

        Ok(normalized)
    }

    fn write_entries(&self, target: &Path, entries: &[HistoryEntry]) -> Result<()> {
        let payload =
            serde_json::to_string_pretty(entries).context("failed to serialize history json")?;
        let tmp_name = format!(
            "{}.tmp",
            target
                .file_name()
                .and_then(|v| v.to_str())
                .unwrap_or("history.json")
        );
        let tmp_path = target.with_file_name(tmp_name);

        fs::write(&tmp_path, payload)
            .with_context(|| format!("failed to write temp json: {}", tmp_path.display()))?;
        if target.exists() {
            fs::remove_file(target)
                .with_context(|| format!("failed to remove old json: {}", target.display()))?;
        }
        fs::rename(&tmp_path, target)
            .with_context(|| format!("failed to replace json: {}", target.display()))
    }

    fn next_entry_id(&self, now: NaiveDateTime, entries: &[HistoryEntry]) -> String {
        let base = now.format("%Y%m%d_%H%M%S").to_string();
        let prefix = format!("{}{}", base, "_");
        let mut seq: i32 = 1;

        for entry in entries {
            if !entry.id.starts_with(&prefix) {
                continue;
            }
            let parts: Vec<&str> = entry.id.split('_').collect();
            if parts.len() != 3 {
                continue;
            }
            if let Ok(parsed) = parts[2].parse::<i32>() {
                seq = seq.max(parsed + 1);
            }
        }

        format!("{base}_{seq:04}")
    }

    fn next_image_rel_path(&self, now: NaiveDateTime, month_dir: &Path, ext: &str) -> PathBuf {
        let base = now.format("%Y%m%d_%H%M%S").to_string();
        let year = now.format("%Y").to_string();
        let month = now.format("%m").to_string();
        let mut seq = 1u32;

        loop {
            let file_name = format!("{}_{:02}{}", base, seq, ext);
            let abs_path = month_dir.join(&file_name);
            if !abs_path.exists() {
                return PathBuf::from("images")
                    .join(year.clone())
                    .join(month.clone())
                    .join(file_name);
            }
            seq += 1;
        }
    }

    fn build_history_html(
        &self,
        entries: &[HistoryEntry],
        title: &str,
        interactive: bool,
        allow_delete: bool,
        server_port: u16,
        archive_date_keys: &[String],
    ) -> String {
        let mut sorted_entries = entries.to_vec();
        sorted_entries.sort_by(|a, b| b.id.cmp(&a.id));

        let mut cards = Vec::new();
        for entry in &sorted_entries {
            let entry_id = encode_double_quoted_attribute(&entry.id).to_string();
            let ts = encode_text(&entry.ts).to_string();
            let prompt_html = encode_text(&entry.prompt).to_string();

            let selected_image_path = entry.images.first().cloned().unwrap_or_default();
            let selected_image_attr =
                encode_double_quoted_attribute(&selected_image_path).to_string();
            let has_image = !selected_image_path.is_empty();

            let mut images_block = String::new();
            if has_image {
                let safe_path_attr =
                    encode_double_quoted_attribute(&selected_image_path).to_string();
                let safe_path_text = encode_text(&selected_image_path).to_string();
                images_block.push_str(&format!(
                    "<div class=\"image-item is-selected\" data-image-path=\"{}\"><a class=\"thumb-image-link\" href=\"{}\" target=\"_blank\" rel=\"noopener noreferrer\"><img class=\"thumb-image\" src=\"{}\" alt=\"history image\" loading=\"lazy\" /></a><a class=\"thumb-path\" href=\"{}\" target=\"_blank\" rel=\"noopener noreferrer\">{}</a></div>",
                    safe_path_attr, safe_path_attr, safe_path_attr, safe_path_attr, safe_path_text
                ));
            } else {
                images_block.push_str("<span class=\"muted\">画像なし</span>");
            }

            let delete_btn = if interactive && allow_delete {
                "<button class=\"btn delete-btn\">削除</button>"
            } else {
                ""
            };
            let image_copy_disabled = if has_image { "" } else { " disabled" };

            let upload_block = if interactive {
                let upload_text = if has_image {
                    "画像追加済み（差し替えはD＆Dまたはクリック）"
                } else {
                    "画像追加: ドラッグ&ドロップ または クリック"
                };
                let upload_state_class = if has_image {
                    "has-image"
                } else {
                    "needs-image"
                };
                format!(
                    "<section class=\"upload\" data-history-id=\"{}\"><div class=\"dropzone {}\">{}</div><input class=\"file-input\" type=\"file\" accept=\".png,.jpg,.jpeg,.webp\" /></section>",
                    entry_id,
                    upload_state_class,
                    encode_text(upload_text)
                )
            } else {
                String::new()
            };

            cards.push(format!(
                "<article class=\"entry\" data-history-id=\"{}\" data-has-image=\"{}\" data-selected-image=\"{}\"><header class=\"entry-header\"><span class=\"timestamp\">{}</span></header><div class=\"entry-body\"><section class=\"prompt-pane\"><div class=\"prompt-toolbar\"><button class=\"btn overwrite-btn\">上書き</button><button class=\"btn copy-btn\">コピー</button>{}</div><textarea class=\"prompt-editor\" spellcheck=\"false\">{}</textarea></section><section class=\"media-pane\">{}<section class=\"images\">{}</section><button class=\"btn image-copy-btn\"{}>画像をクリップボードにコピー</button></section></div></article>",
                entry_id,
                if has_image { "true" } else { "false" },
                selected_image_attr,
                ts,
                delete_btn,
                prompt_html,
                upload_block,
                images_block,
                image_copy_disabled
            ));
        }

        let body_cards = if cards.is_empty() {
            "<p class=\"empty\">履歴はまだありません。</p>".to_string()
        } else {
            cards.join("\n")
        };

        let archive_links = if archive_date_keys.is_empty() {
            String::new()
        } else {
            let mut links = Vec::new();
            for date_key in archive_date_keys {
                let href = format!("History_{}.html", date_key);
                links.push(format!(
                    "<a class=\"archive-link\" href=\"{}\" target=\"_blank\" rel=\"noopener noreferrer\">{}</a>",
                    encode_double_quoted_attribute(&href),
                    encode_text(&href)
                ));
            }
            format!(
                "<section class=\"archives\"><h2>Archives</h2><div class=\"archive-list\">{}</div></section>",
                links.join("")
            )
        };
        let runtime_notice = if allow_delete {
            "<p class=\"runtime-note\">※このページの上書き・削除・画像追加・画像コピー機能は、アプリ起動中のみ使用できます。</p>"
        } else {
            ""
        };

        let interactive_script = if interactive {
            INTERACTIVE_SCRIPT_TEMPLATE
                .replace("__API_BASE__", &format!("http://127.0.0.1:{server_port}"))
        } else {
            NON_INTERACTIVE_SCRIPT.to_string()
        };

        let mut output = String::new();
        output.push_str("<!doctype html>\n<html lang=\"ja\">\n<head>\n");
        output.push_str("  <meta charset=\"utf-8\" />\n");
        output.push_str(
            "  <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\" />\n",
        );
        output.push_str("  <title>");
        output.push_str(&encode_text(title));
        output.push_str("</title>\n");
        output.push_str(HISTORY_STYLE);
        output.push_str("\n</head>\n<body>\n  <main class=\"wrap\">\n    <h1>");
        output.push_str(&encode_text(title));
        output.push_str("</h1>\n");
        output.push_str(runtime_notice);
        output.push_str("\n");
        output.push_str(&archive_links);
        output.push_str("\n");
        output.push_str(&body_cards);
        output.push_str("\n  </main>\n");
        output.push_str(&interactive_script);
        output.push_str("\n</body>\n</html>\n");
        output
    }
}

fn path_to_posix(path: &Path) -> String {
    path.components()
        .map(|c| c.as_os_str().to_string_lossy().to_string())
        .collect::<Vec<String>>()
        .join("/")
}

fn image_content_type(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|v| v.to_str())
        .map(|v| v.to_ascii_lowercase())
        .as_deref()
    {
        Some("png") => "image/png",
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("webp") => "image/webp",
        _ => "application/octet-stream",
    }
}

const HISTORY_STYLE: &str = r#"
  <style>
    :root {
      --bg: #f6f6ef;
      --panel: #ffffff;
      --line: #1f2a44;
      --accent: #cb4b16;
      --accent-2: #174c7a;
      --text: #1e1e1e;
      --muted: #666;
    }
    * { box-sizing: border-box; }
    body {
      margin: 0;
      color: var(--text);
      background: radial-gradient(circle at 10% 10%, #fff8d8, transparent 35%), linear-gradient(180deg, #f7f5ec, #ece8d8);
      font-family: "Yu Mincho", "Hiragino Mincho ProN", serif;
    }
    .wrap { max-width: 980px; margin: 32px auto; padding: 0 16px 32px; }
    h1 { margin: 0 0 10px; font-size: 38px; letter-spacing: 0.04em; }
    .runtime-note {
      margin: 0 0 16px;
      border: 1px solid #d8c78d;
      background: #fff7dc;
      color: #5c4a1f;
      padding: 8px 10px;
      font-family: "Yu Gothic UI", sans-serif;
      font-size: 13px;
      line-height: 1.5;
    }
    h2 { margin: 0 0 8px; font-size: 20px; }
    .archives {
      margin: 0 0 16px;
      border: 1px solid var(--line);
      background: #fff;
      padding: 10px;
    }
    .archive-list { display: flex; gap: 8px; flex-wrap: wrap; }
    .archive-link {
      font-family: "Yu Gothic UI", sans-serif;
      border: 1px solid var(--line);
      padding: 4px 8px;
      text-decoration: none;
      color: var(--accent-2);
      background: #f8f8f8;
      font-size: 13px;
    }
    .entry {
      border: 2px solid var(--line);
      background: var(--panel);
      margin-bottom: 16px;
      padding: 12px;
      box-shadow: 6px 6px 0 #d8d2bf;
    }
    .entry-header {
      display: flex;
      align-items: flex-start;
      margin-bottom: 10px;
    }
    .entry-body {
      display: grid;
      grid-template-columns: minmax(0, 1fr) minmax(0, 1fr);
      gap: 14px;
      align-items: start;
    }
    .prompt-pane, .media-pane { min-width: 0; }
    .media-pane {
      display: flex;
      flex-direction: column;
      align-items: stretch;
    }
    .timestamp { font-weight: 700; color: var(--accent-2); }
    .btn {
      border: 2px solid var(--line);
      background: #fff;
      color: var(--line);
      padding: 6px 12px;
      cursor: pointer;
      font-weight: 700;
    }
    .btn:hover { background: #f4ede1; }
    .btn:disabled {
      cursor: not-allowed;
      opacity: 0.55;
      background: #f0eee7;
    }
    .btn.feedback-visible {
      position: relative;
      overflow: visible;
    }
    .btn.feedback-visible::after {
      content: attr(data-feedback);
      position: absolute;
      left: 50%;
      bottom: calc(100% + 10px);
      transform: translateX(-50%);
      background: #1f2a44;
      color: #fff;
      padding: 4px 8px;
      border-radius: 4px;
      font-size: 12px;
      font-family: "Yu Gothic UI", sans-serif;
      white-space: nowrap;
      box-shadow: 0 2px 8px rgba(0, 0, 0, 0.22);
      pointer-events: none;
      z-index: 2;
    }
    .btn.feedback-visible::before {
      content: "";
      position: absolute;
      left: 50%;
      bottom: calc(100% + 4px);
      width: 8px;
      height: 8px;
      transform: translateX(-50%) rotate(45deg);
      background: #1f2a44;
      pointer-events: none;
      z-index: 1;
    }
    .overwrite-btn { border-color: var(--accent-2); color: var(--accent-2); }
    .delete-btn { border-color: var(--accent); color: var(--accent); }
    .prompt-toolbar {
      display: flex;
      gap: 8px;
      margin-bottom: 8px;
      flex-wrap: wrap;
    }
    .prompt-editor {
      width: 100%;
      border-left: 4px solid var(--line);
      padding: 8px 10px;
      background: #fbfaf5;
      font-family: "Yu Gothic UI", sans-serif;
      font-size: 14px;
      line-height: 1.5;
      min-height: 156px;
      resize: vertical;
      white-space: pre-wrap;
      word-break: break-word;
    }
    .upload { margin-top: 0; }
    .dropzone {
      border: 2px dashed var(--line);
      padding: 10px;
      text-align: center;
      cursor: pointer;
      background: #fefcf3;
      font-family: "Yu Gothic UI", sans-serif;
      display: flex;
      align-items: center;
      justify-content: center;
    }
    .dropzone.needs-image { min-height: 96px; }
    .dropzone.has-image { min-height: 0; }
    .dropzone.dragover { background: #fff4d3; }
    .file-input { display: none; }
    .images {
      margin-top: 10px;
      display: flex;
      flex-wrap: wrap;
      gap: 8px;
      font-family: "Yu Gothic UI", sans-serif;
    }
    .image-item {
      width: 100%;
      display: flex;
      flex-direction: column;
      gap: 6px;
    }
    .thumb-image-link {
      display: block;
      border: 1px solid var(--line);
      background: #f8f8f8;
      padding: 6px;
      cursor: pointer;
    }
    .thumb-image {
      display: block;
      width: 100%;
      max-height: 240px;
      object-fit: contain;
      background: #fff;
    }
    .thumb-path {
      border: 1px solid var(--line);
      padding: 4px 8px;
      font-size: 12px;
      text-decoration: none;
      color: var(--accent-2);
      background: #f8f8f8;
      max-width: 100%;
      overflow: hidden;
      text-overflow: ellipsis;
      white-space: nowrap;
    }
    .image-item.is-selected .thumb-image-link,
    .image-item.is-selected .thumb-path {
      border-color: var(--accent-2);
      box-shadow: inset 0 0 0 1px var(--accent-2);
    }
    .image-copy-btn {
      margin-top: 10px;
      align-self: flex-start;
      position: relative;
      overflow: visible;
    }
    .image-copy-btn.copy-feedback::after {
      content: "クリップボードにコピーしました";
      position: absolute;
      left: 50%;
      bottom: calc(100% + 10px);
      transform: translateX(-50%);
      background: #1f2a44;
      color: #fff;
      padding: 4px 8px;
      border-radius: 4px;
      font-size: 12px;
      font-family: "Yu Gothic UI", sans-serif;
      white-space: nowrap;
      box-shadow: 0 2px 8px rgba(0, 0, 0, 0.22);
      pointer-events: none;
      z-index: 2;
    }
    .image-copy-btn.copy-feedback::before {
      content: "";
      position: absolute;
      left: 50%;
      bottom: calc(100% + 4px);
      width: 8px;
      height: 8px;
      transform: translateX(-50%) rotate(45deg);
      background: #1f2a44;
      pointer-events: none;
      z-index: 1;
    }
    .muted { color: var(--muted); }
    .empty { padding: 24px; border: 1px dashed var(--line); background: #fff; }
    @media (max-width: 720px) {
      h1 { font-size: 30px; }
      .entry-body { grid-template-columns: minmax(0, 1fr); }
      .prompt-editor { min-height: 0; }
      .image-copy-btn { align-self: stretch; }
    }
  </style>
"#;

const INTERACTIVE_SCRIPT_TEMPLATE: &str = r#"
  <script>
    const API_BASE = "__API_BASE__";
    const HISTORY_REVISION_POLL_MS = 1000;
    let lastHistoryRevision = null;
    let historyRevisionPolling = false;
    async function parseApiResponse(res, fallback) {
      let data = {};
      try {
        data = await res.json();
      } catch (_) {
        data = {};
      }
      if (!res.ok || !data.ok) {
        throw new Error(data.error || fallback);
      }
      return data;
    }
    async function fetchHistoryRevision() {
      const res = await fetch(`${API_BASE}/app/history-revision`, {
        method: "GET",
        cache: "no-store"
      });
      const data = await parseApiResponse(res, "history revision failed");
      const revision = Number(data.revision);
      if (!Number.isFinite(revision)) {
        throw new Error("invalid history revision");
      }
      return revision;
    }
    async function pollHistoryRevision() {
      if (historyRevisionPolling) {
        return;
      }
      historyRevisionPolling = true;
      try {
        const revision = await fetchHistoryRevision();
        if (lastHistoryRevision === null) {
          lastHistoryRevision = revision;
          return;
        }
        if (revision !== lastHistoryRevision) {
          location.reload();
          return;
        }
      } catch (_) {
        // Ignore transient errors (e.g. app stopped) and keep current page state.
      } finally {
        historyRevisionPolling = false;
      }
    }
    function getPromptValue(entry) {
      const editor = entry.querySelector(".prompt-editor");
      return editor ? editor.value : "";
    }
    async function copyPrompt(entry) {
      const prompt = getPromptValue(entry);
      await navigator.clipboard.writeText(prompt);
    }
    async function overwritePrompt(historyId, prompt) {
      const res = await fetch(`${API_BASE}/update`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ history_id: historyId, prompt })
      });
      return parseApiResponse(res, "update failed");
    }
    async function deleteEntry(historyId) {
      if (!confirm("プロンプトを削除しますか？（画像は削除されません）")) {
        return;
      }
      const res = await fetch(`${API_BASE}/delete`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ history_id: historyId })
      });
      await parseApiResponse(res, "delete failed");
      location.reload();
    }
    async function uploadFile(historyId, file) {
      const form = new FormData();
      form.append("history_id", historyId);
      form.append("file", file);
      const res = await fetch(`${API_BASE}/upload`, {
        method: "POST",
        body: form
      });
      return parseApiResponse(res, "upload failed");
    }
    async function copyImageToClipboard(imagePath) {
      if (
        !navigator.clipboard ||
        typeof navigator.clipboard.write !== "function" ||
        typeof ClipboardItem === "undefined"
      ) {
        throw new Error("このブラウザは画像コピーに対応していません");
      }
      const imageUrl = `${API_BASE}/image?path=${encodeURIComponent(imagePath)}`;
      let res;
      try {
        res = await fetch(imageUrl, { cache: "no-store" });
      } catch (_) {
        throw new Error("アプリが起動していない可能性があります");
      }
      if (!res.ok) {
        let message = "画像を取得できませんでした";
        try {
          const data = await res.json();
          if (data && typeof data.error === "string" && data.error.trim() !== "") {
            message = data.error;
          }
        } catch (_) {}
        throw new Error(message);
      }
      const blob = await res.blob();
      const blobType = blob.type && blob.type.startsWith("image/") ? blob.type : "image/png";
      const copyBlob = blob.type === blobType ? blob : new Blob([blob], { type: blobType });
      await navigator.clipboard.write([new ClipboardItem({ [blobType]: copyBlob })]);
    }
    function showImageCopyFeedback(button) {
      if (!button) return;
      button.classList.remove("copy-feedback");
      if (button._copyFeedbackTimer) {
        clearTimeout(button._copyFeedbackTimer);
      }
      void button.offsetWidth;
      button.classList.add("copy-feedback");
      button._copyFeedbackTimer = setTimeout(() => {
        button.classList.remove("copy-feedback");
        button._copyFeedbackTimer = null;
      }, 1400);
    }
    function showButtonFeedback(button, message) {
      if (!button || !message) return;
      button.dataset.feedback = message;
      button.classList.remove("feedback-visible");
      if (button._buttonFeedbackTimer) {
        clearTimeout(button._buttonFeedbackTimer);
      }
      void button.offsetWidth;
      button.classList.add("feedback-visible");
      button._buttonFeedbackTimer = setTimeout(() => {
        button.classList.remove("feedback-visible");
        button.dataset.feedback = "";
        button._buttonFeedbackTimer = null;
      }, 1400);
    }
    function syncUploadLabel(entry) {
      const upload = entry.querySelector(".upload");
      if (!upload) return;
      const dropzone = upload.querySelector(".dropzone");
      if (!dropzone) return;
      const hasImage = entry.dataset.hasImage === "true";
      dropzone.classList.toggle("has-image", hasImage);
      dropzone.classList.toggle("needs-image", !hasImage);
      dropzone.textContent = hasImage
        ? "画像追加済み（差し替えはD＆Dまたはクリック）"
        : "画像追加: ドラッグ&ドロップ または クリック";
    }
    function setSelectedImage(entry, imagePath) {
      entry.dataset.selectedImage = imagePath || "";
      for (const item of entry.querySelectorAll(".image-item")) {
        item.classList.toggle("is-selected", (item.dataset.imagePath || "") === entry.dataset.selectedImage);
      }
      const imageCopyBtn = entry.querySelector(".image-copy-btn");
      if (imageCopyBtn) {
        imageCopyBtn.disabled = !entry.dataset.selectedImage;
      }
    }
    function buildImageItem(imagePath) {
      const wrapper = document.createElement("div");
      wrapper.className = "image-item";
      wrapper.dataset.imagePath = imagePath;

      const imageLink = document.createElement("a");
      imageLink.className = "thumb-image-link";
      imageLink.href = imagePath;
      imageLink.target = "_blank";
      imageLink.rel = "noopener noreferrer";

      const img = document.createElement("img");
      img.className = "thumb-image";
      img.src = imagePath;
      img.alt = "history image";
      img.loading = "lazy";
      imageLink.appendChild(img);

      const pathLink = document.createElement("a");
      pathLink.className = "thumb-path";
      pathLink.href = imagePath;
      pathLink.target = "_blank";
      pathLink.rel = "noopener noreferrer";
      pathLink.textContent = imagePath;

      wrapper.appendChild(imageLink);
      wrapper.appendChild(pathLink);
      return wrapper;
    }
    function renderUploadedImage(entry, imagePath) {
      const images = entry.querySelector(".images");
      images.innerHTML = "";
      if (imagePath) {
        entry.dataset.hasImage = "true";
        entry.dataset.selectedImage = imagePath;
        images.appendChild(buildImageItem(imagePath));
      } else {
        entry.dataset.hasImage = "false";
        entry.dataset.selectedImage = "";
        const muted = document.createElement("span");
        muted.className = "muted";
        muted.textContent = "画像なし";
        images.appendChild(muted);
      }
      syncUploadLabel(entry);
      setSelectedImage(entry, entry.dataset.selectedImage || "");
    }
    for (const entry of document.querySelectorAll(".entry")) {
      const historyId = entry.dataset.historyId;
      const editor = entry.querySelector(".prompt-editor");
      const overwriteBtn = entry.querySelector(".overwrite-btn");
      const copyBtn = entry.querySelector(".copy-btn");
      const deleteBtn = entry.querySelector(".delete-btn");
      const imageCopyBtn = entry.querySelector(".image-copy-btn");
      const images = entry.querySelector(".images");
      const upload = entry.querySelector(".upload");
      const dropzone = upload ? upload.querySelector(".dropzone") : null;
      const fileInput = upload ? upload.querySelector(".file-input") : null;

      if (overwriteBtn) {
        overwriteBtn.addEventListener("click", async () => {
          const currentPrompt = getPromptValue(entry);
          try {
            const data = await overwritePrompt(historyId, currentPrompt);
            if (editor) {
              editor.value = typeof data.prompt === "string" ? data.prompt : currentPrompt.trim();
            }
            showButtonFeedback(overwriteBtn, "編集した内容で上書きしました");
          } catch (err) {
            alert(`上書き失敗: ${err.message}`);
          }
        });
      }
      if (copyBtn) {
        copyBtn.addEventListener("click", async () => {
          try {
            await copyPrompt(entry);
            showButtonFeedback(copyBtn, "コピーしました");
          } catch (err) {
            alert(`コピー失敗: ${err.message}`);
          }
        });
      }
      if (deleteBtn) {
        deleteBtn.addEventListener("click", async () => {
          try {
            await deleteEntry(historyId);
          } catch (err) {
            alert(`削除失敗: ${err.message}`);
          }
        });
      }
      if (imageCopyBtn) {
        imageCopyBtn.addEventListener("click", async () => {
          const imagePath = entry.dataset.selectedImage || "";
          if (!imagePath) {
            alert("コピー対象の画像がありません。");
            return;
          }
          try {
            await copyImageToClipboard(imagePath);
            showImageCopyFeedback(imageCopyBtn);
          } catch (err) {
            alert(`画像コピー失敗: ${err.message}`);
          }
        });
      }
      if (images) {
        images.addEventListener("click", (event) => {
          const target = event.target;
          if (!(target instanceof Element)) {
            return;
          }
          const thumbLink = target.closest(".thumb-image-link");
          if (!thumbLink || !images.contains(thumbLink)) {
            return;
          }
          event.preventDefault();
          const imageItem = thumbLink.closest(".image-item");
          if (!imageItem) {
            return;
          }
          setSelectedImage(entry, imageItem.dataset.imagePath || "");
        });
      }
      setSelectedImage(entry, entry.dataset.selectedImage || "");
      if (!dropzone || !fileInput) {
        continue;
      }
      syncUploadLabel(entry);
      const handleFile = async (file) => {
        if (!file) return;
        try {
          const data = await uploadFile(historyId, file);
          renderUploadedImage(entry, data.image_path || "");
        } catch (err) {
          alert(`アップロード失敗: ${err.message}`);
        } finally {
          fileInput.value = "";
        }
      };
      dropzone.addEventListener("click", () => fileInput.click());
      fileInput.addEventListener("change", async () => {
        const file = fileInput.files && fileInput.files[0];
        await handleFile(file);
      });
      dropzone.addEventListener("dragover", (event) => {
        event.preventDefault();
        dropzone.classList.add("dragover");
      });
      dropzone.addEventListener("dragleave", () => {
        dropzone.classList.remove("dragover");
      });
      dropzone.addEventListener("drop", async (event) => {
        event.preventDefault();
        dropzone.classList.remove("dragover");
        const file = event.dataTransfer && event.dataTransfer.files && event.dataTransfer.files[0];
        await handleFile(file);
      });
    }
    void pollHistoryRevision();
    setInterval(() => {
      void pollHistoryRevision();
    }, HISTORY_REVISION_POLL_MS);
  </script>
"#;

const NON_INTERACTIVE_SCRIPT: &str = r#"
  <script>
    function getPromptValue(entry) {
      const editor = entry.querySelector(".prompt-editor");
      return editor ? editor.value : "";
    }
    async function copyPrompt(entry) {
      const prompt = getPromptValue(entry);
      await navigator.clipboard.writeText(prompt);
    }
    function showImageCopyFeedback(button) {
      if (!button) return;
      button.classList.remove("copy-feedback");
      if (button._copyFeedbackTimer) {
        clearTimeout(button._copyFeedbackTimer);
      }
      void button.offsetWidth;
      button.classList.add("copy-feedback");
      button._copyFeedbackTimer = setTimeout(() => {
        button.classList.remove("copy-feedback");
        button._copyFeedbackTimer = null;
      }, 1400);
    }
    function showButtonFeedback(button, message) {
      if (!button || !message) return;
      button.dataset.feedback = message;
      button.classList.remove("feedback-visible");
      if (button._buttonFeedbackTimer) {
        clearTimeout(button._buttonFeedbackTimer);
      }
      void button.offsetWidth;
      button.classList.add("feedback-visible");
      button._buttonFeedbackTimer = setTimeout(() => {
        button.classList.remove("feedback-visible");
        button.dataset.feedback = "";
        button._buttonFeedbackTimer = null;
      }, 1400);
    }
    async function imageBlobFromPath(imagePath) {
      return new Promise((resolve, reject) => {
        const image = new Image();
        image.onload = () => {
          const width = image.naturalWidth || image.width;
          const height = image.naturalHeight || image.height;
          if (!width || !height) {
            reject(new Error("画像サイズを取得できませんでした"));
            return;
          }
          const canvas = document.createElement("canvas");
          canvas.width = width;
          canvas.height = height;
          const ctx = canvas.getContext("2d");
          if (!ctx) {
            reject(new Error("画像変換に失敗しました"));
            return;
          }
          ctx.drawImage(image, 0, 0);
          canvas.toBlob((blob) => {
            if (!blob) {
              reject(new Error("画像変換に失敗しました"));
              return;
            }
            resolve(blob);
          }, "image/png");
        };
        image.onerror = () => reject(new Error("画像を取得できませんでした"));
        image.src = imagePath;
      });
    }
    async function copyImageToClipboard(imagePath) {
      if (
        !navigator.clipboard ||
        typeof navigator.clipboard.write !== "function" ||
        typeof ClipboardItem === "undefined"
      ) {
        throw new Error("このブラウザは画像コピーに対応していません");
      }
      const blob = await imageBlobFromPath(imagePath);
      const blobType = blob.type && blob.type.startsWith("image/") ? blob.type : "image/png";
      const copyBlob = blob.type === blobType ? blob : new Blob([blob], { type: blobType });
      await navigator.clipboard.write([new ClipboardItem({ [blobType]: copyBlob })]);
    }
    for (const button of document.querySelectorAll(".copy-btn")) {
      button.addEventListener("click", async () => {
        try {
          const entry = button.closest(".entry");
          if (!entry) return;
          await copyPrompt(entry);
          showButtonFeedback(button, "コピーしました");
        } catch (err) {
          alert(`コピー失敗: ${err.message}`);
        }
      });
    }
    for (const button of document.querySelectorAll(".image-copy-btn")) {
      button.addEventListener("click", async () => {
        const entry = button.closest(".entry");
        if (!entry) return;
        const imagePath = entry.dataset.selectedImage || "";
        if (!imagePath) {
          alert("コピー対象の画像がありません。");
          return;
        }
        try {
          await copyImageToClipboard(imagePath);
          showImageCopyFeedback(button);
        } catch (err) {
          alert(`画像コピー失敗: ${err.message}`);
        }
      });
    }
  </script>
"#;

#[cfg(test)]
mod tests {
    use super::HistoryStore;
    use serde_json::Value;
    use std::fs;
    use std::path::Path;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_FIXTURE_ID: AtomicU64 = AtomicU64::new(1);

    fn fixture_base() -> std::path::PathBuf {
        let mut base = std::env::temp_dir();
        let sequence = NEXT_FIXTURE_ID.fetch_add(1, Ordering::Relaxed);
        base.push(format!(
            "ipg_history_store_test_{}_{}",
            std::process::id(),
            sequence
        ));
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(&base).expect("mkdir fixture");
        base
    }

    fn read_entries(path: &Path) -> Vec<Value> {
        let raw = fs::read_to_string(path).expect("read history");
        serde_json::from_str::<Value>(&raw)
            .expect("parse history")
            .as_array()
            .expect("history array")
            .to_vec()
    }

    fn find_entry<'a>(
        entries: &'a [Value],
        history_id: &str,
    ) -> &'a serde_json::Map<String, Value> {
        entries
            .iter()
            .find_map(|entry| {
                let obj = entry.as_object()?;
                let id = obj.get("id").and_then(Value::as_str).unwrap_or_default();
                if id == history_id {
                    Some(obj)
                } else {
                    None
                }
            })
            .expect("entry exists")
    }

    #[test]
    fn append_and_rotate_entries() {
        let base = fixture_base();
        let mut store = HistoryStore::new(base.clone(), 2).expect("create store");

        store.append_history("a").expect("append a");
        store.append_history("b").expect("append b");
        store.append_history("c").expect("append c");

        let raw = fs::read_to_string(base.join("history.json")).expect("read active history");
        let values: serde_json::Value = serde_json::from_str(&raw).expect("parse active history");
        assert_eq!(values.as_array().expect("active array").len(), 2);

        fs::remove_dir_all(base).ok();
    }

    #[test]
    fn delete_history_removes_active_entry() {
        let base = fixture_base();
        let mut store = HistoryStore::new(base.clone(), 5).expect("create store");

        let target = store.append_history("delete target").expect("append target");
        store.append_history("keep").expect("append keep");

        assert!(
            store.delete_history(&target.id).expect("delete active"),
            "active history should be deleted"
        );

        let entries = read_entries(&base.join("history.json"));
        assert!(
            entries.iter().all(|entry| {
                entry
                    .get("id")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    != target.id
            }),
            "deleted entry should not remain in active history"
        );

        fs::remove_dir_all(base).ok();
    }

    #[test]
    fn delete_history_removes_archive_entry() {
        let base = fixture_base();
        let mut store = HistoryStore::new(base.clone(), 1).expect("create store");

        let archived = store
            .append_history("archive delete target")
            .expect("append archived");
        store.append_history("active latest").expect("append active");
        let archive_json = base.join(format!("History_{}.json", &archived.id[..8]));
        assert!(archive_json.exists(), "archive file should exist");

        assert!(
            store
                .delete_history(&archived.id)
                .expect("delete archive entry"),
            "archive history should be deleted"
        );

        let archive_entries = read_entries(&archive_json);
        assert!(
            archive_entries.iter().all(|entry| {
                entry
                    .get("id")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    != archived.id
            }),
            "deleted entry should not remain in archive history"
        );

        fs::remove_dir_all(base).ok();
    }

    #[test]
    fn delete_history_returns_false_for_missing_history_id() {
        let base = fixture_base();
        let mut store = HistoryStore::new(base.clone(), 2).expect("create store");
        store.append_history("exists").expect("append");

        let deleted = store
            .delete_history("missing-id")
            .expect("missing id should not error");
        assert!(!deleted);

        fs::remove_dir_all(base).ok();
    }

    #[test]
    fn regenerate_html_includes_delete_button_in_archive_page() {
        let base = fixture_base();
        let mut store = HistoryStore::new(base.clone(), 1).expect("create store");

        let archived = store
            .append_history("archive delete available")
            .expect("append archived");
        store.append_history("active latest").expect("append active");

        store.regenerate_html(8765).expect("regenerate html");

        let archive_html_path = base.join(format!("History_{}.html", &archived.id[..8]));
        let archive_html = fs::read_to_string(&archive_html_path).expect("read archive html");
        assert!(
            archive_html.contains("<button class=\"btn delete-btn\">削除</button>"),
            "archive html should include delete button markup"
        );

        fs::remove_dir_all(base).ok();
    }

    #[test]
    fn update_history_prompt_updates_active_entry_and_keeps_ts_and_images() {
        let base = fixture_base();
        let mut store = HistoryStore::new(base.clone(), 5).expect("create store");

        let entry = store.append_history("before").expect("append");
        store
            .append_image(&entry.id, "sample.png", b"dummy")
            .expect("append image");

        let before_entries = read_entries(&base.join("history.json"));
        let before = find_entry(&before_entries, &entry.id);
        let ts_before = before
            .get("ts")
            .and_then(Value::as_str)
            .expect("before ts")
            .to_string();
        let images_before = before.get("images").cloned().expect("before images");

        assert!(
            store
                .update_history_prompt(&entry.id, "after")
                .expect("update active"),
            "active history should be updated"
        );

        let after_entries = read_entries(&base.join("history.json"));
        let after = find_entry(&after_entries, &entry.id);
        assert_eq!(
            after
                .get("prompt")
                .and_then(Value::as_str)
                .expect("after prompt"),
            "after"
        );
        assert_eq!(
            after.get("ts").and_then(Value::as_str).expect("after ts"),
            ts_before
        );
        assert_eq!(after.get("images").expect("after images"), &images_before);

        fs::remove_dir_all(base).ok();
    }

    #[test]
    fn update_history_prompt_updates_archive_entry() {
        let base = fixture_base();
        let mut store = HistoryStore::new(base.clone(), 1).expect("create store");

        let archived = store
            .append_history("archive before")
            .expect("append archived");
        store
            .append_history("active latest")
            .expect("append active");
        let archive_json = base.join(format!("History_{}.json", &archived.id[..8]));
        assert!(archive_json.exists(), "archive file should exist");

        assert!(
            store
                .update_history_prompt(&archived.id, "archive after")
                .expect("update archive"),
            "archive history should be updated"
        );

        let archive_entries = read_entries(&archive_json);
        let after = find_entry(&archive_entries, &archived.id);
        assert_eq!(
            after
                .get("prompt")
                .and_then(Value::as_str)
                .expect("archive prompt"),
            "archive after"
        );

        fs::remove_dir_all(base).ok();
    }

    #[test]
    fn update_history_prompt_rejects_empty_prompt() {
        let base = fixture_base();
        let mut store = HistoryStore::new(base.clone(), 2).expect("create store");
        store.append_history("exists").expect("append");

        let err = store
            .update_history_prompt("dummy-id", "   ")
            .expect_err("empty prompt should fail");
        assert!(err.to_string().contains("prompt is empty"));

        fs::remove_dir_all(base).ok();
    }

    #[test]
    fn update_history_prompt_returns_false_for_missing_history_id() {
        let base = fixture_base();
        let mut store = HistoryStore::new(base.clone(), 2).expect("create store");
        store.append_history("exists").expect("append");

        let updated = store
            .update_history_prompt("missing-id", "new prompt")
            .expect("missing id should not error");
        assert!(!updated);

        fs::remove_dir_all(base).ok();
    }
}
