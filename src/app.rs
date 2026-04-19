use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;

use crate::cli::{
    BlockCommand, Cli, Command, CreateArgs, ListArgs, OutputFormat, StatusShortcutArgs, TaskIdArgs,
};
use crate::error::{ErrorCode, TkError};
use crate::model::{DiagnosticLevel, TaskStatus, Visibility};
use crate::output::{
    CommandOutput, envelope, print_success, text_for_dir, text_for_list, text_for_task,
    text_for_verify,
};
use crate::storage::{CreateTaskInput, ListFilters, ListStore, UpdateTaskInput, resolve_context};
use crate::watch::watch_list;

#[derive(Debug, Default, Deserialize)]
struct CreateJsonBody {
    subject: Option<String>,
    description: Option<String>,
    active_form: Option<String>,
    owner: Option<String>,
    visibility: Option<Visibility>,
    metadata: Option<BTreeMap<String, Value>>,
}

pub fn run(cli: Cli) -> Result<i32, TkError> {
    let context = resolve_context(cli.root.as_deref(), cli.list.as_deref(), cli.format)?;
    let format = context.output_format;
    let store = ListStore::new(context.paths.clone());

    match cli.command {
        Command::Init(args) => {
            let manifest = store.init(args.title, args.description)?;
            let output = if format == OutputFormat::Json {
                CommandOutput::Json(envelope(
                    "init",
                    &manifest,
                    json!({
                        "manifest": manifest,
                    }),
                ))
            } else {
                CommandOutput::Text(format!(
                    "Initialized task list {} at {}",
                    manifest.list_id,
                    store.paths.list_dir.display()
                ))
            };
            print_success(output, format, cli.quiet)?;
            Ok(0)
        }
        Command::Dir => {
            let info = store.dir_info();
            let output = if format == OutputFormat::Json {
                CommandOutput::Json(json!({
                    "ok": true,
                    "command": "dir",
                    "paths": {
                        "root": info.root,
                        "config_path": info.config_path,
                        "lists_dir": info.lists_dir,
                        "list_dir": info.list_dir,
                        "manifest_path": info.manifest_path,
                        "tasks_dir": info.tasks_dir,
                        "lock_path": info.lock_path,
                        "highwatermark_path": info.highwatermark_path,
                    }
                }))
            } else {
                CommandOutput::Text(text_for_dir(&info))
            };
            print_success(output, format, cli.quiet)?;
            Ok(0)
        }
        Command::Create(args) => {
            let input = create_input(args, context.default_owner)?;
            let (manifest, task) = store.create_task(input)?;
            let output = if format == OutputFormat::Json {
                CommandOutput::Json(envelope(
                    "create",
                    &manifest,
                    json!({
                        "task": task,
                    }),
                ))
            } else {
                CommandOutput::Text(format!(
                    "Created task #{}: {}",
                    task.task.id, task.task.subject
                ))
            };
            print_success(output, format, cli.quiet)?;
            Ok(0)
        }
        Command::List(args) => {
            let filters = list_filters(args);
            let (manifest, tasks) = store.list_task_views(&filters)?;
            let output = if format == OutputFormat::Json {
                CommandOutput::Json(envelope(
                    "list",
                    &manifest,
                    json!({
                        "tasks": tasks,
                    }),
                ))
            } else {
                CommandOutput::Text(text_for_list(&tasks))
            };
            print_success(output, format, cli.quiet)?;
            Ok(0)
        }
        Command::Get(TaskIdArgs { id }) => {
            let (manifest, task) = store.get_task_view(&id)?;
            let output = if format == OutputFormat::Json {
                CommandOutput::Json(envelope(
                    "get",
                    &manifest,
                    json!({
                        "task": task,
                    }),
                ))
            } else {
                CommandOutput::Text(text_for_task(&task))
            };
            print_success(output, format, cli.quiet)?;
            Ok(0)
        }
        Command::Update(args) => {
            let task_id = args.id.clone();
            let input = update_input(args)?;
            let (manifest, task, updated_fields) = store.update_task(&task_id, input)?;
            let output = if format == OutputFormat::Json {
                CommandOutput::Json(envelope(
                    "update",
                    &manifest,
                    json!({
                        "task": task,
                        "updated_fields": updated_fields,
                    }),
                ))
            } else {
                CommandOutput::Text(format!(
                    "Updated task #{} {}",
                    task.task.id,
                    updated_fields.join(", ")
                ))
            };
            print_success(output, format, cli.quiet)?;
            Ok(0)
        }
        Command::Start(args) => {
            update_status_shortcut(&store, format, cli.quiet, args, TaskStatus::InProgress)
        }
        Command::Done(args) => {
            update_status_shortcut(&store, format, cli.quiet, args, TaskStatus::Completed)
        }
        Command::Reopen(TaskIdArgs { id }) => {
            let input = UpdateTaskInput {
                status: Some(TaskStatus::Pending),
                force: true,
                ..Default::default()
            };
            let (manifest, task, _) = store.update_task(&id, input)?;
            let output = if format == OutputFormat::Json {
                CommandOutput::Json(envelope("reopen", &manifest, json!({ "task": task })))
            } else {
                CommandOutput::Text(format!("Reopened task #{}", task.task.id))
            };
            print_success(output, format, cli.quiet)?;
            Ok(0)
        }
        Command::Claim(args) => {
            let owner = args.owner.or(context.default_owner).ok_or_else(|| {
                TkError::usage("claim requires --owner or TK_OWNER/default_owner")
            })?;
            let (manifest, task) = store.claim_task(
                &args.id,
                &owner,
                args.start,
                args.check_busy,
                args.if_revision,
            )?;
            let output = if format == OutputFormat::Json {
                CommandOutput::Json(envelope("claim", &manifest, json!({ "task": task })))
            } else {
                CommandOutput::Text(format!("Claimed task #{} for {}", task.task.id, owner))
            };
            print_success(output, format, cli.quiet)?;
            Ok(0)
        }
        Command::Unclaim(args) => {
            let (manifest, task) = store.unclaim_task(&args.id, args.requeue, args.if_revision)?;
            let output = if format == OutputFormat::Json {
                CommandOutput::Json(envelope("unclaim", &manifest, json!({ "task": task })))
            } else {
                CommandOutput::Text(format!("Unclaimed task #{}", task.task.id))
            };
            print_success(output, format, cli.quiet)?;
            Ok(0)
        }
        Command::Next(args) => {
            let owner = args.owner.or(context.default_owner);
            let (manifest, task) =
                store.next_task(args.claim, owner.as_deref(), args.start, args.check_busy)?;
            let output = if format == OutputFormat::Json {
                CommandOutput::Json(envelope("next", &manifest, json!({ "task": task })))
            } else {
                CommandOutput::Text(text_for_task(&task))
            };
            print_success(output, format, cli.quiet)?;
            Ok(0)
        }
        Command::Block { command } => match command {
            BlockCommand::Add(args) => {
                let (manifest, task) = store.add_blockers(&args.task_id, &args.blocker_ids)?;
                let output = if format == OutputFormat::Json {
                    CommandOutput::Json(envelope("block_add", &manifest, json!({ "task": task })))
                } else {
                    CommandOutput::Text(format!("Updated blockers for task #{}", task.task.id))
                };
                print_success(output, format, cli.quiet)?;
                Ok(0)
            }
            BlockCommand::Remove(args) => {
                let (manifest, task) = store.remove_blockers(&args.task_id, &args.blocker_ids)?;
                let output = if format == OutputFormat::Json {
                    CommandOutput::Json(envelope(
                        "block_remove",
                        &manifest,
                        json!({ "task": task }),
                    ))
                } else {
                    CommandOutput::Text(format!("Removed blockers from task #{}", task.task.id))
                };
                print_success(output, format, cli.quiet)?;
                Ok(0)
            }
        },
        Command::Delete(args) => {
            let manifest = store.delete_task(&args.id, args.detach, args.if_revision)?;
            let output = if format == OutputFormat::Json {
                CommandOutput::Json(envelope(
                    "delete",
                    &manifest,
                    json!({
                        "task_id": args.id,
                    }),
                ))
            } else {
                CommandOutput::Text(format!("Deleted task #{}", args.id))
            };
            print_success(output, format, cli.quiet)?;
            Ok(0)
        }
        Command::Reset(args) => {
            let manifest = store.reset(args.force)?;
            let output = if format == OutputFormat::Json {
                CommandOutput::Json(envelope("reset", &manifest, json!({})))
            } else {
                CommandOutput::Text(format!("Reset list {}", manifest.list_id))
            };
            print_success(output, format, cli.quiet)?;
            Ok(0)
        }
        Command::Verify => {
            let (manifest, diagnostics) = store.verify()?;
            let has_errors = diagnostics
                .iter()
                .any(|diagnostic| matches!(diagnostic.level, DiagnosticLevel::Error));
            let manifest = manifest.unwrap_or_else(|| {
                crate::model::Manifest::new(store.paths.list_id.clone(), None, None)
            });
            let output = if format == OutputFormat::Json {
                CommandOutput::Json(envelope(
                    "verify",
                    &manifest,
                    json!({
                        "diagnostics": diagnostics,
                    }),
                ))
            } else {
                CommandOutput::Text(text_for_verify(&diagnostics))
            };
            print_success(output, format, cli.quiet)?;
            Ok(if has_errors {
                ErrorCode::ValidationError.exit_code()
            } else {
                0
            })
        }
        Command::Watch(args) => {
            if format != OutputFormat::Ndjson {
                return Err(TkError::usage("watch requires --format ndjson"));
            }
            watch_list(&store, args.interval_ms, true)?;
            Ok(0)
        }
    }
}

