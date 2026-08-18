//! `kanyu` 二进制入口。

mod cli;
mod commands;

use clap::Parser;

use crate::cli::{Cli, Command};

fn main() -> anyhow::Result<()> {
    let args = Cli::parse();
    match &args.command {
        Command::Data(cmd) => commands::data(cmd, args.json),
        Command::Analysis(cmd) => commands::analysis(cmd, args.json),
        Command::Render(cmd) => commands::render(cmd),
        Command::Skill(cmd) => commands::skill(cmd, args.json),
        Command::Introspect => commands::introspect_cmd(args.json),
        Command::Agents(cmd) => commands::agents_cmd(cmd, args.json),
        Command::Mcp(cmd) => commands::mcp_cmd(cmd),
        Command::Toolbox(cmd) => commands::toolbox(cmd, args.json),
        Command::Crs(cmd) => commands::crs(cmd, args.json),
    }
}
