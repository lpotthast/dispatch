use std::io::{self, Write};

use rootcause::{Result, prelude::*};
use serde::Serialize;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum Format {
    #[default]
    Text,
    Json,
}

impl Format {
    pub(crate) const fn from_json_flag(json: bool) -> Self {
        if json { Self::Json } else { Self::Text }
    }
}

pub(crate) fn write<T>(
    format: Format,
    value: &T,
    text: impl FnOnce(&mut dyn Write) -> io::Result<()>,
) -> Result<()>
where
    T: Serialize,
{
    let stdout = io::stdout();
    let mut output = stdout.lock();
    if format == Format::Json {
        serde_json::to_writer_pretty(&mut output, value).context("failed to write JSON output")?;
        writeln!(output).context("failed to write CLI output")?;
    } else {
        text(&mut output).context("failed to write CLI output")?;
    }
    Ok(())
}