fn create_input(
    args: CreateArgs,
    default_owner: Option<String>,
) -> Result<CreateTaskInput, TkError> {
    let file_body = match args.json_body {
        Some(path) => {
            let contents = fs::read_to_string(&path)
                .map_err(|err| TkError::storage(format!("failed to read {path}: {err}")))?;
            serde_json::from_str::<CreateJsonBody>(&contents).map_err(|err| {
                TkError::validation(format!("failed to parse create JSON body {path}: {err}"))
            })?
        }
        None => CreateJsonBody::default(),
    };

    let subject = args
        .subject
        .or(file_body.subject)
        .ok_or_else(|| TkError::usage("create requires a subject"))?;
    let description = args
        .description
        .or(file_body.description)
        .unwrap_or_default();
    let active_form = args.active_form.or(file_body.active_form);
    let owner = args.owner.or(default_owner).or(file_body.owner);
    let visibility = args
        .visibility
        .or(file_body.visibility)
        .unwrap_or(Visibility::Public);
    let mut metadata = file_body.metadata.unwrap_or_default();
    for entry in args.metadata {
        let (key, value) = parse_key_value(&entry)?;
        metadata.insert(key, value);
    }
    Ok(CreateTaskInput {
        subject,
        description,
        active_form,
        owner,
        visibility,
        metadata,
    })
}

