//! Accessibility tree management.
//!
//! Stores and manages the accessibility tree received from Chromium.

use std::collections::HashMap;

use super::AXNode;

/// The accessibility tree for a document
#[derive(Debug, Default)]
pub struct AXTree {
    /// All nodes indexed by ID
    nodes: HashMap<i32, AXNode>,

    /// Root node ID
    root_id: Option<i32>,

    /// Currently focused node ID
    focus_id: Option<i32>,
}

impl AXTree {
    pub fn new() -> Self {
        Self::default()
    }

    /// Update the tree with new nodes
    pub fn update(&mut self, nodes: Vec<AXNode>, root_id: i32) {
        self.nodes.clear();
        self.root_id = Some(root_id);

        for node in nodes {
            self.nodes.insert(node.id, node);
        }
    }

    /// Get the root node
    pub fn root(&self) -> Option<&AXNode> {
        self.root_id.and_then(|id| self.nodes.get(&id))
    }

    /// Get a node by ID
    pub fn get(&self, id: i32) -> Option<&AXNode> {
        self.nodes.get(&id)
    }

    /// Get the focused node
    pub fn focused(&self) -> Option<&AXNode> {
        self.focus_id.and_then(|id| self.nodes.get(&id))
    }

    /// Set focus to a node
    pub fn set_focus(&mut self, id: i32) {
        if self.nodes.contains_key(&id) {
            self.focus_id = Some(id);
        }
    }

    /// Get children of a node
    pub fn children(&self, id: i32) -> Vec<&AXNode> {
        self.nodes
            .get(&id)
            .map(|node| {
                node.child_ids
                    .iter()
                    .filter_map(|child_id| self.nodes.get(child_id))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Get parent of a node
    pub fn parent(&self, id: i32) -> Option<&AXNode> {
        self.nodes
            .get(&id)
            .and_then(|node| node.parent_id)
            .and_then(|parent_id| self.nodes.get(&parent_id))
    }

    /// Linearize the tree for output (depth-first traversal)
    pub fn linearize(&self) -> Vec<&AXNode> {
        let mut result = Vec::new();

        if let Some(root_id) = self.root_id {
            self.linearize_node(root_id, &mut result);
        }

        result
    }

    fn linearize_node<'a>(&'a self, id: i32, result: &mut Vec<&'a AXNode>) {
        if let Some(node) = self.nodes.get(&id) {
            // Only include interesting nodes
            if node.is_interesting() {
                result.push(node);
            }

            // Process children
            for child_id in &node.child_ids {
                self.linearize_node(*child_id, result);
            }
        }
    }

    /// Get all interactive elements for Tab navigation
    pub fn interactive_elements(&self) -> Vec<&AXNode> {
        self.linearize()
            .into_iter()
            .filter(|node| node.is_interactive())
            .collect()
    }

    /// Find next/previous interactive element
    pub fn find_next_interactive(&self, current_id: Option<i32>, forward: bool) -> Option<&AXNode> {
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
