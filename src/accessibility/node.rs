//! Accessibility node representation.
//!
//! Unified accessibility node type that works with any browser backend.
//! Both Chromium (CDP) and Firefox (RDP) convert their native formats to this type.

use crate::backend::NodeHandle;

/// Accessibility roles (subset of ax::mojom::Role)
#[derive(Clone, Debug, PartialEq)]
pub enum Role {
    // Document structure
    Document,
    Article,
    Section,

    // Headings
    Heading,

    // Text content
    Paragraph,
    StaticText,

    // Interactive elements
    Link,
    Button,
    TextField,
    TextFieldMultiLine,
    CheckBox,
    RadioButton,
    ComboBox,
    ListBox,

    // Lists
    List,
    ListItem,

    // Tables
    Table,
    Row,
    Cell,
    ColumnHeader,
    RowHeader,

    // Landmarks
    Banner,
    Navigation,
    Main,
    ContentInfo,
    Complementary,
    Search,
    Form,

    // Media
    Image,
    Figure,

    // Grouping
    Group,
    Generic,

    // Other
    Unknown,
}

impl Default for Role {
    fn default() -> Self {
        Role::Unknown
    }
}

/// State flags for accessibility nodes
#[derive(Clone, Debug, Default)]
pub struct NodeState {
    pub focusable: bool,
    pub focused: bool,
    pub selected: bool,
    pub checked: Option<bool>, // None = not checkable, Some(bool) = checked state
    pub expanded: Option<bool>,
    pub disabled: bool,
    pub readonly: bool,
    pub required: bool,
    pub visited: bool, // For links
}

/// An accessibility node from any browser backend.
///
/// This is the unified node type used throughout the application.
/// Backend-specific node formats (CDP AXNode, Firefox accessible) are
/// converted to this type.
#[derive(Clone, Debug, Default)]
pub struct AXNode {
    /// Unique string identifier (works for both CDP node IDs and Firefox actor IDs)
    pub id: String,

    /// Backend-specific handle for operations (click, focus, etc.)
    /// This is used when interacting with the browser (e.g., clicking an element).
    pub handle: Option<NodeHandle>,

    /// Semantic role
    pub role: Role,

    /// Accessible name (visible or computed)
    pub name: String,

    /// Description (aria-describedby, title, etc.)
    pub description: String,

    /// Value (for form controls, etc.)
    pub value: String,

    /// URL (for links)
    pub url: Option<String>,

    /// Heading level (1-6, 0 if not a heading)
    pub level: u8,

    /// State flags
    pub state: NodeState,

    /// Parent node ID
    pub parent_id: Option<String>,

    /// Child node IDs
    pub child_ids: Vec<String>,

    /// Position in set (e.g., list item 3 of 5)
    pub pos_in_set: Option<u32>,
    pub set_size: Option<u32>,

    /// Secondary role when this node effectively contains another role
    /// e.g., a heading that contains a link: display as "# [link text]"
    pub contains_role: Option<Role>,

    /// Handle of the contained interactive element (for activation when h_index > 0)
    pub contained_handle: Option<NodeHandle>,
}

impl AXNode {
    /// Create a new node with the given ID
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            ..Default::default()
        }
    }

    /// Create a new node with ID and handle
    pub fn with_handle(id: impl Into<String>, handle: NodeHandle) -> Self {
        Self {
            id: id.into(),
            handle: Some(handle),
            ..Default::default()
        }
    }

    /// Get the role as a string (for compatibility)
    pub fn role_str(&self) -> &'static str {
        match self.role {
            Role::Document => "document",
            Role::Article => "article",
            Role::Section => "section",
            Role::Heading => "heading",
            Role::Paragraph => "paragraph",
            Role::StaticText => "statictext",
            Role::Link => "link",
            Role::Button => "button",
            Role::TextField => "textbox",
            Role::TextFieldMultiLine => "textarea",
            Role::CheckBox => "checkbox",
            Role::RadioButton => "radiobutton",
            Role::ComboBox => "combobox",
            Role::ListBox => "listbox",
            Role::List => "list",
            Role::ListItem => "listitem",
            Role::Table => "table",
            Role::Row => "row",
            Role::Cell => "cell",
            Role::ColumnHeader => "columnheader",
            Role::RowHeader => "rowheader",
            Role::Banner => "banner",
            Role::Navigation => "navigation",
            Role::Main => "main",
            Role::ContentInfo => "contentinfo",
            Role::Complementary => "complementary",
            Role::Search => "search",
            Role::Form => "form",
            Role::Image => "image",
            Role::Figure => "figure",
            Role::Group => "group",
            Role::Generic => "generic",
            Role::Unknown => "unknown",
        }
    }

    /// Get the accessible name (for compatibility)
    pub fn name_str(&self) -> &str {
        &self.name
    }

    /// Get the backend DOM node ID (for CDP compatibility)
    /// Returns None if no handle or not a Chromium handle
    pub fn backend_dom_node_id(&self) -> Option<i64> {
        use crate::backend::NodeHandleInner;
        self.handle.as_ref().and_then(|h| match &h.inner {
            NodeHandleInner::Chromium { backend_dom_node_id, .. } => Some(*backend_dom_node_id),
            _ => None,
        })
    }

    /// Check if this node should be announced/displayed
    pub fn is_interesting(&self) -> bool {
        match self.role {
            Role::StaticText | Role::Paragraph => !self.name.trim().is_empty(),
            Role::Link | Role::Button | Role::Heading => true,
            Role::TextField | Role::TextFieldMultiLine => true,
            Role::ComboBox | Role::ListBox => true,
            Role::CheckBox | Role::RadioButton => true,
            Role::ListItem => true,
            Role::Image => true, // Always show images (AI can describe those without alt)
            Role::Banner | Role::Navigation | Role::Main |
            Role::ContentInfo | Role::Complementary | Role::Search => true,
            // Generic nodes are interesting if they're focusable (menu items, etc.)
            Role::Generic => self.state.focusable && !self.name.trim().is_empty(),
            _ => false,
        }
    }

    /// Check if this node is focusable/interactive
    pub fn is_interactive(&self) -> bool {
        self.state.focusable || matches!(
            self.role,
            Role::Link | Role::Button | Role::TextField |
            Role::TextFieldMultiLine | Role::CheckBox |
            Role::RadioButton | Role::ComboBox | Role::ListBox
        )
    }

    /// Get the landmark name if this is a landmark
    pub fn landmark_name(&self) -> Option<&'static str> {
        match self.role {
            Role::Banner => Some("banner"),
            Role::Navigation => Some("navigation"),
            Role::Main => Some("main"),
            Role::ContentInfo => Some("content info"),
            Role::Complementary => Some("complementary"),
            Role::Search => Some("search"),
            Role::Form => Some("form"),
            _ => None,
        }
    }

    /// Count clickable elements within this node
    /// Returns 1 if the node itself is clickable, 2 if it contains another clickable element
    pub fn clickable_children_count(&self) -> usize {
        // If this node contains a link or button, there are 2 clickable targets
        if let Some(ref contained) = self.contains_role {
            match contained {
                Role::Link | Role::Button => 2,
                _ => 1,
            }
        } else {
            1
        }
    }

    /// Check if the node or its contained element at the given index is clickable
    pub fn is_clickable_at(&self, h_index: usize) -> bool {
        if h_index == 0 {
            // First position is always the main element (if interactive)
            self.is_interactive()
        } else if h_index == 1 {
            // Second position is the contained element (if any)
            self.contains_role.as_ref().map_or(false, |r| {
                matches!(r, Role::Link | Role::Button)
            })
        } else {
            false
        }
    }
}
