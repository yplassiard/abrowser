//! Accessibility tree management.
//!
//! Stores and manages the accessibility tree received from the browser backend.

use std::collections::HashMap;

use super::{AXNode, Role};

/// The accessibility tree for a document
#[derive(Debug, Default)]
pub struct AXTree {
    /// All nodes indexed by ID
    nodes: HashMap<String, AXNode>,

    /// Root node ID
    root_id: Option<String>,

    /// Currently focused node ID
    focus_id: Option<String>,
}

impl AXTree {
    pub fn new() -> Self {
        Self::default()
    }

    /// Update the tree with new nodes
    pub fn update(&mut self, nodes: Vec<AXNode>, root_id: impl Into<String>) {
        self.nodes.clear();
        self.root_id = Some(root_id.into());

        for node in nodes {
            self.nodes.insert(node.id.clone(), node);
        }

        // Populate contains_role for nodes that contain interactive children
        self.populate_contained_roles();

        // Derive names for links/buttons that have no name but contain images
        self.derive_names_from_children();
    }

    /// For links/buttons without names, try to derive name from child images or text
    fn derive_names_from_children(&mut self) {
        // Find links/buttons with empty names
        let unnamed: Vec<String> = self
            .nodes
            .values()
            .filter(|n| {
                n.name.is_empty()
                    && matches!(n.role, Role::Link | Role::Button)
                    && !n.child_ids.is_empty()
            })
            .map(|n| n.id.clone())
            .collect();

        // For each, try to find a name from children
        for id in unnamed {
            if let Some(derived_name) = self.find_child_name(&id) {
                if let Some(node) = self.nodes.get_mut(&id) {
                    node.name = derived_name;
                }
            }
        }
    }

    /// Recursively find a name from child nodes (images, text, etc.)
    fn find_child_name(&self, id: &str) -> Option<String> {
        let node = self.nodes.get(id)?;

        for child_id in &node.child_ids {
            if let Some(child) = self.nodes.get(child_id) {
                // Image with name (alt text)
                if matches!(child.role, Role::Image) && !child.name.is_empty() {
                    return Some(child.name.clone());
                }
                // Static text
                if matches!(child.role, Role::StaticText) && !child.name.is_empty() {
                    return Some(child.name.clone());
                }
                // Image with URL - extract filename
                if matches!(child.role, Role::Image) && child.name.is_empty() {
                    if let Some(ref url) = child.url {
                        if let Some(filename) = url.rsplit('/').next() {
                            let name = filename
                                .split('?').next().unwrap_or(filename)
                                .trim_end_matches(".png")
                                .trim_end_matches(".jpg")
                                .trim_end_matches(".jpeg")
                                .trim_end_matches(".gif")
                                .trim_end_matches(".svg")
                                .trim_end_matches(".webp")
                                .replace('-', " ")
                                .replace('_', " ");
                            if !name.is_empty() && name.len() > 2 {
                                return Some(format!("[img: {}]", name));
                            }
                        }
                    }
                }
                // Recurse into children
                if let Some(name) = self.find_child_name(child_id) {
                    return Some(name);
                }
            }
        }
        None
    }

    /// Find headings that contain interactive elements and set their contains_role
    fn populate_contained_roles(&mut self) {
        // Collect IDs and names of headings
        let headings: Vec<(String, String)> = self
            .nodes
            .values()
            .filter(|n| matches!(n.role, Role::Heading))
            .map(|n| (n.id.clone(), n.name.clone()))
            .collect();

        // For each heading, check and collect results
        let mut updates: Vec<(String, Role)> = Vec::new();
        for (id, name) in &headings {
            if let Some(node) = self.nodes.get(id) {
                if let Some(role) = self.find_contained_role_recursive(node, name) {
                    updates.push((id.clone(), role));
                }
            }
        }

        // Apply updates
        for (id, role) in updates {
            if let Some(node) = self.nodes.get_mut(&id) {
                node.contains_role = Some(role);
            }
        }
    }

    /// Get the root node
    pub fn root(&self) -> Option<&AXNode> {
        self.root_id.as_ref().and_then(|id| self.nodes.get(id))
    }

