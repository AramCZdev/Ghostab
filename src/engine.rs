pub mod dom;
pub mod html;
pub mod layout;

/// The engine's own version, shown in the About window. It follows the crate
/// version by default; set it to a literal if the engine ever starts
/// versioning separately from the browser around it.
pub const ENGINE_VERSION: &str = env!("CARGO_PKG_VERSION");

pub use dom::Document;
pub use html::parse_html;
pub use layout::{
    layout_document, ImageSpec, LayoutBox, LinkSpan, Rect, TextStyle, Viewport,
};
