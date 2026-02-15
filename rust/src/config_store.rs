use anyhow::{anyhow, Context, Result};
use serde::Serialize;
use std::fs;
use std::path::PathBuf;
use toml::map::Map;
use toml::Value;

use crate::NO_SELECTION;

#[derive(Debug, Clone, Serialize)]
pub struct ItemConfig {
    pub section_name: String,
    pub key: String,
    pub label: String,
    pub choices: Vec<String>,
    pub allow_free_text: bool,
    pub template: String,
}

impl ItemConfig {
    pub fn item_id(&self) -> String {
        format!("{}:{}", self.section_name, self.key)
    }
}

#[derive(Debug)]
pub struct ConfigStore {
    pub path: PathBuf,
    doc: Value,
}

impl ConfigStore {
    pub fn new(path: PathBuf) -> Result<Self> {
        if !path.exists() {
            return Err(anyhow!("config file not found: {}", path.display()));
        }

        let text = fs::read_to_string(&path)
            .with_context(|| format!("failed to read config: {}", path.display()))?;
        let doc: Value = toml::from_str(&text)
            .with_context(|| format!("failed to parse TOML: {}", path.display()))?;

        let mut store = Self { path, doc };
        store.normalize_doc();
        store.save()?;
        Ok(store)
    }

    pub fn save(&self) -> Result<()> {
        let serialized = toml::to_string_pretty(&self.doc).context("failed to serialize TOML")?;
        let text = move_app_table_to_top(&serialized);
        fs::write(&self.path, text)
            .with_context(|| format!("failed to write config: {}", self.path.display()))
    }

