use std::io::{self, Write};

use rootcause::{Result, prelude::*};
use serde::Serialize;

pub(crate) fn write<T>(
    json: bool,
    value: &T,
    text: impl FnOnce(&mut dyn Write) -> io::Result<()>,
) -> Result<()>
where
    T: Serialize,
{
    let stdout = io::stdout();
    let mut output = stdout.lock();
    if json {
        serde_json::to_writer_pretty(&mut output, value).context("failed to write JSON output")?;
        writeln!(output).context("failed to write CLI output")?;
    } else {
        text(&mut output).context("failed to write CLI output")?;
    }
    Ok(())
}
