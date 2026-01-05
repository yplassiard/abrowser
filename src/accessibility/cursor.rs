//! Virtual cursor for navigating the accessibility tree.
//!
//! Provides screen reader-like navigation through content.

use super::{AXTree, Role};

/// Navigation direction
#[derive(Clone, Copy, Debug)]
pub enum Direction {
    Forward,
    Backward,
}

/// Navigation granularity
#[derive(Clone, Copy, Debug)]
pub enum Granularity {
    /// Move by individual element
    Element,
    /// Move by heading
    Heading,
    /// Move by landmark
    Landmark,
    /// Move by link
    Link,
    /// Move by form control
    FormControl,
    /// Move by list
    List,
    /// Move by table
    Table,
}

/// Virtual cursor for navigating content
pub struct VirtualCursor {
    /// Current position (node ID)
    current_id: Option<String>,

    /// Linearized view of the tree
    elements: Vec<String>,

    /// Current index in elements
    current_index: usize,
}

impl VirtualCursor {
    pub fn new() -> Self {
        Self {
            current_id: None,
            elements: Vec::new(),
            current_index: 0,
        }
    }

    /// Update the cursor with a new tree
    pub fn update(&mut self, tree: &AXTree) {
        self.elements = tree
            .linearize()
            .into_iter()
            .map(|n| n.id.clone())
            .collect();

        // Try to maintain position
        if let Some(id) = &self.current_id {
            if let Some(idx) = self.elements.iter().position(|e| e == id) {
                self.current_index = idx;
            } else {
                self.current_index = 0;
                self.current_id = self.elements.first().cloned();
            }
        } else {
            self.current_index = 0;
            self.current_id = self.elements.first().cloned();
        }
    }

    /// Get current node ID
    pub fn current(&self) -> Option<&str> {
        self.current_id.as_deref()
    }

    /// Move by element
    pub fn move_by_element(&mut self, direction: Direction) -> Option<&str> {
        if self.elements.is_empty() {
            return None;
        }

        match direction {
            Direction::Forward => {
                if self.current_index + 1 < self.elements.len() {
                    self.current_index += 1;
                }
            }
            Direction::Backward => {
                if self.current_index > 0 {
                    self.current_index -= 1;
                }
            }
        }

        self.current_id = self.elements.get(self.current_index).cloned();
        self.current_id.as_deref()
    }

    /// Move to next/previous element of specific type
    pub fn move_by_role(&mut self, tree: &AXTree, roles: &[Role], direction: Direction) -> Option<&str> {
        let start = self.current_index;
        let len = self.elements.len();

        if len == 0 {
            return None;
        }

        let mut idx = start;

        loop {
            match direction {
                Direction::Forward => {
                    idx = (idx + 1) % len;
                }
                Direction::Backward => {
                    idx = (idx + len - 1) % len;
                }
            }

            // Wrapped around without finding
            if idx == start {
                return None;
            }

            if let Some(id) = self.elements.get(idx) {
                if let Some(node) = tree.get(id) {
                    if roles.contains(&node.role) {
                        self.current_index = idx;
                        self.current_id = Some(id.clone());
                        return self.current_id.as_deref();
                    }
                }
            }
        }
    }

    /// Move by granularity
    pub fn move_by(&mut self, tree: &AXTree, granularity: Granularity, direction: Direction) -> Option<&str> {
        match granularity {
            Granularity::Element => self.move_by_element(direction),
            Granularity::Heading => {
                self.move_by_role(tree, &[Role::Heading], direction)
            }
            Granularity::Landmark => {
                self.move_by_role(tree, &[
                    Role::Banner,
                    Role::Navigation,
                    Role::Main,
                    Role::ContentInfo,
                    Role::Complementary,
                    Role::Search,
                    Role::Form,
                ], direction)
            }
            Granularity::Link => {
                self.move_by_role(tree, &[Role::Link], direction)
            }
            Granularity::FormControl => {
                self.move_by_role(tree, &[
                    Role::TextField,
                    Role::TextFieldMultiLine,
                    Role::CheckBox,
                    Role::RadioButton,
                    Role::ComboBox,
                    Role::ListBox,
                    Role::Button,
                ], direction)
            }
            Granularity::List => {
                self.move_by_role(tree, &[Role::List], direction)
            }
            Granularity::Table => {
                self.move_by_role(tree, &[Role::Table], direction)
            }
        }
    }

    /// Move to first element
    pub fn move_to_start(&mut self) -> Option<&str> {
        if !self.elements.is_empty() {
            self.current_index = 0;
            self.current_id = self.elements.first().cloned();
        }
        self.current_id.as_deref()
    }

    /// Move to last element
    pub fn move_to_end(&mut self) -> Option<&str> {
        if !self.elements.is_empty() {
            self.current_index = self.elements.len() - 1;
            self.current_id = self.elements.last().cloned();
        }
        self.current_id.as_deref()
    }
}

impl Default for VirtualCursor {
    fn default() -> Self {
        Self::new()
    }
}
