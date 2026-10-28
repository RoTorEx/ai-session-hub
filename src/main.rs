use anyhow::Result;
use clap::{Parser, Subcommand};
use std::io::IsTerminal;
mod usage;
use serde_json::json;
mod activity;
mod benchmark;
mod command;
mod config;
mod content;
mod iterm;
mod model;
mod process;
mod session_data;
mod theme;
mod tui;
mod update;

#[derive(Parser)]
#[command(
    version,
    about = "Find live Codex and Claude sessions in iTerm2",
    args_conflicts_with_subcommands = true
)]
struct Args {
    /// Print a fresh plain list and exit
    #[arg(long)]
    plain: bool,
    #[command(subcommand)]
    command: Option<Action>,
}
#[derive(Subcommand)]
enum Action {
    /// Print currently live interactive sessions
    List {
        #[arg(long)]
        json: bool,
    },
    /// Revalidate and select an existing iTerm2 pane
    Jump {
        id: String,
    },
    /// Measure local discovery/content costs; Jump is opt-in
    Benchmark {
        #[arg(long, default_value_t = 10)]
        samples: usize,
        /// Also measure selection of this exact live session (changes focus)
        #[arg(long)]
        jump: Option<String>,
    },
    Version,
    /// Install the latest Apple Silicon GitHub Release
    Update,
    /// Create starter configuration, preserving existing settings
    SetupConfig,
    Themes,
    Theme {
        name: String,
    },
}

fn run(args: Args) -> Result<()> {
    let action = match args.command {
        Some(a) => a,
        None if args.plain => Action::List { json: false },
        None if std::io::stdin().is_terminal() && std::io::stdout().is_terminal() => {
            let config = config::load()?;
            let theme = theme::get(&config.theme)?;
            return tui::run(config, theme);
        }
        None => {
            use clap::CommandFactory;
            Args::command().print_help()?;
            println!();
            return Ok(());
        }
    };
    usage::event(
        "command_selected",
        "started",
        json!({"mode":match &action {Action::Benchmark{..}=>"benchmark",Action::Version=>"version",Action::Update=>"update",Action::SetupConfig=>"setup",Action::Theme{..}=>"theme",Action::Themes=>"themes",Action::List{..}=>"list",Action::Jump{..}=>"jump"}}),
    );
    match action {
        Action::Benchmark { samples, jump } => benchmark::run(samples, jump.as_deref())?,
        Action::Version => println!("ai-session-hub {}", env!("CARGO_PKG_VERSION")),
        Action::Update => update::run()?,
        Action::SetupConfig => config::setup()?,
        Action::Theme { name } => config::set_theme(&name)?,
        Action::Themes => {
            let active = config::load()?.theme;
            for name in theme::NAMES {
                println!("{} {name}", if name == active { "*" } else { " " });
            }
        }
        Action::List { json } => {
            let sessions = model::collect()?;
            usage::event(
                "sessions_listed",
                "success",
                json!({"count":sessions.len(),"linked":sessions.iter().filter(|s|s.location.is_some()).count()}),
            );
            if json {
                println!("{}", serde_json::to_string_pretty(&sessions)?);
            } else {
                if sessions.is_empty() {
                    println!("No live Codex/Claude sessions");
                }
                for s in sessions {
                    println!(
                        "{} {} {} {} {}",
                        s.id,
                        s.provider,
                        model::safe_text(&s.project),
                        s.status,
                        s.location
                            .as_ref()
                            .map(|p| format!(
                                "{} / tab {} / pane {}",
                                model::safe_text(&p.window),
                                p.tab_index + 1,
                                p.pane_index + 1
                            ))
                            .unwrap_or_else(|| format!(
                                "Unlinked: {}",
                                model::safe_text(s.link_reason.as_deref().unwrap_or("Unknown"))
                            ))
                    );
                }
            }
        }
        Action::Jump { id } => {
            let mut collector = model::Collector::new(30);
            let s = collector
                .scan(true)?
                .into_iter()
                .find(|s| s.id == id)
                .ok_or_else(|| anyhow::anyhow!("Session not found"))?;
            model::jump_with(&s, &mut collector.terminal)?;
            println!("Selected {}", s.id);
        }
    }
    Ok(())
}
fn main() {
    if let Ok(path) = config::path() {
        usage::init(
            "ai-session-hub",
            path.with_file_name("logs").join("events.jsonl"),
        );
    }
    let args = match Args::try_parse() {
        Ok(args) => args,
        Err(e) => {
            usage::event(
                "cli_parse",
                if e.use_stderr() { "error" } else { "no_op" },
                json!({}),
            );
            e.exit()
        }
    };
    let operation = usage::Operation::start("command");
    let result = run(args);
    operation.finish(if result.is_ok() { "success" } else { "error" }, json!({}));
    if let Err(e) = result {
        eprintln!("ai-session-hub: {e:#}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn script_commands_do_not_need_a_terminal_and_plain_conflicts_with_subcommands() {
        for args in [
            vec!["ai-session-hub", "list", "--json"],
            vec!["ai-session-hub", "theme", "ocean"],
            vec!["ai-session-hub", "--plain"],
        ] {
            assert!(Args::try_parse_from(args).is_ok());
        }
        assert!(Args::try_parse_from(["ai-session-hub", "--plain", "list"]).is_err());
    }
}
