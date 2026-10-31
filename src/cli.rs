use std::process::ExitCode;

use crate::flavor::{self, Flavor};
use crate::{config, desktop, integrity, launch, machine, paths, presence, roblox, ui};

const USAGE: &str = "usage: ginrose [command]

  (none)                  open the launcher (the setup, on the first run)
  setup                   run the setup again
  launch [link]           start Roblox, joining the game a roblox-player: or roblox: link names
  update                  download the current Roblox if it is not installed
  verify [Roblox.app]     check that Roblox and the prefix are exactly as signed
  repair                  download a clean Roblox and put the prefix's system files back
  rosemary <path>         copy a Rosemary binary into Ginrose and use it
  install-desktop [--default]
                          add menu entries; --default also opens Roblox links with Ginrose
  uninstall-desktop       remove the menu entries
  status                  show what is installed and what would get in the way";

const STUDIO_USAGE: &str = "usage: ginrose-studio [command]

  (none)                  open the launcher (the setup, on the first run)
  setup                   run the setup again
  launch [link or file]   start Roblox Studio, opening a roblox-studio: or
                          roblox-studio-auth: link or a place file; a running
                          Studio is handed the link instead
  update                  download the current Roblox Studio if it is not installed
  repair                  download a clean Roblox Studio and put the prefix's system files back
  rosemary <path>         copy a Rosemary binary into Ginrose Studio and use it
  install-desktop [--default]
                          add menu entries; --default also opens Studio links with Ginrose Studio
  uninstall-desktop       remove the menu entries
  status                  show what is installed, where, and what would get in the way";

fn usage() -> &'static str {
    match flavor::current() {
        Flavor::Player => USAGE,
        Flavor::Studio => STUDIO_USAGE,
    }
}

fn print_progress(text: &str, fraction: Option<f64>) {
    match fraction {
        Some(f) => eprint!("\r{text} {:3.0}%   ", f * 100.0),
        None => eprintln!("{text}"),
    }
}

fn command() -> &'static str {
    flavor::current().command()
}

fn cli_launch(uri: Option<String>) -> ExitCode {
    let config = config::Config::load();
    let prepared = match launch::prepare(&config, uri.as_deref(), &print_progress) {
        Ok(prepared) => prepared,
        Err(e) => {
            eprintln!("\n{}: {e}", command());
            return ExitCode::FAILURE;
        }
    };
    eprintln!();
    for warning in &prepared.warnings {
        eprintln!("{}: {}: {}", command(), warning.title, warning.detail);
    }
    let child = match launch::spawn(&config, &prepared, uri.as_deref()) {
        Ok(child) => child,
        Err(e) => {
            eprintln!("{}: {e}", command());
            return ExitCode::FAILURE;
        }
    };
    if flavor::is_studio() {
        // No Discord for Studio: wait for it, or for the hand-off to the
        // Studio already running.
        let mut child = child.child;
        return match child.wait() {
            Ok(status) if status.success() => ExitCode::SUCCESS,
            _ => ExitCode::FAILURE,
        };
    }
    let child = std::cell::RefCell::new(child.child);
    presence::follow(&config.discord, &|| {
        !matches!(child.borrow_mut().try_wait(), Ok(Some(_)))
    });
    ExitCode::SUCCESS
}

fn cli_verify() -> ExitCode {
    let mut ok = true;
    match roblox::installed() {
        Some(build) => {
            let started = std::time::Instant::now();
            match integrity::client(&build.app()) {
                Ok(()) => println!(
                    "roblox:   {} as Roblox signed it ({:.1} s)",
                    build.version,
                    started.elapsed().as_secs_f64()
                ),
                Err(e) => {
                    println!("roblox:   modified: {e}");
                    ok = false;
                }
            }
        }
        None => println!("roblox:   not installed"),
    }
    match config::Config::load().rosemary_path() {
        _ if !paths::prefix().join("root").is_dir() => println!("prefix:   not created yet"),
        Some(rosemary) => match integrity::prefix(&rosemary, false) {
            Ok(()) => println!("prefix:   system files match the pinned base"),
            Err(e) => {
                println!("prefix:   {e}");
                ok = false;
            }
        },
        None => println!("prefix:   no Rosemary to check it with"),
    }
    if ok { ExitCode::SUCCESS } else { ExitCode::FAILURE }
}

