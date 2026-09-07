use super::{repository::ToolRepository, runtime::ToolDiscovery};
use crate::backend::{
    events::UiEventBus,
    storage::{TransactionManager, utc_now},
};
use dispatch_types::{AgentToolName, AgentToolView};
use rootcause::{Result, prelude::*};
use std::{path::PathBuf, sync::Arc};
pub(crate) struct ToolService {
    transactions: Arc<TransactionManager>,
    repository: Arc<ToolRepository>,
    discovery: Arc<ToolDiscovery>,
    events: UiEventBus,
}
impl ToolService {
    pub(crate) fn new(
        transactions: Arc<TransactionManager>,
        repository: Arc<ToolRepository>,
        discovery: Arc<ToolDiscovery>,
        events: UiEventBus,
    ) -> Self {
        Self {
            transactions,
            repository,
            discovery,
            events,
        }
    }
    pub(crate) async fn list(&self) -> Result<Vec<AgentToolView>> {
        let transaction = self.transactions.begin().await?;
        let tools = self.repository.list_in(&transaction).await?;
        transaction.commit().await?;
        Ok(tools)
    }
    pub(crate) async fn discover(&self) -> Result<Vec<AgentToolView>> {
        // Filesystem discovery finishes before acquiring the database transaction.
        let observed = AgentToolName::all()
            .into_iter()
            .map(|tool| (tool, self.discovery.find(tool.as_storage())))
            .collect::<Vec<_>>();
        let transaction = self.transactions.begin().await?;
        let mut tools = Vec::new();
        for (tool, path) in observed {
            let existing = self.repository.find_in(&transaction, tool).await?;
            let present = existing.is_some();
            let mut record = existing.unwrap_or_else(|| new_record(tool, None));
            record.discovered_path = path.map(|path| path.to_string_lossy().into_owned());
            record.last_discovered_at = Some(utc_now());
            record.updated_at = utc_now();
            tools.push(if present {
                self.repository.save_in(&transaction, record).await?
            } else {
                self.repository.insert_in(&transaction, record).await?
            });
        }
        transaction.commit().await?;
        self.events.publish_agent_tool_changed();
        Ok(tools)
    }
    pub(crate) async fn resolve(&self, tool: AgentToolName) -> Result<PathBuf> {
        let transaction = self.transactions.begin().await?;
        let record = self.repository.find_in(&transaction, tool).await?;
        transaction.commit().await?;
        let record = match record {
            Some(record) => record,
            None => self
                .discover()
                .await?
                .into_iter()
                .find(|record| record.tool_name == tool)
                .ok_or_else(|| report!("agent tool '{tool}' was not discovered"))?,
        };
        record
            .effective_path
            .map(PathBuf::from)
            .ok_or_else(|| report!("agent tool '{tool}' is not configured or discoverable"))
    }
    pub(crate) async fn create(
        &self,
        tool: AgentToolName,
        path: Option<String>,
    ) -> Result<AgentToolView> {
        validate_path(&path)?;
        let transaction = self.transactions.begin().await?;
        let record = self
            .repository
            .insert_in(&transaction, new_record(tool, path))
            .await?;
        transaction.commit().await?;
        self.events.publish_agent_tool_changed();
        Ok(record)
    }
    pub(crate) async fn update(&self, id: i64, path: Option<String>) -> Result<AgentToolView> {
        validate_path(&path)?;
        let transaction = self.transactions.begin().await?;
        let mut record = self.repository.get_in(&transaction, id).await?;
        record.executable_path = path;
        record.updated_at = utc_now();
        let record = self.repository.save_in(&transaction, record).await?;
        transaction.commit().await?;
        self.events.publish_agent_tool_changed();
        Ok(record)
    }
    pub(crate) async fn delete(&self, id: i64) -> Result<u64> {
        let transaction = self.transactions.begin().await?;
        self.repository.get_in(&transaction, id).await?;
        let count = self.repository.delete_in(&transaction, id).await?;
        transaction.commit().await?;
        self.events.publish_agent_tool_changed();
        Ok(count)
    }
    #[cfg(test)]
    pub(crate) async fn set_path(
        &self,
        tool: AgentToolName,
        path: PathBuf,
    ) -> Result<AgentToolView> {
        let path = Some(path.to_string_lossy().into_owned());
        validate_path(&path)?;
        let transaction = self.transactions.begin().await?;
        let existing = self.repository.find_in(&transaction, tool).await?;
        let record = if let Some(mut existing) = existing {
            existing.executable_path = path;
            existing.updated_at = utc_now();
            self.repository.save_in(&transaction, existing).await?
        } else {
            self.repository
                .insert_in(&transaction, new_record(tool, path))
                .await?
        };
        transaction.commit().await?;
        self.events.publish_agent_tool_changed();
        Ok(record)
    }
}
pub(super) fn validate_path(path: &Option<String>) -> Result<()> {
    if path.as_deref() == Some("") {
        bail!("agent tool path cannot be empty");
    }
    Ok(())
}
fn new_record(tool: AgentToolName, path: Option<String>) -> AgentToolView {
    let now = utc_now();
    AgentToolView {
        id: 0,
        tool_name: tool,
        executable_path: path,
        discovered_path: None,
        effective_path: None,
        last_discovered_at: None,
        created_at: now.clone(),
        updated_at: now,
    }
}
