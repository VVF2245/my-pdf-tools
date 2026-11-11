//! What the shell tells the tools about the machine they run on.

use std::path::PathBuf;

use mpt_engines::pdf_reader::PdfiumLocation;

/// Settings the tools need from the application that hosts them.
#[derive(Debug, Clone)]
pub struct ToolsConfig {
    /// Where to look for the PDFium library.
    pub pdfium: PdfiumLocation,
}

impl ToolsConfig {
    /// Looks for PDFium in `pdfium_dirs` (the packaged resource folders only the shell
    /// knows), then next to the executable, in `$PDFIUM_DIR` and in the system.
    pub fn new(pdfium_dirs: Vec<PathBuf>) -> Self {
        Self { pdfium: PdfiumLocation::standard(pdfium_dirs) }
    }
}

impl Default for ToolsConfig {
    fn default() -> Self {
        Self::new(Vec::new())
    }
}
