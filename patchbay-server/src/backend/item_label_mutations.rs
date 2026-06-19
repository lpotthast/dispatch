use rootcause::{Result, prelude::*};

use crate::{
    backend::{entities::work_item_label::WorkItemLabelModel, item_labels},
    shared::view_models::STATE_LABEL_KEY,
};

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

impl AddLabelMutation {
    pub(crate) fn new(key: String, value: Option<String>) -> Result<Self> {
        let key = item_labels::normalize_key(key)?;
        reject_state_label_mutation(&key)?;
        let value = item_labels::normalize_value(value);
        item_labels::validate_pair(&key, value.as_deref())?;

        Ok(Self { key, value })
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
        reject_state_label_mutation(&existing.key)?;

        let key = match self.key {
            Some(key) => item_labels::normalize_key(key)?,
            None => existing.key.clone(),
        };
        reject_state_label_mutation(&key)?;

        let value = match self.value {
            Some(value) => item_labels::normalize_value(value),
            None => existing.value.clone(),
        };
        item_labels::validate_pair(&key, value.as_deref())?;

        Ok(AppliedLabelMutation { key, value })
    }
}

pub(crate) fn ensure_label_can_be_deleted(key: &str) -> Result<()> {
    if key == STATE_LABEL_KEY {
        bail!("state label cannot be deleted; move the item to another state instead");
    }
    Ok(())
}

fn reject_state_label_mutation(key: &str) -> Result<()> {
    if key == STATE_LABEL_KEY {
        bail!(
            "state label cannot be changed through label mutations; move the item to another state instead"
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
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
}
