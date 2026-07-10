use rootcause::{Result, prelude::*};

use crate::backend::{entities::work_item_label::WorkItemLabelModel, item_labels, workflow_labels};

#[derive(Debug, Eq, PartialEq)]
pub(crate) struct AddLabelMutation {
    pub(crate) key: String,
    pub(crate) value: Option<String>,
}

#[derive(Debug, Eq, PartialEq)]
pub(crate) struct UpdateLabelMutation {
    key: Option<String>,
    value: Option<Option<String>>,
}

#[derive(Debug, Eq, PartialEq)]
pub(crate) struct AppliedLabelMutation {
    pub(crate) key: String,
    pub(crate) value: Option<String>,
}

#[derive(Debug, Eq, PartialEq)]
pub(crate) struct DeleteLabelMutation {
    label_id: i64,
    key: String,
    value: Option<String>,
}

#[derive(Debug, Eq, PartialEq)]
pub(crate) struct LabelMutationEvent {
    pub(crate) event_type: &'static str,
    pub(crate) body: String,
}

impl AddLabelMutation {
    pub(crate) fn new(key: String, value: Option<String>) -> Result<Self> {
        let key = item_labels::normalize_key(key)?;
        workflow_labels::ensure_generic_label_can_be_changed(&key)?;
        let value = item_labels::normalize_value(value);
        item_labels::validate_pair(&key, value.as_deref())?;

        Ok(Self { key, value })
    }

    pub(crate) fn added_event(&self) -> LabelMutationEvent {
        LabelMutationEvent {
            event_type: "label_added",
            body: format!(
                "Added label {}",
                item_labels::format_label(&self.key, self.value.as_deref())
            ),
        }
    }
}

impl UpdateLabelMutation {
    pub(crate) fn new(key: Option<String>, value: Option<Option<String>>) -> Result<Self> {
        if key.is_none() && value.is_none() {
            bail!("label update requires at least one field");
        }

        Ok(Self { key, value })
    }

    pub(crate) fn apply_to(self, existing: &WorkItemLabelModel) -> Result<AppliedLabelMutation> {
        workflow_labels::ensure_generic_label_can_be_changed(&existing.key)?;

        let key = match self.key {
            Some(key) => item_labels::normalize_key(key)?,
            None => existing.key.clone(),
        };
        workflow_labels::ensure_generic_label_can_be_changed(&key)?;

        let value = match self.value {
            Some(value) => item_labels::normalize_value(value),
            None => existing.value.clone(),
        };
        item_labels::validate_pair(&key, value.as_deref())?;

        Ok(AppliedLabelMutation { key, value })
    }
}

impl AppliedLabelMutation {
    pub(crate) fn updated_event(&self) -> LabelMutationEvent {
        LabelMutationEvent {
            event_type: "label_updated",
            body: format!(
                "Updated label {}",
                item_labels::format_label(&self.key, self.value.as_deref())
            ),
        }
    }
}

impl DeleteLabelMutation {
    pub(crate) fn new(label: &WorkItemLabelModel) -> Result<Self> {
        workflow_labels::ensure_generic_label_can_be_deleted(&label.key)?;
        Ok(Self {
            label_id: label.id,
            key: label.key.clone(),
            value: label.value.clone(),
        })
    }

    pub(crate) fn label_id(&self) -> i64 {
        self.label_id
    }