    pub fn delimiter(&self) -> String {
        self.app_table()
            .and_then(|t| t.get("delimiter"))
            .and_then(Value::as_str)
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| ", ".to_string())
    }

    pub fn confirm_delete(&self) -> bool {
        self.app_table()
            .and_then(|t| t.get("confirm_delete"))
            .and_then(Value::as_bool)
            .unwrap_or(true)
    }

    pub fn copy_debounce_sec(&self) -> f64 {
        self.app_table()
            .and_then(|t| t.get("copy_debounce_sec"))
            .and_then(value_to_f64)
            .filter(|v| *v >= 0.0)
            .unwrap_or(2.0)
    }

    pub fn history_server_port(&self) -> u16 {
        self.app_table()
            .and_then(|t| t.get("history_server_port"))
            .and_then(value_to_i64)
            .and_then(|v| u16::try_from(v).ok())
            .filter(|v| *v > 0)
            .unwrap_or(3000)
    }

    pub fn history_confirm_delete(&self) -> bool {
        self.app_table()
            .and_then(|t| t.get("history_confirm_delete"))
            .and_then(Value::as_bool)
            .unwrap_or(true)
    }

    pub fn history_max_entries(&self) -> usize {
        self.app_table()
            .and_then(|t| t.get("history_max_entries"))
            .and_then(value_to_i64)
            .and_then(|v| usize::try_from(v).ok())
            .filter(|v| *v > 0)
            .unwrap_or(300)
    }

    pub fn get_items(&self, section_name: &str) -> Vec<ItemConfig> {
        let mut items = Vec::new();
        let sections = self
            .doc
            .as_table()
            .and_then(|root| root.get("sections"))
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();

        for section_value in sections {
            let Some(section) = section_value.as_table() else {
                continue;
            };
            let Some(name) = section.get("name").and_then(Value::as_str) else {
                continue;
            };
            if name != section_name {
                continue;
            }

            let Some(section_items) = section.get("items").and_then(Value::as_array) else {
                continue;
            };
            for item_value in section_items {
                let Some(item) = item_value.as_table() else {
                    continue;
                };
                let key = item
                    .get("key")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .unwrap_or_default()
                    .to_string();
                if key.is_empty() {
                    continue;
                }

                let label = item
                    .get("label")
                    .and_then(Value::as_str)
                    .map(ToOwned::to_owned)
                    .unwrap_or_else(|| key.clone());

                let template = item
                    .get("template")
                    .and_then(Value::as_str)
                    .map(ToOwned::to_owned)
                    .unwrap_or_else(|| "{value}".to_string());

                let allow_free_text = item
                    .get("allow_free_text")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);

                let choices = normalize_choices_from_value(item.get("choices"));

                items.push(ItemConfig {
                    section_name: section_name.to_string(),
                    key,
                    label,
                    choices,
                    allow_free_text,
                    template,
                });
            }
        }

        items
    }

    pub fn add_choice(&mut self, section_name: &str, key: &str, value: &str) -> Result<bool> {
        let normalized = value.trim();
        if normalized.is_empty() || normalized == NO_SELECTION {
            return Ok(false);
        }

        let item = self
            .find_item_table_mut(section_name, key)
            .ok_or_else(|| anyhow!("item not found: {}.{}", section_name, key))?;
        let mut choices = normalize_choices_from_value(item.get("choices"));
        if choices.iter().any(|c| c == normalized) {
            return Ok(false);
        }

        choices.push(normalized.to_string());
        item.insert("choices".to_string(), choices_to_value(&choices));
        self.save()?;
        Ok(true)
    }

    pub fn remove_choice(&mut self, section_name: &str, key: &str, value: &str) -> Result<bool> {
        let normalized = value.trim();
        if normalized.is_empty() || normalized == NO_SELECTION {
            return Ok(false);
        }

        let item = self
            .find_item_table_mut(section_name, key)
            .ok_or_else(|| anyhow!("item not found: {}.{}", section_name, key))?;
        let choices = normalize_choices_from_value(item.get("choices"));
        if !choices.iter().any(|c| c == normalized) {
            return Ok(false);
        }

        let filtered: Vec<String> = choices.into_iter().filter(|c| c != normalized).collect();
        item.insert("choices".to_string(), choices_to_value(&filtered));
        self.save()?;
        Ok(true)
    }

    pub fn get_item_state(&self, section_name: &str, key: &str) -> (String, String) {
        let selected_key = format!("{}_selected", key);
        let free_key = format!("{}_free_text", key);

        let section_state = self
            .doc
            .as_table()
            .and_then(|root| root.get("state"))
            .and_then(Value::as_table)
            .and_then(|state| state.get(section_name))
            .and_then(Value::as_table);

        let selected = section_state
            .and_then(|table| table.get(&selected_key))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or(NO_SELECTION)
            .to_string();

        let free_text = section_state
            .and_then(|table| table.get(&free_key))
            .and_then(Value::as_str)
            .map(str::trim)
            .unwrap_or_default()
            .to_string();

        (selected, free_text)
    }

    pub fn set_item_state(
        &mut self,
        section_name: &str,
        key: &str,
        selected: &str,
        free_text: &str,
    ) -> Result<()> {
        let selected_value = if selected.trim().is_empty() {
            NO_SELECTION
        } else {
            selected.trim()
        };

        let section_table = self.ensure_section_state_mut(section_name);
        section_table.insert(
            format!("{}_selected", key),
            Value::String(selected_value.to_string()),
        );
        section_table.insert(
            format!("{}_free_text", key),
            Value::String(free_text.trim().to_string()),
        );

        self.save()
    }

    pub fn clear_section_state(&mut self, section_name: &str) -> Result<()> {
        let state = self.ensure_state_table_mut();
        state.insert(section_name.to_string(), Value::Table(Map::new()));
        self.save()
    }

    fn normalize_doc(&mut self) {
        if !self.doc.is_table() {
            self.doc = Value::Table(Map::new());
        }

        {
            let app = self.ensure_app_table_mut();

            if app.get("delimiter").and_then(Value::as_str).is_none() {
                app.insert("delimiter".to_string(), Value::String(", ".to_string()));
            }

            if app.get("confirm_delete").and_then(Value::as_bool).is_none() {
                app.insert("confirm_delete".to_string(), Value::Boolean(true));
            }

            let debounce = app
                .get("copy_debounce_sec")
                .and_then(value_to_f64)
                .filter(|v| *v >= 0.0)
                .unwrap_or(2.0);
            app.insert("copy_debounce_sec".to_string(), Value::Float(debounce));

            let port = app
                .get("history_server_port")
                .and_then(value_to_i64)
                .filter(|v| (1..=65_535).contains(v))
                .unwrap_or(3000);
            app.insert("history_server_port".to_string(), Value::Integer(port));

            if app
                .get("history_confirm_delete")
                .and_then(Value::as_bool)
                .is_none()
            {
                app.insert("history_confirm_delete".to_string(), Value::Boolean(true));
            }

            let max_entries = app
                .get("history_max_entries")
                .and_then(value_to_i64)
                .filter(|v| *v > 0)
                .unwrap_or(300);
            app.insert(
                "history_max_entries".to_string(),
                Value::Integer(max_entries),
            );
        }

        {
            let sections = self.ensure_sections_array_mut();
            for section_value in sections.iter_mut() {
                if !section_value.is_table() {
                    *section_value = Value::Table(Map::new());
                }
                let section = section_value
                    .as_table_mut()
                    .expect("section should be table after normalization");

                let name = section
                    .get("name")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .unwrap_or("prompt")
                    .to_string();
                section.insert("name".to_string(), Value::String(name.clone()));

                let label = section
                    .get("label")
                    .and_then(Value::as_str)
                    .map(ToOwned::to_owned)
                    .unwrap_or_else(|| name.clone());
                section.insert("label".to_string(), Value::String(label));

                let items_value = section
                    .entry("items".to_string())
                    .or_insert_with(|| Value::Array(Vec::new()));
                if !items_value.is_array() {
                    *items_value = Value::Array(Vec::new());
                }

                if let Some(items) = items_value.as_array_mut() {
                    for item_value in items.iter_mut() {
                        if !item_value.is_table() {
                            *item_value = Value::Table(Map::new());
                        }
                        let item = item_value
                            .as_table_mut()
                            .expect("item should be table after normalization");

                        let key = item
                            .get("key")
                            .map(value_to_text)
                            .map(|v| v.trim().to_string())
                            .unwrap_or_default();
                        item.insert("key".to_string(), Value::String(key.clone()));

                        let label = item
                            .get("label")
                            .and_then(Value::as_str)
                            .map(ToOwned::to_owned)
                            .unwrap_or_else(|| key.clone());
                        item.insert("label".to_string(), Value::String(label));

                        let allow_free_text = item
                            .get("allow_free_text")
                            .and_then(Value::as_bool)
                            .unwrap_or(true);
                        item.insert(
                            "allow_free_text".to_string(),
                            Value::Boolean(allow_free_text),
                        );

                        let template = item
                            .get("template")
                            .and_then(Value::as_str)
                            .map(ToOwned::to_owned)
                            .unwrap_or_else(|| "{value}".to_string());
                        item.insert("template".to_string(), Value::String(template));

                        let choices = normalize_choices_from_value(item.get("choices"));
                        item.insert("choices".to_string(), choices_to_value(&choices));
                    }
                }
            }
        }

        self.ensure_state_table_mut();
        self.reorder_root_tables();
    }

    fn app_table(&self) -> Option<&Map<String, Value>> {
        self.doc
            .as_table()
            .and_then(|root| root.get("app"))
            .and_then(Value::as_table)
    }

    fn root_table_mut(&mut self) -> &mut Map<String, Value> {
        if !self.doc.is_table() {
            self.doc = Value::Table(Map::new());
        }
        self.doc
            .as_table_mut()
            .expect("root should be table after normalization")
    }

    fn ensure_app_table_mut(&mut self) -> &mut Map<String, Value> {
        let root = self.root_table_mut();
        let app = root
            .entry("app".to_string())
            .or_insert_with(|| Value::Table(Map::new()));
        if !app.is_table() {
            *app = Value::Table(Map::new());
        }
        app.as_table_mut()
            .expect("app should be table after normalization")
    }

    fn ensure_sections_array_mut(&mut self) -> &mut Vec<Value> {
        let root = self.root_table_mut();
        let sections = root
            .entry("sections".to_string())
            .or_insert_with(|| Value::Array(Vec::new()));
        if !sections.is_array() {
            *sections = Value::Array(Vec::new());
        }
        sections
            .as_array_mut()
            .expect("sections should be array after normalization")
    }

    fn ensure_state_table_mut(&mut self) -> &mut Map<String, Value> {
        let root = self.root_table_mut();
        let state = root
            .entry("state".to_string())
            .or_insert_with(|| Value::Table(Map::new()));
        if !state.is_table() {
            *state = Value::Table(Map::new());
        }
        state
            .as_table_mut()
            .expect("state should be table after normalization")
    }

    fn ensure_section_state_mut(&mut self, section_name: &str) -> &mut Map<String, Value> {
        let state = self.ensure_state_table_mut();
        let section = state
            .entry(section_name.to_string())
            .or_insert_with(|| Value::Table(Map::new()));
        if !section.is_table() {
            *section = Value::Table(Map::new());
        }
        section
            .as_table_mut()
            .expect("section state should be table after normalization")
    }

    fn reorder_root_tables(&mut self) {
        let root = self.root_table_mut();
        let mut reordered = Map::new();

        for key in ["app", "sections", "state"] {
            if let Some(value) = root.remove(key) {
                reordered.insert(key.to_string(), value);
            }
        }

        let remaining_keys: Vec<String> = root.keys().cloned().collect();
        for key in remaining_keys {
            if let Some(value) = root.remove(&key) {
                reordered.insert(key, value);
            }
        }

        *root = reordered;
    }

    fn find_item_table_mut(
        &mut self,
        section_name: &str,
        key: &str,
    ) -> Option<&mut Map<String, Value>> {
        let sections = self.ensure_sections_array_mut();
        for section_value in sections.iter_mut() {
            let Some(section) = section_value.as_table_mut() else {
                continue;
            };
            let Some(name) = section.get("name").and_then(Value::as_str) else {
                continue;
            };
            if name != section_name {
                continue;
            }

            let Some(items) = section.get_mut("items").and_then(Value::as_array_mut) else {
                continue;
            };
            for item_value in items.iter_mut() {
                let Some(item) = item_value.as_table_mut() else {
                    continue;
                };
                if item.get("key").and_then(Value::as_str) == Some(key) {
                    return Some(item);
                }
            }
        }

        None
    }
}

