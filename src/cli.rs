use clap::{Args, Parser, Subcommand, ValueEnum};

use crate::model::{TaskStatus, Visibility};

#[derive(Debug, Clone, Copy, ValueEnum, PartialEq, Eq)]
pub enum OutputFormat {
    Text,
    Json,
    Ndjson,
}

#[derive(Debug, Parser)]
#[command(name = "tk", version, about = "Structured task list CLI")]
pub struct Cli {
    #[arg(long, global = true)]
    pub root: Option<String>,
    #[arg(long, global = true)]
    pub list: Option<String>,
    #[arg(long, global = true)]
    pub format: Option<OutputFormat>,
    #[arg(long, global = true)]
    pub no_color: bool,
    #[arg(long, global = true)]
    pub quiet: bool,
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    Init(InitArgs),
    Dir,
    Create(CreateArgs),
    List(ListArgs),
    Get(TaskIdArgs),
    Update(UpdateArgs),
    Start(StatusShortcutArgs),
    Done(StatusShortcutArgs),
    Reopen(TaskIdArgs),
    Claim(ClaimArgs),
    Unclaim(UnclaimArgs),
    Next(NextArgs),
    Block {
        #[command(subcommand)]
        command: BlockCommand,
    },
    Delete(DeleteArgs),
    Reset(ResetArgs),
    Verify,
    Watch(WatchArgs),
}

#[derive(Debug, Args)]
pub struct InitArgs {
    #[arg(long)]
    pub title: Option<String>,
    #[arg(long)]
    pub description: Option<String>,
}

#[derive(Debug, Args)]
pub struct CreateArgs {
    pub subject: Option<String>,
    #[arg(long)]
    pub description: Option<String>,
    #[arg(long = "active-form")]
    pub active_form: Option<String>,
    #[arg(long)]
    pub owner: Option<String>,
    #[arg(long, value_enum)]
    pub visibility: Option<Visibility>,
    #[arg(long = "meta")]
    pub metadata: Vec<String>,
    #[arg(long = "json-body")]
    pub json_body: Option<String>,
}

#[derive(Debug, Args)]
pub struct ListArgs {
    #[arg(long, value_enum)]
    pub status: Vec<TaskStatus>,
    #[arg(long)]
    pub owner: Option<String>,
    #[arg(long)]
    pub unowned: bool,
    #[arg(long)]
    pub claimable: bool,
    #[arg(long = "all")]
    pub include_internal: bool,
    #[arg(long)]
    pub limit: Option<usize>,
}

#[derive(Debug, Args)]
pub struct TaskIdArgs {
    pub id: String,
}

#[derive(Debug, Args)]
pub struct UpdateArgs {
    pub id: String,
    #[arg(long)]
    pub subject: Option<String>,
    #[arg(long)]
    pub description: Option<String>,
    #[arg(long = "active-form")]
    pub active_form: Option<String>,
    #[arg(long, value_enum)]
    pub status: Option<TaskStatus>,
    #[arg(long)]
    pub owner: Option<String>,
    #[arg(long)]
    pub clear_owner: bool,
    #[arg(long, value_enum)]
    pub visibility: Option<Visibility>,
    #[arg(long = "set-meta")]
    pub set_metadata: Vec<String>,
    #[arg(long = "unset-meta")]
    pub unset_metadata: Vec<String>,
    #[arg(long = "if-revision")]
    pub if_revision: Option<u64>,
    #[arg(long)]
    pub force: bool,
}

#[derive(Debug, Args)]
pub struct StatusShortcutArgs {
    pub id: String,
    #[arg(long = "if-revision")]
    pub if_revision: Option<u64>,
    #[arg(long)]
    pub force: bool,
}

#[derive(Debug, Args)]
pub struct ClaimArgs {
    pub id: String,
    #[arg(long)]
    pub owner: Option<String>,
    #[arg(long)]
    pub start: bool,
    #[arg(long = "check-busy")]
    pub check_busy: bool,
    #[arg(long = "if-revision")]
    pub if_revision: Option<u64>,
}

#[derive(Debug, Args)]
pub struct UnclaimArgs {
    pub id: String,
    #[arg(long)]
    pub requeue: bool,
    #[arg(long = "if-revision")]
    pub if_revision: Option<u64>,
}

#[derive(Debug, Args)]
pub struct NextArgs {
    #[arg(long)]
    pub claim: bool,
    #[arg(long)]
    pub owner: Option<String>,
    #[arg(long)]
    pub start: bool,
    #[arg(long = "check-busy")]
    pub check_busy: bool,
}

#[derive(Debug, Subcommand)]
pub enum BlockCommand {
    Add(BlockArgs),
    Remove(BlockArgs),
}

#[derive(Debug, Args)]
pub struct BlockArgs {
    pub task_id: String,
    pub blocker_ids: Vec<String>,
}

#[derive(Debug, Args)]
pub struct DeleteArgs {
    pub id: String,
    #[arg(long)]
    pub detach: bool,
    #[arg(long = "if-revision")]
    pub if_revision: Option<u64>,
}

#[derive(Debug, Args)]
pub struct ResetArgs {
    #[arg(long)]
    pub force: bool,
}

#[derive(Debug, Args)]
pub struct WatchArgs {
    #[arg(long = "interval-ms", default_value_t = 1000)]
    pub interval_ms: u64,
}
