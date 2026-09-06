#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct RunSelection {
    requested: Option<i64>,
    pub(super) selected: Option<i64>,
}

impl RunSelection {
    pub(super) fn resolve(requested: Option<i64>, previous: Option<&Self>, ids: &[i64]) -> Self {
        let retained = previous
            .filter(|previous| previous.requested == requested)
            .and_then(|previous| previous.selected);
        let selected = requested
            .filter(|id| ids.contains(id))
            .or_else(|| retained.filter(|id| ids.contains(id)))
            .or_else(|| ids.first().copied());
        Self {
            requested,
            selected,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::RunSelection;
    use assertr::prelude::*;

    #[test]
    fn live_updates_preserve_the_implicitly_selected_run() {
        let initial = RunSelection::resolve(None, None, &[1, 2]);
        let refreshed = RunSelection::resolve(None, Some(&initial), &[3, 1, 2]);
        assert_that!(refreshed.selected).is_equal_to(Some(1));
        let removed = RunSelection::resolve(None, Some(&refreshed), &[3, 2]);
        assert_that!(removed.selected).is_equal_to(Some(3));
    }

    #[test]
    fn clearing_the_url_selection_selects_the_default_run() {
        let explicit = RunSelection::resolve(Some(2), None, &[1, 2]);
        assert_that!(explicit.selected).is_equal_to(Some(2));
        let cleared = RunSelection::resolve(None, Some(&explicit), &[1, 2]);
        assert_that!(cleared.selected).is_equal_to(Some(1));
    }

    #[test]
    fn requested_run_survives_initial_loading_and_invalid_ids_fall_back() {
        let pending = RunSelection::resolve(Some(2), None, &[]);
        assert_that!(pending.selected).is_none();
        let loaded = RunSelection::resolve(Some(2), Some(&pending), &[1, 2]);
        assert_that!(loaded.selected).is_equal_to(Some(2));
        let invalid = RunSelection::resolve(Some(99), Some(&loaded), &[1, 2]);
        assert_that!(invalid.selected).is_equal_to(Some(1));
    }
}
