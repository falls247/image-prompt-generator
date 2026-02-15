use serde::Serialize;

use crate::NO_SELECTION;

#[derive(Debug, Clone, Serialize)]
pub struct RenderEntry {
    pub label: String,
    pub selected: String,
    pub free_text: String,
}

pub fn render_prompt(entries: &[RenderEntry]) -> String {
    let mut parts = Vec::new();
    for entry in entries {
        let free_text = entry.free_text.trim();
        let selected = entry.selected.trim();
        let value = if free_text.is_empty() {
            selected
        } else {
            free_text
        };
        if value.is_empty() || value == NO_SELECTION {
            continue;
        }
        parts.push(format!("[{}]：{}", entry.label, value));
    }
    parts.join("\n")
}

#[cfg(test)]
mod tests {
    use super::{render_prompt, RenderEntry};

    #[test]
    fn render_uses_confirmed_free_text() {
        let out = render_prompt(&[
            RenderEntry {
                label: "被写体".to_string(),
                selected: "ロボット".to_string(),
                free_text: "青いロボット".to_string(),
            },
            RenderEntry {
                label: "向き".to_string(),
                selected: "指定なし".to_string(),
                free_text: "".to_string(),
            },
        ]);
        assert_eq!(out, "[被写体]：青いロボット");
    }
}
