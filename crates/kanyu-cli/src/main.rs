//! `kanyu` 二进制入口。

mod cli;
mod commands;

use clap::Parser;

use crate::cli::{Cli, Command};

fn main() -> anyhow::Result<()> {
    let args = Cli::parse();
    match &args.command {
        Command::Data(cmd) => commands::data(cmd, args.json),
        Command::Introspect => commands::introspect_cmd(args.json),
        Command::Agents(cmd) => commands::agents_cmd(cmd, args.json),
        Command::Mcp(cmd) => commands::mcp_cmd(cmd),
    }
}