fn update_input(args: crate::cli::UpdateArgs) -> Result<UpdateTaskInput, TkError> {
    if args.owner.is_some() && args.clear_owner {
        return Err(TkError::usage(
            "--owner and --clear-owner cannot be used together",
        ));
    }
    let mut set_metadata = BTreeMap::new();
    for entry in args.set_metadata {
        let (key, value) = parse_key_value(&entry)?;
        set_metadata.insert(key, value);
    }
    Ok(UpdateTaskInput {
        subject: args.subject,
        description: args.description,
        active_form: args.active_form,
        status: args.status,
        owner: args.owner,
        clear_owner: args.clear_owner,
        visibility: args.visibility,
        set_metadata,
        unset_metadata: args.unset_metadata,
        if_revision: args.if_revision,
        force: args.force,
    })
}

fn list_filters(args: ListArgs) -> ListFilters {
    ListFilters {
        statuses: args.status.into_iter().collect::<BTreeSet<_>>(),
        owner: args.owner,
        unowned: args.unowned,
        claimable: args.claimable,
        include_internal: args.include_internal,
        limit: args.limit,
    }
}

fn parse_key_value(input: &str) -> Result<(String, Value), TkError> {
    let (key, raw_value) = input
        .split_once('=')
        .ok_or_else(|| TkError::usage(format!("invalid key=value pair: {input}")))?;
    if key.is_empty() {
        return Err(TkError::usage("metadata key must not be empty"));
    }
    let value = match serde_json::from_str::<Value>(raw_value) {
        Ok(value) => value,
        Err(_) => Value::String(raw_value.to_owned()),
    };
    Ok((key.to_owned(), value))
}

fn update_status_shortcut(
    store: &ListStore,
    format: OutputFormat,
    quiet: bool,
    args: StatusShortcutArgs,
    status: TaskStatus,
) -> Result<i32, TkError> {
    let input = UpdateTaskInput {
        status: Some(status),
        if_revision: args.if_revision,
        force: args.force,
        ..Default::default()
    };
    let (manifest, task, _) = store.update_task(&args.id, input)?;
    let command_name = match status {
        TaskStatus::InProgress => "start",
        TaskStatus::Completed => "done",
        TaskStatus::Pending => "update",
    };
    let output = if format == OutputFormat::Json {
        CommandOutput::Json(envelope(command_name, &manifest, json!({ "task": task })))
    } else {
        CommandOutput::Text(format!(
            "Updated task #{} to {}",
            task.task.id,
            status.as_str()
        ))
    };
    print_success(output, format, quiet)?;
    Ok(0)
}
