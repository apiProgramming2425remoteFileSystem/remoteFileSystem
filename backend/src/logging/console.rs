use crate::logging::LogWriter;

/// Console logging configuration
#[derive(Default, Debug)]
pub struct ConsoleLog {
    ansi: bool,
}

impl ConsoleLog {
    pub fn new() -> Self {
        Self::default().with_ansi_enabled()
    }

    /// Enable ANSI escape code support for coloring and formatting
    pub fn with_ansi_enabled(mut self) -> Self {
        self.ansi = true;
        self
    }
}
impl LogWriter for ConsoleLog {
    fn create_writer(&self) -> Box<dyn std::io::Write + Send + Sync> {
        Box::new(std::io::stdout())
    }

    fn ansi_enabled(&self) -> bool {
        self.ansi
    }
}
