use crate::value::Value;

use super::model::{NodeType, TreeNode};

pub const BUILTIN_PACK: &str = "mermaid-treeview";

pub const FOLDER_GLYPH: &str =
    "M3 6.5A1.5 1.5 0 0 1 4.5 5h4.6l2.4 2.5h8A1.5 1.5 0 0 1 21 9v9a1.5 1.5 0 0 1-1.5 1.5h-15A1.5 1.5 0 0 1 3 18z";
pub const FILE_GLYPH: &str =
    "M6.5 3h7L19 8.5v11a1.5 1.5 0 0 1-1.5 1.5h-11A1.5 1.5 0 0 1 5 19.5v-15A1.5 1.5 0 0 1 6.5 3zM13.5 3v5.5H19";

pub fn glyph(name: &str) -> Option<&'static str> {
    match name {
        "folder" => Some(FOLDER_GLYPH),
        "file" => Some(FILE_GLYPH),
        _ => None,
    }
}

pub struct IconConfig<'a> {
    pub show_icons: bool,
    pub default_icon_pack: String,
    pub filename_icons: Option<&'a Value>,
    pub extension_icons: Option<&'a Value>,
}

fn own<'a>(map: Option<&'a Value>, key: &str) -> Option<&'a str> {
    let m = map?;
    if !m.has(key) {
        return None;
    }
    m.get(key).as_str()
}

pub fn detect_icon(name: &str, config: &IconConfig) -> Option<String> {
    if let Some(by_name) = own(config.filename_icons, name) {
        if !by_name.is_empty() {
            return Some(by_name.to_string());
        }
    }
    if let Some(dot) = name.rfind('.').filter(|&d| d > 0) {
        let ext = name[dot..].to_lowercase();
        if let Some(found) = own(config.extension_icons, &ext).or_else(|| own(config.extension_icons, &ext[1..])) {
            return Some(found.to_string());
        }
    }
    None
}

fn qualify_icon(icon: &str, default_icon_pack: &str) -> String {
    if icon.contains(':') {
        return icon.to_string();
    }
    if glyph(icon).is_some() || default_icon_pack.is_empty() {
        return format!("{}:{}", BUILTIN_PACK, icon);
    }
    format!("{}:{}", default_icon_pack, icon)
}

pub fn get_node_icon(node: &TreeNode, config: &IconConfig) -> Option<String> {
    if let Some(ref ic) = node.icon {
        if ic == "none" {
            return None;
        }
        if !ic.is_empty() {
            return Some(qualify_icon(ic, &config.default_icon_pack));
        }
    }
    if !config.show_icons {
        return None;
    }
    if node.node_type == NodeType::File {
        if let Some(detected) = detect_icon(&node.name, config) {
            if detected == "none" {
                return None;
            }
            if !detected.is_empty() {
                return Some(qualify_icon(&detected, &config.default_icon_pack));
            }
        }
    }
    Some(format!(
        "{}:{}",
        BUILTIN_PACK,
        if node.node_type == NodeType::Directory { "folder" } else { "file" }
    ))
}

pub fn icon_config(config: &Value) -> IconConfig<'_> {
    let c = config.get("treeView");
    let filename_icons = c.get("filenameIcons");
    let extension_icons = c.get("extensionIcons");
    IconConfig {
        show_icons: c.get("showIcons").truthy(),
        default_icon_pack: c.get("defaultIconPack").as_str().unwrap_or("").to_string(),
        filename_icons: filename_icons.is_object().then_some(filename_icons),
        extension_icons: extension_icons.is_object().then_some(extension_icons),
    }
}
