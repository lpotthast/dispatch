use crate::backend::{
    entities::agent_tool::{self, AgentTool, AgentToolActiveModel, AgentToolModel},
    storage::Transaction,
};
use dispatch_types::{AgentToolName, AgentToolView};
use rootcause::{Result, prelude::*};
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, EntityTrait, QueryFilter, QueryOrder,
};
use std::str::FromStr;
fn model_to_view(tool: AgentToolModel) -> Result<AgentToolView> {
    let tool_name = AgentToolName::from_str(&tool.tool_name)?;
    let effective_path = tool
        .executable_path
        .clone()
        .or_else(|| tool.discovered_path.clone());
    Ok(AgentToolView {
        id: tool.id,
        tool_name,
        executable_path: tool.executable_path,
        discovered_path: tool.discovered_path,
        effective_path,
        last_discovered_at: tool.last_discovered_at,
        created_at: tool.created_at,
        updated_at: tool.updated_at,
    })
}

pub(crate) fn encode(record: AgentToolView) -> AgentToolModel {
    AgentToolModel {
        id: record.id,
        tool_name: record.tool_name.as_storage().to_owned(),
        executable_path: record.executable_path,
        discovered_path: record.discovered_path,
        last_discovered_at: record.last_discovered_at,
        created_at: record.created_at,
        updated_at: record.updated_at,
    }
}
pub(crate) struct ToolRepository;
impl ToolRepository {
    pub(crate) async fn list_in(&self, transaction: &Transaction) -> Result<Vec<AgentToolView>> {
        AgentTool::find()
            .filter(agent_tool::Column::ToolName.eq(AgentToolName::Codex.as_storage()))
            .order_by_asc(agent_tool::Column::ToolName)
            .all(transaction.connection())
            .await
            .context("failed to list agent tools")?
            .into_iter()
            .map(model_to_view)
            .collect()
    }
    pub(crate) async fn find_in(
        &self,
        transaction: &Transaction,
        tool: AgentToolName,
    ) -> Result<Option<AgentToolView>> {
        AgentTool::find()
            .filter(agent_tool::Column::ToolName.eq(tool.as_storage()))
            .one(transaction.connection())
            .await
            .context_with(|| format!("failed to load agent tool '{tool}'"))?
            .map(model_to_view)
            .transpose()
    }
    pub(crate) async fn get_in(&self, transaction: &Transaction, id: i64) -> Result<AgentToolView> {
        model_to_view(
            AgentTool::find_by_id(id)
                .one(transaction.connection())
                .await
                .context("failed to load agent tool")?
                .ok_or_else(|| report!("agent tool {id} does not exist"))?,
        )
    }
    pub(crate) async fn save_in(
        &self,
        transaction: &Transaction,
        record: AgentToolView,
    ) -> Result<AgentToolView> {
        use sea_orm::IntoActiveModel;
        let active = encode(record).into_active_model().reset_all();
        model_to_view(
            active
                .update(transaction.connection())
                .await
                .context("failed to update agent tool")?,
        )
    }
    pub(crate) async fn insert_in(
        &self,
        transaction: &Transaction,
        record: AgentToolView,
    ) -> Result<AgentToolView> {
        model_to_view(
            AgentToolActiveModel {
                tool_name: Set(record.tool_name.as_storage().to_owned()),
                executable_path: Set(record.executable_path),
                discovered_path: Set(record.discovered_path),
                last_discovered_at: Set(record.last_discovered_at),
                created_at: Set(record.created_at),
                updated_at: Set(record.updated_at),
                ..Default::default()
            }
            .insert(transaction.connection())
            .await
            .context("failed to create agent tool")?,
        )
    }
    pub(crate) async fn delete_in(&self, transaction: &Transaction, id: i64) -> Result<u64> {
        Ok(AgentTool::delete_by_id(id)
            .exec(transaction.connection())
            .await
            .context("failed to delete agent tool")?
            .rows_affected)
    }
}
