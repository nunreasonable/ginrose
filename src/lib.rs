//! Ginrose and Ginrose Studio. Both binaries are this library with a different
//! [`Flavor`]; see `flavor.rs`.

mod cli;
mod config;
mod desktop;
mod discord;
mod flags;
mod flavor;
mod integrity;
mod launch;
mod machine;
mod paths;
mod presence;
mod roblox;
mod rosemary;
mod settings;
mod setup;
mod studio;
mod studio_ui;
mod ui;

pub use flavor::Flavor;

/// Run as `flavor`, with the process's arguments.
pub fn main(flavor: Flavor) -> std::process::ExitCode {
    flavor::set(flavor);
    cli::main()
}
