//! CDP-specific accessibility types for deserialization.
//!
//! These types are used to deserialize CDP accessibility responses.
//! They are then converted to the unified `accessibility::AXNode` type.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::accessibility::{AXNode as UnifiedAXNode, NodeState, Role};
use crate::backend::{NodeHandle, NodeHandleInner};

/// CDP Accessibility node (for deserialization)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CdpAXNode {
    pub node_id: String,
    #[serde(default)]
    pub ignored: bool,
    #[serde(default)]
    pub ignored_reasons: Vec<CdpAXProperty>,
    pub role: Option<CdpAXValue>,
    pub name: Option<CdpAXValue>,
    pub description: Option<CdpAXValue>,
    pub value: Option<CdpAXValue>,
    #[serde(default)]
    pub properties: Vec<CdpAXProperty>,
    #[serde(default)]
    pub child_ids: Vec<String>,
    pub parent_id: Option<String>,
    #[serde(rename = "backendDOMNodeId")]
    pub backend_dom_node_id: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CdpAXValue {
    #[serde(rename = "type")]
    pub value_type: String,
    pub value: Option<Value>,
    #[serde(default)]
    pub related_nodes: Vec<CdpAXRelatedNode>,
    #[serde(default)]
    pub sources: Vec<CdpAXValueSource>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CdpAXProperty {
    pub name: String,
    pub value: CdpAXValue,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CdpAXRelatedNode {
    #[serde(default)]
    pub backend_dom_node_id: Option<i64>,
    pub text: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CdpAXValueSource {
    #[serde(rename = "type")]
    pub source_type: String,
    pub value: Option<CdpAXValue>,
}

impl CdpAXNode {
    /// Get the role as a string
    pub fn role_str(&self) -> &str {
        self.role
            .as_ref()
            .and_then(|r| r.value.as_ref())
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
    }

    /// Get the accessible name
    pub fn name_str(&self) -> &str {
        self.name
            .as_ref()
            .and_then(|n| n.value.as_ref())
            .and_then(|v| v.as_str())
            .unwrap_or("")
    }

    /// Get the accessible description
    pub fn description_str(&self) -> &str {
        self.description
            .as_ref()
            .and_then(|d| d.value.as_ref())
            .and_then(|v| v.as_str())
            .unwrap_or("")
    }

    /// Get the value
    pub fn value_str(&self) -> &str {
        self.value
            .as_ref()
            .and_then(|v| v.value.as_ref())
            .and_then(|v| v.as_str())
            .unwrap_or("")
    }

    /// Get a property value by name
    pub fn get_property(&self, name: &str) -> Option<&Value> {
        self.properties
            .iter()
            .find(|p| p.name == name)
            .and_then(|p| p.value.value.as_ref())
    }

    /// Check if node is focusable
    pub fn is_focusable(&self) -> bool {
        self.get_property("focusable")
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
    }

    /// Check if node is focused
    pub fn is_focused(&self) -> bool {
        self.get_property("focused")
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
    }

    /// Get the heading level (1-6) if this is a heading
    pub fn heading_level(&self) -> Option<u8> {
        if self.role_str() == "heading" {
            self.get_property("level")
                .and_then(|v| v.as_u64())
                .map(|l| l as u8)
        } else {
            None
        }
    }

    /// Convert to unified AXNode
    pub fn to_unified(&self) -> UnifiedAXNode {
        // Ignored nodes get Generic role so they're included in tree but not shown
        let role = if self.ignored {
            Role::Generic
        } else {
            Self::parse_role(self.role_str())
        };
        let handle = self.backend_dom_node_id.map(|id| NodeHandle {
            inner: NodeHandleInner::Chromium {
                backend_dom_node_id: id,
                node_id: self.node_id.clone(),
            },
        });

        let mut state = NodeState::default();
        state.focusable = self.is_focusable();
        state.focused = self.is_focused();
        state.selected = self
            .get_property("selected")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        state.checked = self.get_property("checked").and_then(|v| {
            if v.as_str() == Some("mixed") {
                None
            } else {
                v.as_bool()
            }
        });
        state.expanded = self.get_property("expanded").and_then(|v| v.as_bool());
        state.disabled = self
            .get_property("disabled")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        state.readonly = self
            .get_property("readonly")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        state.required = self
            .get_property("required")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        state.visited = self
            .get_property("visited")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        UnifiedAXNode {
            id: self.node_id.clone(),
            handle,
            role,
            name: self.name_str().to_string(),
            description: self.description_str().to_string(),
            value: self.value_str().to_string(),
            url: self
                .get_property("url")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
            level: self.heading_level().unwrap_or(0),
            state,
            parent_id: self.parent_id.clone(),
            child_ids: self.child_ids.clone(),
            pos_in_set: self
                .get_property("posinset")
                .and_then(|v| v.as_u64())
                .map(|n| n as u32),
            set_size: self
                .get_property("setsize")
                .and_then(|v| v.as_u64())
                .map(|n| n as u32),
            contains_role: None, // Populated later by tree.populate_contained_roles()
        }
    }

    /// Parse role string to Role enum
    fn parse_role(role_str: &str) -> Role {
        match role_str.to_lowercase().as_str() {
            "rootwebarea" | "document" => Role::Document,
            "article" => Role::Article,
            "region" | "section" => Role::Section,
            "heading" => Role::Heading,
            "paragraph" => Role::Paragraph,
            "statictext" => Role::StaticText,
            "link" => Role::Link,
            "button" => Role::Button,
            "textbox" | "textfield" | "searchbox" => Role::TextField,
            "textarea" => Role::TextFieldMultiLine,
            "checkbox" => Role::CheckBox,
            "radiobutton" | "radio" => Role::RadioButton,
            "combobox" => Role::ComboBox,
            "listbox" => Role::ListBox,
            "list" => Role::List,
            "listitem" => Role::ListItem,
            "table" => Role::Table,
            "row" => Role::Row,
            "cell" => Role::Cell,
            "columnheader" => Role::ColumnHeader,
            "rowheader" => Role::RowHeader,
            "banner" => Role::Banner,
            "navigation" => Role::Navigation,
            "main" => Role::Main,
            "contentinfo" => Role::ContentInfo,
            "complementary" => Role::Complementary,
            "search" => Role::Search,
            "form" => Role::Form,
            "image" | "img" => Role::Image,
            "figure" => Role::Figure,
            "group" => Role::Group,
            "generic" | "none" => Role::Generic,
            _ => Role::Unknown,
        }
    }
}