fn normalize_choices_from_value(value: Option<&Value>) -> Vec<String> {
    let mut normalized = Vec::new();
    if let Some(Value::Array(items)) = value {
        for item in items {
            let text = value_to_text(item).trim().to_string();
            if !text.is_empty() && !normalized.iter().any(|existing| existing == &text) {
                normalized.push(text);
            }
        }
    }

    normalized.retain(|v| v != NO_SELECTION);
    normalized.insert(0, NO_SELECTION.to_string());
    normalized
}

fn choices_to_value(choices: &[String]) -> Value {
    Value::Array(choices.iter().cloned().map(Value::String).collect())
}

fn value_to_text(value: &Value) -> String {
    match value {
        Value::String(v) => v.clone(),
        Value::Integer(v) => v.to_string(),
        Value::Float(v) => v.to_string(),
        Value::Boolean(v) => v.to_string(),
        Value::Datetime(v) => v.to_string(),
        Value::Array(v) => format!("{:?}", v),
        Value::Table(v) => format!("{:?}", v),
    }
}

fn value_to_f64(value: &Value) -> Option<f64> {
    value
        .as_float()
        .or_else(|| value.as_integer().map(|v| v as f64))
        .or_else(|| value.as_str().and_then(|v| v.parse::<f64>().ok()))
}

