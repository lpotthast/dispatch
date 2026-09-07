use std::collections::VecDeque;

use crate::shared::view_models::AgentRunOutputPiece;

/// Retains the newest output within a byte budget, always keeping the newest piece even
/// when it alone exceeds the budget. Pieces are immutable after insertion so their cached
/// sizes stay valid. Each piece is measured once and removed at most once.
#[derive(Debug)]
pub(crate) struct BoundedOutput {
    pieces: VecDeque<(AgentRunOutputPiece, usize)>,
    retained_bytes: usize,
    max_bytes: usize,
}

impl BoundedOutput {
    pub(crate) fn for_run() -> Self {
        Self::new(1024 * 1024)
    }

    pub(crate) fn for_session() -> Self {
        Self::new(256 * 1024)
    }

    fn new(max_bytes: usize) -> Self {
        Self {
            pieces: VecDeque::new(),
            retained_bytes: 0,
            max_bytes,
        }
    }

    pub(crate) fn push(&mut self, piece: AgentRunOutputPiece) {
        let bytes = output_piece_size(&piece);
        // Evict before adding, including all old pieces when the newest piece is oversized.
        let available = self.max_bytes.saturating_sub(bytes);
        while self.retained_bytes > available {
            let (_, removed_bytes) = self.pieces.pop_front().expect("retained output exists");
            self.retained_bytes -= removed_bytes;
        }
        self.retained_bytes += bytes;
        self.pieces.push_back((piece, bytes));
    }

    pub(crate) fn iter(
        &self,
    ) -> impl DoubleEndedIterator<Item = &AgentRunOutputPiece> + ExactSizeIterator {
        self.pieces.iter().map(|(piece, _)| piece)
    }

    pub(crate) fn last(&self) -> Option<&AgentRunOutputPiece> {
        self.iter().next_back()
    }
}

fn output_piece_size(piece: &AgentRunOutputPiece) -> usize {
    piece.timestamp.len()
        + piece.source.len()
        + piece.item_id.as_deref().map(str::len).unwrap_or_default()
        + piece.title.len()
        + piece.body.len()
        + piece.metadata.to_string().len()
}

#[cfg(test)]
mod tests {
    use assertr::prelude::*;

    use super::*;
    use crate::shared::view_models::AgentRunOutputKind;

    fn piece(sequence: u64, body: &str) -> AgentRunOutputPiece {
        AgentRunOutputPiece {
            sequence,
            timestamp: String::new(),
            kind: AgentRunOutputKind::ModelMessage,
            source: String::new(),
            item_id: None,
            title: String::new(),
            body: body.to_owned(),
            metadata: serde_json::Value::Null,
        }
    }

    fn sequences(output: &BoundedOutput) -> Vec<u64> {
        output.iter().map(|piece| piece.sequence).collect()
    }

    #[test]
    fn retains_exact_budget_and_discards_only_the_oldest_required_pieces() {
        let mut output = BoundedOutput::new(16);
        output.push(piece(1, "aaaa"));
        output.push(piece(2, "bbbb"));
        assert_that!(sequences(&output)).is_equal_to(vec![1, 2]);

        output.push(piece(3, "cccc"));
        assert_that!(sequences(&output)).is_equal_to(vec![2, 3]);

        output.push(piece(4, "ddddddddddddd"));
        assert_that!(sequences(&output)).is_equal_to(vec![4]);

        output.push(piece(5, "e"));
        output.push(piece(6, "f"));
        assert_that!(sequences(&output)).is_equal_to(vec![5, 6]);
    }

    #[test]
    fn zero_budget_keeps_only_the_newest_piece() {
        let mut output = BoundedOutput::new(0);
        assert_that!(output.last()).is_none();
        for sequence in 1..=3 {
            output.push(piece(sequence, ""));
            assert_that!(sequences(&output)).is_equal_to(vec![sequence]);
        }
    }

    #[test]
    fn budget_counts_utf8_fields_and_encoded_metadata() {
        let rich = AgentRunOutputPiece {
            timestamp: "now".into(),
            source: "codex".into(),
            item_id: Some("42".into()),
            title: "title".into(),
            body: "é".into(),
            metadata: serde_json::json!({ "text": "\n" }),
            ..piece(2, "")
        };
        // 3 + 5 + 2 + 5 + 2 UTF-8 bytes, plus 13 bytes of compact JSON.
        for (budget, expected) in [(38, vec![1, 2]), (37, vec![2])] {
            let mut output = BoundedOutput::new(budget);
            output.push(piece(1, "aaaa"));
            output.push(rich.clone());
            assert_that!(sequences(&output)).is_equal_to(expected);
        }
    }

    #[test]
    fn repeated_eviction_preserves_order() {
        let mut output = BoundedOutput::new(16);
        for sequence in 1..=1000 {
            output.push(piece(sequence, "aaaa"));
        }
        assert_that!(sequences(&output)).is_equal_to(vec![999, 1000]);
    }
}
