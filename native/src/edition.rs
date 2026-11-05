//! Which app this is.

/// The window and message box title, the data folder under %LOCALAPPDATA%,
/// and how /api/version tells a second launch it's us.
pub const NAME: &str = "LSP Converter";
/// the UI port
pub const PORT: u16 = 8092;

/// The window's title.
pub fn window_title() -> String {
    NAME.to_string()
}