fn value_to_i64(value: &Value) -> Option<i64> {
    value
        .as_integer()
        .or_else(|| value.as_float().map(|v| v as i64))
        .or_else(|| value.as_str().and_then(|v| v.parse::<i64>().ok()))
}

fn move_app_table_to_top(serialized: &str) -> String {
    let ends_with_newline = serialized.ends_with('\n');
    let lines: Vec<&str> = serialized.split('\n').collect();
    let header_starts: Vec<usize> = lines
        .iter()
        .enumerate()
        .filter_map(|(index, line)| is_top_level_header_line(line).then_some(index))
        .collect();

    if header_starts.is_empty() {
        return serialized.to_string();
    }

    let first_header = header_starts[0];
    let mut app_block_range: Option<(usize, usize)> = None;
    for (i, start) in header_starts.iter().enumerate() {
        if lines[*start].trim() == "[app]" {
            let end = header_starts.get(i + 1).copied().unwrap_or(lines.len());
            app_block_range = Some((*start, end));
            break;
        }
    }

    let Some((app_start, app_end)) = app_block_range else {
        return serialized.to_string();
    };

    if app_start == first_header {
        return serialized.to_string();
    }

    let mut rebuilt: Vec<&str> = Vec::with_capacity(lines.len());
    rebuilt.extend_from_slice(&lines[..first_header]);
    rebuilt.extend_from_slice(&lines[app_start..app_end]);

    for (i, start) in header_starts.iter().enumerate() {
        let end = header_starts.get(i + 1).copied().unwrap_or(lines.len());
        if *start == app_start {
            continue;
        }
        rebuilt.extend_from_slice(&lines[*start..end]);
    }

    let mut output = rebuilt.join("\n");
    if ends_with_newline {
        output.push('\n');
    }
    output
}

