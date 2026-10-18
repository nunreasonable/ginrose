mod config;
mod desktop;
mod discord;
mod flags;
mod launch;
mod machine;
mod paths;
mod presence;
mod roblox;
mod rosemary;
mod settings;
mod setup;
mod ui;

use std::process::ExitCode;

const USAGE: &str = "usage: ginrose [command]

  (none)                  open the launcher (the setup, on the first run)
  setup                   run the setup again
  launch [link]           start Roblox, joining the game a roblox-player: or roblox: link names
  update                  download the current Roblox if it is not installed
  rosemary <path>         copy a Rosemary binary into Ginrose and use it
  install-desktop [--default]
                          add menu entries; --default also opens Roblox links with Ginrose
  uninstall-desktop       remove the menu entries
  status                  show what is installed and what would get in the way";

fn print_progress(text: &str, fraction: Option<f64>) {
    match fraction {
        Some(f) => eprint!("\r{text} {:3.0}%   ", f * 100.0),
        None => eprintln!("{text}"),
    }
}

fn cli_launch(uri: Option<String>) -> ExitCode {
    let config = config::Config::load();
    let prepared = match launch::prepare(&config, uri.as_deref(), &print_progress) {
        Ok(prepared) => prepared,
        Err(e) => {
            eprintln!("\nginrose: {e}");
            return ExitCode::FAILURE;
        }
    };
    eprintln!();
    for warning in &prepared.warnings {
        eprintln!("ginrose: {}: {}", warning.title, warning.detail);
    }
    let child = match launch::spawn(&config, &prepared, uri.as_deref()) {
        Ok(child) => child,
        Err(e) => {
            eprintln!("ginrose: {e}");
            return ExitCode::FAILURE;
        }
    };
    let child = std::cell::RefCell::new(child.child);
    presence::follow(&config.discord, &|| {
        !matches!(child.borrow_mut().try_wait(), Ok(Some(_)))
    });
    ExitCode::SUCCESS
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        None => ui::run(None, false),
        Some("setup") => ui::run(None, true),
        // A link from the browser opens a small window while Roblox gets ready.
        Some("launch") if std::env::var_os("GINROSE_NO_WINDOW").is_none() && ui::has_display() => {
            ui::run(Some(args.get(1).cloned().unwrap_or_default()), false)
        }
        Some("launch") => cli_launch(args.get(1).cloned()),
        Some("update") => match roblox::ensure(true, &print_progress) {
            Ok(build) => {
                eprintln!("\nRoblox {} is installed", build.version);
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("\nginrose: {e}");
                ExitCode::FAILURE
            }
        },
        Some("rosemary") => {
            let Some(source) = args.get(1) else {
                eprintln!("{USAGE}");
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
                    eprintln!("ginrose: {e}");
                    ExitCode::FAILURE
                }
            }
        }
        Some("install-desktop") => match desktop::install(args.iter().any(|a| a == "--default")) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("ginrose: {e}");
                ExitCode::FAILURE
            }
        },
        Some("uninstall-desktop") => {
            desktop::uninstall();
            ExitCode::SUCCESS
        }
        Some("status") => {
            let config = config::Config::load();
            let rosemary = config.rosemary_path();
            println!(
                "rosemary: {}",
                rosemary
                    .as_ref()
                    .map_or("not found".into(), |p| p.display().to_string())
            );
            println!(
                "roblox:   {}",
                roblox::installed().map_or("not installed".into(), |b| format!(
                    "{} ({})",
                    b.version, b.upload
                ))
            );
            println!(
                "links:    {}",
                if desktop::is_default() {
                    "opened by Ginrose"
                } else {
                    "opened by another program"
                }
            );
            for warning in machine::check(&config, None) {
                println!("warning:  {}: {}", warning.title, warning.detail);
            }
            ExitCode::SUCCESS
        }
        _ => {
            eprintln!("{USAGE}");
            ExitCode::FAILURE
        }
    }
}
