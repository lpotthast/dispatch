use rootcause::{Result, prelude::*};

/// Converts Tiptap's stored HTML into Markdown suitable for an agent prompt.
///
/// Conversion belongs at the prompt boundary: Dispatch keeps the original rich text for editing
/// and display while agents receive a readable task.
pub(crate) fn rich_text_to_prompt_markdown(value: &str) -> Result<String> {
    Ok(htmd::convert(value)
        .context("failed to convert rich-text HTML into Markdown for the automation prompt")?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tiptap_html_preserves_prompt_structure_as_markdown() {
        let html = concat!(
            "<h2>Acceptance criteria</h2>",
            "<ul><li>Inspect <code>parser-module</code>.</li>",
            "<li>Run <strong>focused tests</strong>.</li></ul>",
            "<pre><code>just test</code></pre>",
        );

        let markdown = rich_text_to_prompt_markdown(html).unwrap();

        assert!(markdown.contains("## Acceptance criteria"), "{markdown}");
        assert!(
            markdown.contains("*   Inspect `parser-module`."),
            "{markdown}"
        );
        assert!(
            markdown.contains("*   Run **focused tests**."),
            "{markdown}"
        );
        assert!(markdown.contains("```"), "{markdown}");
        assert!(markdown.contains("just test"), "{markdown}");
        assert!(!markdown.contains("<li>"), "{markdown}");
    }
}
