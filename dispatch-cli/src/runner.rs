mod automation;
mod comments;
mod items;
mod knowledge;
mod labels;
mod relationships;
mod work_groups;

use rootcause::Result;

use crate::{
    commands::{Command, ProjectCommand},
    context::ResolvedContext,
    git_guard::run_git,
    output, render,
};

pub(crate) async fn run(
    command: Command,
    context: ResolvedContext,
    format: output::Format,
) -> Result<()> {
    match command {
        Command::Project { command } => run_project(command, context, format).await,
        Command::Item { command } => items::run(command, context, format).await,
        Command::Comment { command } => comments::run(command, context, format).await,
        Command::Label { command } => labels::run(command, context, format).await,
        Command::Relationship { command } => relationships::run(command, context, format).await,
        Command::Group { command } => work_groups::run(command, context, format).await,
        Command::Knowledge { command } => knowledge::run(command, context, format).await,
        Command::Automation { command } => automation::run(command, context, format).await,
        Command::Git(args) => run_git(args.args),
    }
}

async fn run_project(
    command: ProjectCommand,
    context: ResolvedContext,
    format: output::Format,
) -> Result<()> {
    let client = context.client();
    match command {
        ProjectCommand::List => {
            let projects = client.list_projects().await?;
            output::write(format, &projects, |output| {
                render::write_project_rows(output, &projects)
            })
        }
    }
}

#[cfg(test)]
mod tests;