fn cli_repair() -> ExitCode {
    if flavor::is_studio() {
        return match launch::repair(&print_progress) {
            Ok(build) => {
                eprintln!("\nRoblox Studio {} is installed again", build.version);
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("\nginrose-studio: {e}");
                ExitCode::FAILURE
            }
        };
    }
    launch::stop_running();
    if let Err(e) = roblox::reinstall(&print_progress) {
        eprintln!("\nginrose: {e}");
        return ExitCode::FAILURE;
    }
    eprintln!();
    if let Some(rosemary) = config::Config::load().rosemary_path()
        && paths::prefix().join("root").is_dir()
        && let Err(e) = integrity::repair_prefix(&rosemary)
    {
        eprintln!("ginrose: could not repair the prefix: {e}");
        return ExitCode::FAILURE;
    }
    cli_verify()
}

fn cli_rosemary(source: Option<&String>) -> ExitCode {
    let Some(source) = source else {
        eprintln!("{}", usage());
        return ExitCode::FAILURE;
    };
    let target = paths::bundled_rosemary();
    let tmp = target.with_extension("new");
    let result = std::fs::create_dir_all(target.parent().unwrap())
        .and_then(|_| std::fs::copy(source, &tmp))
        .and_then(|_| std::fs::rename(&tmp, &target));
    match result {
        Ok(()) => {
            let mut config = config::Config::load();
            config.rosemary.clear();
            let _ = config.save();
            eprintln!("Rosemary installed at {}", target.display());
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("{}: {e}", command());
            ExitCode::FAILURE
        }
    }
}

fn cli_status() -> ExitCode {
    let flavor = flavor::current();
    let config = config::Config::load();
    let rosemary = config.rosemary_path();
    println!(
        "rosemary: {}",
        rosemary
            .as_ref()
            .map_or("not found".into(), |p| p.display().to_string())
    );
    let client = match flavor {
        Flavor::Player => "roblox:  ",
        Flavor::Studio => "studio:  ",
    };
    println!(
        "{client} {}",
        roblox::installed().map_or("not installed".into(), |b| format!(
            "{} ({})",
            b.version, b.upload
        ))
    );
    println!(
        "links:    {}",
        match (desktop::is_default(), flavor) {
            (true, Flavor::Player) => "opened by Ginrose",
            (true, Flavor::Studio) => "opened by Ginrose Studio",
            (false, _) => "opened by another program",
        }
    );
    if flavor == Flavor::Studio {
        let running = launch::running_builds();
        if !running.is_empty() {
            println!(
                "running:  {}",
                running.into_iter().collect::<Vec<_>>().join(", ")
            );
        }
        println!("prefix:   {}", paths::prefix().display());
        println!("plugins:  {}", paths::studio_plugins().display());
        println!("logs:     {}", paths::logs_dir().display());
    }
    for warning in machine::check(&config, None) {
        println!("warning:  {}: {}", warning.title, warning.detail);
    }
    ExitCode::SUCCESS
}

pub fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let studio = flavor::is_studio();
    match args.first().map(String::as_str) {
        None if studio => ui::run_studio(Vec::new()),
        Some("setup") if studio => ui::run_studio(args),
        None => ui::run(None, false),
        Some("setup") => ui::run(None, true),
        // A link from the browser opens a small window while Roblox gets ready.
        Some("launch") if std::env::var_os("GINROSE_NO_WINDOW").is_none() && ui::has_display() => {
            match studio {
                true => ui::run_studio(args),
                false => ui::run(Some(args.get(1).cloned().unwrap_or_default()), false),
            }
        }
        Some("launch") => cli_launch(args.get(1).cloned()),
        Some("update") => match roblox::ensure(true, &print_progress) {
            Ok(build) => {
                eprintln!(
                    "\n{} {} is installed",
                    flavor::current().client(),
                    build.version
                );
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("\n{}: {e}", command());
                ExitCode::FAILURE
            }
        },
        Some("verify") if !studio => match args.get(1) {
            Some(app) => match integrity::client(std::path::Path::new(app)) {
                Ok(()) => {
                    println!("{app}: as Roblox signed it");
                    ExitCode::SUCCESS
                }
                Err(e) => {
                    println!("{app}: modified: {e}");
                    ExitCode::FAILURE
                }
            },
            None => cli_verify(),
        },
        Some("repair") => cli_repair(),
        Some("rosemary") if paths::in_flatpak() && !studio => {
            eprintln!("ginrose: the Flatpak runs only the Rosemary it comes with");
            ExitCode::FAILURE
        }
        Some("rosemary") => cli_rosemary(args.get(1)),
        Some("install-desktop") => match desktop::install(args.iter().any(|a| a == "--default")) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("{}: {e}", command());
                ExitCode::FAILURE
            }
        },
        Some("uninstall-desktop") => {
            desktop::uninstall();
            ExitCode::SUCCESS
        }
        Some("status") => cli_status(),
        _ => {
            eprintln!("{}", usage());
            ExitCode::FAILURE
        }
    }
}
