/// Camoufox WebDriver - Direct browser control via Marionette protocol
///
/// This module provides direct control of Camoufox browser using the WebDriver protocol,
/// bypassing Playwright to avoid detection. It launches Camoufox with --marionette flag
/// and connects via thirtyfour crate.
///
/// Architecture:
/// ```
/// Rust CLI
///     ↓ WebDriver protocol
/// Camoufox --marionette (port 2828)
/// ```

mod driver;

pub use driver::CamofoxDriver;
