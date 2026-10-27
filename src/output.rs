use crate::Result;
use cli_command_kit::Mode;
use std::{
    io::Write,
    path::{Path, PathBuf},
};

pub fn expand(path: &Path) -> PathBuf {
    if let Ok(tail) = path.strip_prefix("~")
        && let Some(home) = dirs::home_dir()
    {
        return home.join(tail);
    }
    path.to_path_buf()
}
pub use cli_command_kit::display_message as fold_text;
pub struct Output<'a> {
    pub mode: Mode,
    pub out: &'a mut dyn Write,
    pub diagnostics: &'a mut dyn Write,
}
impl Output<'_> {
    pub fn line(&mut self, text: impl AsRef<str>) -> Result<()> {
        writeln!(self.out, "{}", fold_text(text.as_ref()))?;
        Ok(())
    }
    pub fn fields(&mut self, values: &[(&str, String)]) -> Result<()> {
        for (key, value) in values {
            self.line(if self.mode == Mode::Agent {
                format!("{key}: {value}")
            } else {
                format!("{}: {value}", key.replace('_', " "))
            })?;
        }
        Ok(())
    }
    pub fn warning(&mut self, text: &str) -> Result<()> {
        writeln!(
            self.diagnostics,
            "{} {}",
            if self.mode == Mode::Agent {
                "WARN:"
            } else {
                "[WARN]"
            },
            fold_text(text)
        )?;
        Ok(())
    }
}