    /// Get a node by ID
    pub fn get(&self, id: &str) -> Option<&AXNode> {
        self.nodes.get(id)
    }

    /// Get the focused node
    pub fn focused(&self) -> Option<&AXNode> {
        self.focus_id.as_ref().and_then(|id| self.nodes.get(id))
    }

    /// Set focus to a node
    pub fn set_focus(&mut self, id: impl Into<String>) {
        let id = id.into();
        if self.nodes.contains_key(&id) {
            self.focus_id = Some(id);
        }
    }

    /// Get children of a node
    pub fn children(&self, id: &str) -> Vec<&AXNode> {
        self.nodes
            .get(id)
            .map(|node| {
                node.child_ids
                    .iter()
                    .filter_map(|child_id| self.nodes.get(child_id))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Get parent of a node
    pub fn parent(&self, id: &str) -> Option<&AXNode> {
        self.nodes
            .get(id)
            .and_then(|node| node.parent_id.as_ref())
            .and_then(|parent_id| self.nodes.get(parent_id))
    }

    /// Get the number of nodes in the tree
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    /// Check if the tree is empty
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// Linearize the tree for output (depth-first traversal)
    pub fn linearize(&self) -> Vec<&AXNode> {
        let mut result = Vec::new();

        if let Some(root_id) = &self.root_id {
            self.linearize_node(root_id, &mut result, None);
        }

        result
    }

    fn linearize_node<'a>(&'a self, id: &str, result: &mut Vec<&'a AXNode>, parent_name: Option<&str>) {
        if let Some(node) = self.nodes.get(id) {
            let dominated_by_parent = if let Some(pname) = parent_name {
                let pname = pname.trim();
                let nname = node.name.trim();

                if nname.is_empty() {
                    false
                } else {
                    // Check if names overlap significantly (one contains the other)
                    pname.contains(nname) || nname.contains(pname)
                }
            } else {
                false
            };

            // Only include interesting nodes that aren't dominated by parent
            if node.is_interesting() && !dominated_by_parent {
                result.push(node);
            }

            // Pass current node's name to children if it's interesting and has a name
            let child_parent_name = if node.is_interesting() && !node.name.is_empty() {
                Some(node.name.as_str())
            } else {
                parent_name
            };

            // Process children
            for child_id in &node.child_ids {
                self.linearize_node(child_id, result, child_parent_name);
            }
        }
    }

    /// Check if a node contains an interactive child with similar name
    /// Returns the role of the contained interactive element if found
    pub fn find_contained_interactive_role(&self, node: &AXNode) -> Option<Role> {
        self.find_contained_role_recursive(node, &node.name)
    }

    fn find_contained_role_recursive(&self, node: &AXNode, parent_name: &str) -> Option<Role> {
        for child_id in &node.child_ids {
            if let Some(child) = self.nodes.get(child_id) {
                // Check if this child is interactive with similar name
                if child.is_interactive() {
                    let pname = parent_name.trim();
                    let cname = child.name.trim();
                    if !cname.is_empty() && (pname.contains(cname) || cname.contains(pname)) {
                        return Some(child.role.clone());
                    }
                }
                // Recurse into children
                if let Some(role) = self.find_contained_role_recursive(child, parent_name) {
                    return Some(role);
                }
            }
        }
        None
    }

    /// Get all interactive elements for Tab navigation
    pub fn interactive_elements(&self) -> Vec<&AXNode> {
        self.linearize()
            .into_iter()
            .filter(|node| node.is_interactive())
            .collect()
    }

    /// Find next/previous interactive element
    pub fn find_next_interactive(&self, current_id: Option<&str>, forward: bool) -> Option<&AXNode> {
        let elements = self.interactive_elements();

        if elements.is_empty() {
            return None;
        }

        let current_idx = current_id
            .and_then(|id| elements.iter().position(|n| n.id == id));

        match (current_idx, forward) {
            (Some(idx), true) => elements.get((idx + 1) % elements.len()).copied(),
            (Some(idx), false) => elements.get((idx + elements.len() - 1) % elements.len()).copied(),
            (None, true) => elements.first().copied(),
            (None, false) => elements.last().copied(),
        }
    }
}