fn is_top_level_header_line(line: &str) -> bool {
    let trimmed = line.trim();
    if trimmed.is_empty() || !trimmed.starts_with('[') || !trimmed.ends_with(']') {
        return false;
    }
    !trimmed.contains(" = ")
}

#[cfg(test)]
mod tests {
    use super::ConfigStore;
    use crate::NO_SELECTION;
    use std::fs;
    use std::path::PathBuf;

    fn fixture_path(name: &str) -> PathBuf {
        let mut path = std::env::temp_dir();
        path.push(format!(
            "ipg_config_store_test_{}_{}.toml",
            name,
            std::process::id()
        ));
        path
    }

    #[test]
    fn normalizes_and_persists_choices() {
        let path = fixture_path("normalize");
        fs::write(
            &path,
            r#"
[app]
copy_debounce_sec = -1

[[sections]]
name = "prompt"

  [[sections.items]]
  key = "subject"
  choices = ["robot", "", "指定なし", "robot", "cat"]
"#,
        )
        .expect("fixture write");

        let mut store = ConfigStore::new(path.clone()).expect("load store");
        let items = store.get_items("prompt");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].choices[0], NO_SELECTION);
        assert_eq!(items[0].choices[1], "robot");
        assert_eq!(items[0].choices[2], "cat");

        let added = store
            .add_choice("prompt", "subject", "wolf")
            .expect("add choice");
        assert!(added);

        let removed = store
            .remove_choice("prompt", "subject", "cat")
            .expect("remove choice");
        assert!(removed);

        let items2 = store.get_items("prompt");
        assert_eq!(items2[0].choices, vec!["指定なし", "robot", "wolf"]);

        fs::remove_file(path).ok();
    }

    #[test]
    fn keeps_app_table_before_sections_after_save() {
        let path = fixture_path("app_order");
        fs::write(
            &path,
            r#"
[[sections]]
name = "prompt"

  [[sections.items]]
  key = "subject"
  choices = ["指定なし", "robot"]

[app]
history_server_port = 3000
"#,
        )
        .expect("fixture write");

        let mut store = ConfigStore::new(path.clone()).expect("load store");
        store
            .set_item_state("prompt", "subject", NO_SELECTION, "")
            .expect("set state");

        let saved = fs::read_to_string(&path).expect("read saved");
        let app_pos = saved.find("[app]").expect("app exists");
        let sections_pos = saved.find("[[sections]]").expect("sections exists");
        assert!(
            app_pos < sections_pos,
            "[app] should be before [[sections]] after save"
        );

        fs::remove_file(path).ok();
    }
}
