//! Accessibility node representation.
//!
//! Represents a single node from Chromium's accessibility tree.

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

/// An accessibility node from the AXTree
#[derive(Clone, Debug, Default)]
pub struct AXNode {
    /// Unique identifier
    pub id: i32,

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
    pub parent_id: Option<i32>,

    /// Child node IDs
    pub child_ids: Vec<i32>,

    /// Position in set (e.g., list item 3 of 5)
    pub pos_in_set: Option<u32>,
    pub set_size: Option<u32>,
}

impl AXNode {
    pub fn new(id: i32) -> Self {
        Self {
            id,
            ..Default::default()
        }
    }

    /// Check if this node should be announced/displayed
    pub fn is_interesting(&self) -> bool {
        match self.role {
            Role::StaticText | Role::Paragraph => !self.name.trim().is_empty(),
            Role::Link | Role::Button | Role::Heading => true,
            Role::TextField | Role::TextFieldMultiLine => true,
            Role::CheckBox | Role::RadioButton => true,
            Role::ListItem => true,
            Role::Image => !self.name.is_empty(), // Only if has alt text
            Role::Banner | Role::Navigation | Role::Main |
            Role::ContentInfo | Role::Complementary | Role::Search => true,
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
}
