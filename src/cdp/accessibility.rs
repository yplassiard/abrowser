// CDP Accessibility domain types and methods

use super::CdpClient;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;

/// Accessibility node role
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum AXRole {
    // Document structure
    RootWebArea,
    Document,
    Article,
    Region,
    Navigation,
    Main,
    Complementary,
    ContentInfo,
    Banner,
    Search,
    Form,

    // Headings
    Heading,

    // Text
    StaticText,
    InlineTextBox,
    Paragraph,

    // Interactive
    Link,
    Button,
    TextField,
    TextArea,
    CheckBox,
    RadioButton,
    ComboBox,
    ListBox,
    Option,
    Menu,
    MenuItem,
    MenuBar,

    // Lists
    List,
    ListItem,
    DescriptionList,
    DescriptionListTerm,
    DescriptionListDetail,

    // Tables
    Table,
    Row,
    Cell,
    ColumnHeader,
    RowHeader,

    // Other
    Image,
    Figure,
    FigCaption,
    Group,
    Generic,

    // Unknown/other
    #[serde(other)]
    Unknown,
}

/// Accessibility node from CDP
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AXNode {
    pub node_id: String,
    #[serde(default)]
    pub ignored: bool,
    #[serde(default)]
    pub ignored_reasons: Vec<AXProperty>,
    pub role: Option<AXValue>,
    pub name: Option<AXValue>,
    pub description: Option<AXValue>,
    pub value: Option<AXValue>,
    #[serde(default)]
    pub properties: Vec<AXProperty>,
    #[serde(default)]
    pub child_ids: Vec<String>,
    pub parent_id: Option<String>,
    #[serde(rename = "backendDOMNodeId")]
    pub backend_dom_node_id: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AXValue {
    #[serde(rename = "type")]
    pub value_type: String,
    pub value: Option<Value>,
    #[serde(default)]
    pub related_nodes: Vec<AXRelatedNode>,
    #[serde(default)]
    pub sources: Vec<AXValueSource>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AXProperty {
    pub name: String,
    pub value: AXValue,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AXRelatedNode {
    #[serde(default)]
    pub backend_dom_node_id: Option<i64>,
    pub text: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AXValueSource {
    #[serde(rename = "type")]
    pub source_type: String,
    pub value: Option<AXValue>,
}

impl AXNode {
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
}

/// Accessibility tree built from CDP nodes
pub struct AXTree {
    nodes: HashMap<String, AXNode>,
    root_id: Option<String>,
}

impl AXTree {
    pub fn new() -> Self {
        Self {
            nodes: HashMap::new(),
            root_id: None,
        }
    }

    pub fn from_nodes(nodes: Vec<AXNode>) -> Self {
        let mut tree = Self::new();
        for node in nodes {
            if tree.root_id.is_none() && node.parent_id.is_none() {
                tree.root_id = Some(node.node_id.clone());
            }
            tree.nodes.insert(node.node_id.clone(), node);
        }
        tree
    }

    pub fn root(&self) -> Option<&AXNode> {
        self.root_id.as_ref().and_then(|id| self.nodes.get(id))
    }

    pub fn get(&self, node_id: &str) -> Option<&AXNode> {
        self.nodes.get(node_id)
    }

    pub fn children(&self, node: &AXNode) -> Vec<&AXNode> {
        node.child_ids
            .iter()
            .filter_map(|id| self.nodes.get(id))
            .collect()
    }

    /// Iterate over all non-ignored nodes in document order
    pub fn iter_visible(&self) -> impl Iterator<Item = &AXNode> {
        self.nodes.values().filter(|n| !n.ignored)
    }

    /// Get a linearized list of all visible, meaningful nodes
    pub fn linearize(&self) -> Vec<&AXNode> {
        let mut result = Vec::new();
        if let Some(root) = self.root() {
            self.linearize_recursive(root, &mut result);
        }
        result
    }

    fn linearize_recursive<'a>(&'a self, node: &'a AXNode, result: &mut Vec<&'a AXNode>) {
        // Include this node if it's not ignored and has meaningful content
        if !node.ignored {
            let role = node.role_str();
            let name = node.name_str();

            // Skip nodes that are just duplicating parent/ancestor content
            let is_meaningful = match role {
                // Skip structural/container roles
                "RootWebArea" | "document" | "generic" | "group" | "none" => false,
                // Skip inline text boxes (granular text segments)
                "InlineTextBox" | "inlineTextBox" => false,
                // Skip StaticText if any ancestor already has the same name
                "StaticText" | "staticText" => {
                    !name.is_empty() && !self.ancestor_has_name(node, name)
                }
                // For other roles, skip if parent is a clickable element with same name
                _ => {
                    if name.is_empty() {
                        true // Keep elements without names (might have other content)
                    } else if let Some(parent_id) = &node.parent_id {
                        if let Some(parent) = self.get(parent_id) {
                            let parent_role = parent.role_str().to_lowercase();
                            // Skip if parent is interactive and has same name
                            let parent_is_interactive = matches!(
                                parent_role.as_str(),
                                "link" | "button" | "menuitem" | "option" | "tab" |
                                "checkbox" | "radio" | "switch" | "combobox"
                            );
                            !(parent_is_interactive && parent.name_str() == name)
                        } else {
                            true
                        }
                    } else {
                        true
                    }
                }
            };

            if is_meaningful {
                result.push(node);
            }
        }

        // Always recurse into children (even for ignored nodes, they may have meaningful descendants)
        for child in self.children(node) {
            self.linearize_recursive(child, result);
        }
    }

    /// Check if any ancestor has the given name
    fn ancestor_has_name(&self, node: &AXNode, name: &str) -> bool {
        let mut current_id = node.parent_id.clone();
        while let Some(ref id) = current_id {
            if let Some(ancestor) = self.get(id) {
                if ancestor.name_str() == name {
                    return true;
                }
                current_id = ancestor.parent_id.clone();
            } else {
                break;
            }
        }
        false
    }
}

impl Default for AXTree {
    fn default() -> Self {
        Self::new()
    }
}

/// CDP Accessibility domain methods
pub struct AccessibilityDomain<'a> {
    client: &'a CdpClient,
}

impl<'a> AccessibilityDomain<'a> {
    pub fn new(client: &'a CdpClient) -> Self {
        Self { client }
    }

    /// Enable the Accessibility domain
    pub async fn enable(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.client.call("Accessibility.enable", json!({})).await?;
        Ok(())
    }

    /// Disable the Accessibility domain
    pub async fn disable(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.client.call("Accessibility.disable", json!({})).await?;
        Ok(())
    }

    /// Get the full accessibility tree
    pub async fn get_full_tree(&self) -> Result<AXTree, Box<dyn std::error::Error + Send + Sync>> {
        let result = self.client.call("Accessibility.getFullAXTree", json!({})).await?;

        let nodes: Vec<AXNode> = serde_json::from_value(
            result.get("nodes").cloned().unwrap_or(Value::Array(vec![]))
        )?;

        Ok(AXTree::from_nodes(nodes))
    }

    /// Get the root accessibility node
    pub async fn get_root_node(&self) -> Result<Option<AXNode>, Box<dyn std::error::Error + Send + Sync>> {
        let result = self.client.call("Accessibility.getRootAXNode", json!({})).await?;

        if let Some(node_value) = result.get("node") {
            let node: AXNode = serde_json::from_value(node_value.clone())?;
            Ok(Some(node))
        } else {
            Ok(None)
        }
    }

    /// Query accessibility nodes based on criteria
    pub async fn query_nodes(
        &self,
        accessible_name: Option<&str>,
        role: Option<&str>,
    ) -> Result<Vec<AXNode>, Box<dyn std::error::Error + Send + Sync>> {
        let mut params = json!({});

        if let Some(name) = accessible_name {
            params["accessibleName"] = json!(name);
        }
        if let Some(r) = role {
            params["role"] = json!(r);
        }

        let result = self.client.call("Accessibility.queryAXTree", params).await?;

        let nodes: Vec<AXNode> = serde_json::from_value(
            result.get("nodes").cloned().unwrap_or(Value::Array(vec![]))
        )?;

        Ok(nodes)
    }
}
