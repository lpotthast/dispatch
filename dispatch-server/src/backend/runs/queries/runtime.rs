use crate::backend::execution::output::{read_run_output, read_run_token_usage};
use dispatch_types::{AgentRunView, RunLogView};
use rootcause::{Result, prelude::*};
pub(crate) struct RunArtifacts;
impl RunArtifacts {
    pub(crate) async fn enrich(&self, mut view: AgentRunView) -> AgentRunView {
        if view.token_usage.is_none() {
            view.token_usage = read_run_token_usage(view.log_path.as_deref()).await;
        }
        view
    }
    pub(crate) async fn read_log(&self, log: &mut RunLogView) -> Result<()> {
        log.developer_instructions =
            read_optional_text(log.run.developer_instructions_path.as_deref()).await?;
        log.user_prompt = read_optional_text(log.run.user_prompt_path.as_deref()).await?;
        log.output = read_run_output(log.run.log_path.as_deref()).await?;
        Ok(())
    }
}
async fn read_optional_text(path: Option<&str>) -> Result<Option<String>> {
    let Some(path) = path else {
        return Ok(None);
    };
    let body = match tokio::fs::read_to_string(path).await {
        Ok(body) => body,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(err) => return Err(err).context_with(|| format!("failed to read {}", path))?,
    };
    Ok(Some(body))
}