    pub(crate) fn deleted_event(&self) -> LabelMutationEvent {
        LabelMutationEvent {
            event_type: "label_deleted",
            body: format!(
                "Deleted label {}",
                item_labels::format_label(&self.key, self.value.as_deref())
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::shared::view_models::{CLAIMED_FROM_STATE_LABEL_KEY, STATE_LABEL_KEY};

    use super::*;

    fn label(key: &str, value: Option<&str>) -> WorkItemLabelModel {
        WorkItemLabelModel {
            id: 11,
            project_id: 3,
            work_item_id: 7,
            key: key.to_owned(),
            value: value.map(ToOwned::to_owned),
            created_at: "2026-06-19T00:00:00Z".to_owned(),
            updated_at: "2026-06-19T00:00:00Z".to_owned(),
        }
    }

    #[test]
    fn add_label_mutation_normalizes_non_state_labels() {
        let mutation =
            AddLabelMutation::new(" priority ".to_owned(), Some(" high ".to_owned())).unwrap();

        assert_eq!(
            mutation,
            AddLabelMutation {
                key: "priority".to_owned(),
                value: Some("high".to_owned()),
            }
        );
        assert_eq!(
            mutation.added_event(),
            LabelMutationEvent {
                event_type: "label_added",
                body: "Added label priority=high".to_owned(),
            }
        );
    }

    #[test]
    fn update_label_mutation_applies_partial_updates_to_existing_label() {
        let mutation = UpdateLabelMutation::new(None, Some(Some(" low ".to_owned()))).unwrap();

        let applied = mutation.apply_to(&label("priority", Some("high"))).unwrap();

        assert_eq!(
            applied,
            AppliedLabelMutation {
                key: "priority".to_owned(),
                value: Some("low".to_owned()),
            }
        );
        assert_eq!(
            applied.updated_event(),
            LabelMutationEvent {
                event_type: "label_updated",
                body: "Updated label priority=low".to_owned(),
            }
        );
    }

    #[test]
    fn delete_label_mutation_rejects_state_and_keeps_deleted_label_snapshot() {
        let mutation = DeleteLabelMutation::new(&label("priority", Some("high"))).unwrap();

        assert_eq!(mutation.label_id(), 11);
        assert_eq!(
            mutation.deleted_event(),
            LabelMutationEvent {
                event_type: "label_deleted",
                body: "Deleted label priority=high".to_owned(),
            }
        );

        let state_err =
            DeleteLabelMutation::new(&label(STATE_LABEL_KEY, Some("open"))).unwrap_err();
        assert!(state_err.to_string().contains("move the item"));
    }

    #[test]
    fn state_label_mutations_are_rejected() {
        let add_err =
            AddLabelMutation::new(STATE_LABEL_KEY.to_owned(), Some("open".to_owned())).unwrap_err();
        assert!(add_err.to_string().contains("move the item"));

        let update_existing_err = UpdateLabelMutation::new(None, Some(Some("done".to_owned())))
            .unwrap()
            .apply_to(&label(STATE_LABEL_KEY, Some("open")))
            .unwrap_err();
        assert!(update_existing_err.to_string().contains("move the item"));

        let update_to_state_err = UpdateLabelMutation::new(
            Some(STATE_LABEL_KEY.to_owned()),
            Some(Some("open".to_owned())),
        )
        .unwrap()
        .apply_to(&label("priority", Some("high")))
        .unwrap_err();
        assert!(update_to_state_err.to_string().contains("move the item"));
    }

    #[test]
    fn private_workflow_label_mutations_are_rejected() {
        let add_err = AddLabelMutation::new(
            CLAIMED_FROM_STATE_LABEL_KEY.to_owned(),
            Some("open".to_owned()),
        )
        .unwrap_err();
        assert!(add_err.to_string().contains("workflow bookkeeping"));

        let update_existing_err = UpdateLabelMutation::new(None, Some(Some("done".to_owned())))
            .unwrap()
            .apply_to(&label(CLAIMED_FROM_STATE_LABEL_KEY, Some("open")))
            .unwrap_err();
        assert!(
            update_existing_err
                .to_string()
                .contains("workflow bookkeeping")
        );

        let update_to_private_err = UpdateLabelMutation::new(
            Some(CLAIMED_FROM_STATE_LABEL_KEY.to_owned()),
            Some(Some("open".to_owned())),
        )
        .unwrap()
        .apply_to(&label("priority", Some("high")))
        .unwrap_err();
        assert!(
            update_to_private_err
                .to_string()
                .contains("workflow bookkeeping")
        );

        let delete_err =
            DeleteLabelMutation::new(&label(CLAIMED_FROM_STATE_LABEL_KEY, Some("open")))
                .unwrap_err();
        assert!(delete_err.to_string().contains("workflow bookkeeping"));
    }
}
