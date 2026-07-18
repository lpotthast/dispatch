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
    use assertr::prelude::*;

    #[test]
    fn tiptap_html_preserves_prompt_structure_as_markdown() {
        let html = concat!(
            "<h2>Acceptance criteria</h2>",
            "<ul><li>Inspect <code>parser-module</code>.</li>",
            "<li>Run <strong>focused tests</strong>.</li></ul>",
            "<pre><code>just test</code></pre>",
        );

        let markdown = rich_text_to_prompt_markdown(html).unwrap();

        assert_that!(&(markdown.contains("## Acceptance criteria")))
            .with_detail_message(markdown.to_string())
            .is_true();
        assert_that!(&(markdown.contains("*   Inspect `parser-module`.")))
            .with_detail_message(markdown.to_string())
            .is_true();
        assert_that!(&(markdown.contains("*   Run **focused tests**.")))
            .with_detail_message(markdown.to_string())
            .is_true();
        assert_that!(&(markdown.contains("```")))
            .with_detail_message(markdown.to_string())
            .is_true();
        assert_that!(&(markdown.contains("just test")))
            .with_detail_message(markdown.to_string())
            .is_true();
        assert_that!(&(!markdown.contains("<li>")))
            .with_detail_message(markdown.to_string())
            .is_true();
    }
}
