use crate::shared::view_models::AgentRunOutputPiece;

/// Appends one output piece and discards the oldest pieces until the retained output fits.
///
/// The newest piece is always retained, even when it alone exceeds `max_bytes`. This keeps a
/// caller from losing the event it just observed while still bounding accumulated history.
pub(crate) fn push_with_limit(
    pieces: &mut Vec<AgentRunOutputPiece>,
    piece: AgentRunOutputPiece,
    max_bytes: usize,
) {
    pieces.push(piece);

    if pieces.len() <= 1 {
        return;
    }

    let mut retained_bytes = pieces
        .iter()
        .map(output_piece_size)
        .fold(0_usize, usize::saturating_add);
    if retained_bytes <= max_bytes {
        return;
    }

    let mut remove_count = 0;
    for piece in pieces.iter().take(pieces.len() - 1) {
        if retained_bytes <= max_bytes {
            break;
        }
        retained_bytes = retained_bytes.saturating_sub(output_piece_size(piece));
        remove_count += 1;
    }
    pieces.drain(..remove_count);
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

    #[test]
    fn push_with_limit_discards_only_the_oldest_required_pieces() {
        let mut pieces = vec![piece(1, "aaaa"), piece(2, "bbbb")];

        push_with_limit(&mut pieces, piece(3, "cccc"), 16);

        assert_that!(
            &(pieces
                .iter()
                .map(|piece| piece.sequence)
                .collect::<Vec<_>>())
        )
        .is_equal_to(vec![2, 3]);
    }

    #[test]
    fn push_with_limit_keeps_an_oversized_newest_piece() {
        let mut pieces = vec![piece(1, "old")];

        push_with_limit(&mut pieces, piece(2, "oversized"), 1);

        assert_that!(&(pieces.len())).is_equal_to(1);
        assert_that!(&(pieces[0].sequence)).is_equal_to(2);
    }
}
