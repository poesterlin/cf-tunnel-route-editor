use similar::{ChangeTag, TextDiff};

use crate::config::model::TunnelConfig;

/// Generate a human-readable diff between two tunnel configurations
pub fn diff_configs(old: &TunnelConfig, new: &TunnelConfig) -> String {
    let old_str = serde_json::to_string_pretty(&old.raw).unwrap_or_else(|_| "{}".to_string());
    let new_str = serde_json::to_string_pretty(&new.raw).unwrap_or_else(|_| "{}".to_string());

    let diff = TextDiff::from_lines(&old_str, &new_str);

    let mut output = String::new();
    for change in diff.iter_all_changes() {
        let sign = match change.tag() {
            ChangeTag::Delete => "-",
            ChangeTag::Insert => "+",
            ChangeTag::Equal => " ",
        };
        output.push_str(&format!("{}{}", sign, change));
    }

    output
}

/// Generate a semantic diff focused on ingress rule changes
pub fn diff_ingress_rules(old: &TunnelConfig, new: &TunnelConfig) -> String {
    let old_rules = old.ingress_rules();
    let new_rules = new.ingress_rules();

    let mut output = String::new();

    if old_rules.len() != new_rules.len() {
        output.push_str(&format!(
            "rule count: {} -> {}\n",
            old_rules.len(),
            new_rules.len()
        ));
    }

    let max_len = old_rules.len().max(new_rules.len());
    for i in 0..max_len {
        let old_rule = old_rules.get(i);
        let new_rule = new_rules.get(i);

        match (old_rule, new_rule) {
            (Some(old), Some(new)) => {
                let old_json = serde_json::to_string(&old.raw).unwrap_or_default();
                let new_json = serde_json::to_string(&new.raw).unwrap_or_default();
                if old_json != new_json {
                    let host = new.hostname.as_deref().unwrap_or("(catch-all)");
                    output.push_str(&format!("\n--- rule {i}: {host}\n"));
                    let old_repr = serde_json::to_string_pretty(&old.raw).unwrap_or_default();
                    let new_repr = serde_json::to_string_pretty(&new.raw).unwrap_or_default();
                    let diff = TextDiff::from_lines(&old_repr, &new_repr);
                    for change in diff.iter_all_changes() {
                        let sign = match change.tag() {
                            ChangeTag::Delete => "-",
                            ChangeTag::Insert => "+",
                            ChangeTag::Equal => " ",
                        };
                        output.push_str(&format!("{}{}", sign, change));
                    }
                }
            }
            (None, Some(new)) => {
                let host = new.hostname.as_deref().unwrap_or("(catch-all)");
                output.push_str(&format!("\n+ added rule at index {i}: {host}\n"));
                output.push_str(&serde_json::to_string_pretty(&new.raw).unwrap_or_default());
                output.push('\n');
            }
            (Some(old), None) => {
                let host = old.hostname.as_deref().unwrap_or("(catch-all)");
                output.push_str(&format!("\n- removed rule at index {i}: {host}\n"));
            }
            (None, None) => unreachable!(),
        }
    }

    output
}
