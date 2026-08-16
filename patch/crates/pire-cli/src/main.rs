use std::process::ExitCode;

mod app;
mod approval;
mod builtins;
mod cli;
mod config;
mod error;
mod interactive;
mod output;
mod resources;
mod shell;
mod trust;

fn main() -> ExitCode {
    app::main_exit()
}
