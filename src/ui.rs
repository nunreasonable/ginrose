use std::cell::RefCell;
use std::process::ExitCode;
use std::rc::Rc;
use std::sync::mpsc;
use std::time::Duration;

use adw::prelude::*;
use gtk::{gio, glib};
use serde_json::Value;

use crate::config::{Config, Profile};
use crate::{desktop, flags, launch, presence, roblox};

type Shared = Rc<RefCell<Config>>;

pub fn has_display() -> bool {
    std::env::var_os("WAYLAND_DISPLAY").is_some() || std::env::var_os("DISPLAY").is_some()
}

enum Message {
    Progress(String, Option<f64>),
    Warning(String, String),
    Started(Option<String>),
    /// Roblox's window is on screen, or it is taking too long to say so.
    Visible,
    Failed(String),
    Exited,
}

/// Prepare and start Roblox on a thread of its own, then follow it for Discord
/// until it closes. Everything the window needs to know arrives as messages.
fn start(config: Config, uri: Option<String>) -> mpsc::Receiver<Message> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let progress_tx = tx.clone();
        let progress = move |text: &str, fraction: Option<f64>| {
            let _ = progress_tx.send(Message::Progress(text.to_string(), fraction));
        };
        let prepared = match launch::prepare(&config, uri.as_deref(), &progress) {
            Ok(prepared) => prepared,
            Err(e) => {
                let _ = tx.send(Message::Failed(e));
                return;
            }
        };
        for warning in &prepared.warnings {
            let _ = tx.send(Message::Warning(
                warning.title.clone(),
                warning.detail.clone(),
            ));
        }
        let child = match launch::spawn(&config, &prepared, uri.as_deref()) {
            Ok(child) => child,
            Err(e) => {
                let _ = tx.send(Message::Failed(e));
                return;
            }
        };
        let _ = tx.send(Message::Started(prepared.profile.clone()));
        let (visible_tx, log) = (tx.clone(), child.log.clone());
        std::thread::spawn(move || {
            launch::wait_for_window(&log, Duration::from_secs(20));
            let _ = visible_tx.send(Message::Visible);
        });
        let child = RefCell::new(child.child);
        presence::follow(&config.discord, &|| {
            !matches!(child.borrow_mut().try_wait(), Ok(Some(_)))
        });
        let _ = tx.send(Message::Exited);
    });
    rx
}

/// Deliver a thread's messages on the main loop, until the sender is gone.
fn receive(rx: mpsc::Receiver<Message>, mut handle: impl FnMut(Message) + 'static) {
    glib::timeout_add_local(Duration::from_millis(100), move || {
        loop {
            match rx.try_recv() {
                Ok(message) => handle(message),
                Err(mpsc::TryRecvError::Empty) => return glib::ControlFlow::Continue,
                Err(mpsc::TryRecvError::Disconnected) => return glib::ControlFlow::Break,
            }
        }
    });
}

pub fn run(launch_uri: Option<String>, force_setup: bool) -> ExitCode {
    let app = adw::Application::builder()
        .application_id(desktop::app_id())
        .flags(gio::ApplicationFlags::NON_UNIQUE)
        .build();
    let uri = Rc::new(launch_uri);
    app.connect_startup(|_| gtk::Window::set_default_icon_name(desktop::app_id()));
    app.connect_activate(move |app| match uri.as_ref() {
        Some(uri) => launch_window(app, uri.clone()),
        None if force_setup || !Config::load().setup_done => {
            let next = app.clone();
            crate::setup::window(app, move || main_window(&next));
        }
        None => main_window(app),
    });
    match app.run_with_args(&[] as &[&str]) {
        glib::ExitCode::SUCCESS => ExitCode::SUCCESS,
        _ => ExitCode::FAILURE,
    }
}

/// The splash a browser link opens: the logo while Roblox gets ready, gone the
/// moment Roblox's own window is up. Downloads show a progress bar in it. The
/// process stays after it closes, to keep Discord up to date.
fn launch_window(app: &adw::Application, uri: String) {
    let config = Config::load();
    let label = gtk::Label::new(Some("Starting Roblox"));
    label.add_css_class("title-3");
    let spinner = adw::Spinner::new();
    spinner.set_size_request(24, 24);
    let bar = gtk::ProgressBar::new();
    bar.set_visible(false);
    let detail = gtk::Label::new(None);
    detail.set_wrap(true);
    detail.set_justify(gtk::Justification::Center);
    detail.add_css_class("dim-label");
    let content = gtk::Box::new(gtk::Orientation::Vertical, 14);
    content.set_valign(gtk::Align::Center);
    content.set_margin_top(28);
    content.set_margin_bottom(28);
    content.set_margin_start(28);
    content.set_margin_end(28);
    content.append(&crate::setup::logo(96));
    content.append(&label);
    content.append(&spinner);
    content.append(&bar);
    content.append(&detail);
    let window = adw::ApplicationWindow::builder()
        .application(app)
        .title("Ginrose")
        .default_width(340)
        .resizable(false)
        .content(&content)
        .build();
    window.present();
    let hold = app.hold();
    let hold = Rc::new(RefCell::new(Some(hold)));
    let window_ref = window.clone();
    receive(start(config, Some(uri)), move |message| match message {
        Message::Progress(text, fraction) => {
            detail.set_text(&text);
            if let Some(f) = fraction {
                spinner.set_visible(false);
                bar.set_visible(true);
                bar.set_fraction(f);
            }
        }
        Message::Warning(..) => {}
        Message::Started(_) => {
            bar.set_visible(false);
            spinner.set_visible(true);
            detail.set_text("");
            label.set_text("Opening Roblox");
        }
        Message::Visible => window_ref.set_visible(false),
        Message::Failed(e) => {
            label.set_text("Roblox could not start");
            detail.set_text(&e);
            spinner.set_visible(false);
            bar.set_visible(false);
            window_ref.set_visible(true);
            hold.borrow_mut().take();
        }
        Message::Exited => {
            hold.borrow_mut().take();
            window_ref.close();
        }
    });
}

fn save(config: &Shared) {
    let _ = config.borrow().save();
}

fn main_window(app: &adw::Application) {
    let config: Shared = Rc::new(RefCell::new(Config::load()));
    let toasts = adw::ToastOverlay::new();
    let stack = adw::ViewStack::new();
    stack.add_titled_with_icon(
        &play_page(&config, &toasts, app),
        Some("play"),
        "Play",
        "media-playback-start-symbolic",
    );
    stack.add_titled_with_icon(
        &settings_page(&config, &toasts),
        Some("settings"),
        "Settings",
        "emblem-system-symbolic",
    );
    stack.add_titled_with_icon(
        &profiles_page(&config, &toasts),
        Some("profiles"),
        "Profiles",
        "view-list-symbolic",
    );
    let switcher = adw::ViewSwitcher::builder()
        .stack(&stack)
        .policy(adw::ViewSwitcherPolicy::Wide)
        .build();
    let header = adw::HeaderBar::new();
    header.set_title_widget(Some(&switcher));
    toasts.set_child(Some(&stack));
    let view = adw::ToolbarView::new();
    view.add_top_bar(&header);
    view.set_content(Some(&toasts));
    let window = adw::ApplicationWindow::builder()
        .application(app)
        .title("Ginrose")
        .default_width(720)
        .default_height(640)
        .content(&view)
        .build();
    window.present();
}

fn play_page(config: &Shared, toasts: &adw::ToastOverlay, app: &adw::Application) -> gtk::Widget {
    let status = adw::StatusPage::builder().title("Ginrose").build();
    status.set_paintable(crate::setup::logo(128).paintable().as_ref());
    let describe = || {
        let roblox = roblox::installed().map_or("Roblox not installed yet".to_string(), |b| {
            format!("Roblox {}", b.version)
        });
        let rosemary = Config::load()
            .rosemary_path()
            .map_or("Rosemary not found".to_string(), |p| {
                format!("Rosemary: {}", p.display())
            });
        format!("{roblox}\n{rosemary}")
    };
    status.set_description(Some(&describe()));
    let button = gtk::Button::with_label("Play");
    button.add_css_class("suggested-action");
    button.add_css_class("pill");
    button.set_halign(gtk::Align::Center);
    let bar = gtk::ProgressBar::new();
    bar.set_visible(false);
    let line = gtk::Label::new(None);
    line.add_css_class("dim-label");
    line.set_wrap(true);
    let banners = gtk::Box::new(gtk::Orientation::Vertical, 6);
    let column = gtk::Box::new(gtk::Orientation::Vertical, 12);
    column.set_halign(gtk::Align::Center);
    column.set_width_request(360);
    column.append(&button);
    column.append(&bar);
    column.append(&line);
    column.append(&banners);
    status.set_child(Some(&column));
    for warning in crate::machine::check(&config.borrow(), None) {
        let banner = adw::Banner::new(&format!("{}: {}", warning.title, warning.detail));
        banner.set_revealed(true);
        banners.append(&banner);
    }
    let config = config.clone();
    let toasts = toasts.clone();
    let app = app.clone();
    let page = status.clone();
    button.connect_clicked(move |button| {
        button.set_sensitive(false);
        bar.set_visible(true);
        let hold = Rc::new(RefCell::new(Some(app.hold())));
        let (button, bar, line, toasts, status) = (
            button.clone(),
            bar.clone(),
            line.clone(),
            toasts.clone(),
            page.clone(),
        );
        receive(
            start(config.borrow().clone(), None),
            move |message| match message {
                Message::Progress(text, fraction) => {
                    line.set_text(&text);
                    match fraction {
                        Some(f) => bar.set_fraction(f),
                        None => bar.pulse(),
                    }
                }
                Message::Warning(title, text) => {
                    toasts.add_toast(adw::Toast::new(&format!("{title}: {text}")))
                }
                Message::Started(profile) => {
                    bar.set_visible(false);
                    line.set_text(&match profile {
                        Some(name) => format!("Roblox is running ({name} profile)"),
                        None => "Roblox is running".to_string(),
                    });
                    status.set_description(Some(&describe()));
                }
                Message::Visible => {}
                Message::Failed(e) => {
                    bar.set_visible(false);
                    line.set_text(&e);
                    button.set_sensitive(true);
                    hold.borrow_mut().take();
                }
                Message::Exited => {
                    line.set_text("");
                    button.set_sensitive(true);
                    hold.borrow_mut().take();
                }
            },
        );
    });
    status.upcast()
}

fn switch_row(
    title: &str,
    subtitle: &str,
    active: bool,
    set: impl Fn(bool) + 'static,
) -> adw::SwitchRow {
    let row = adw::SwitchRow::builder()
        .title(title)
        .subtitle(subtitle)
        .active(active)
        .build();
    row.connect_active_notify(move |row| set(row.is_active()));
    row
}

fn combo_row(
    title: &str,
    subtitle: &str,
    items: &[&str],
    selected: u32,
    set: impl Fn(u32) + 'static,
) -> adw::ComboRow {
    let row = adw::ComboRow::builder()
        .title(title)
        .subtitle(subtitle)
        .model(&gtk::StringList::new(items))
        .selected(selected)
        .build();
    row.connect_selected_notify(move |row| set(row.selected()));
    row
}

const QUALITIES: &[&str] = &[
    "Roblox's own",
    "1",
    "2",
    "3",
    "4",
    "5",
    "6",
    "7",
    "8",
    "9",
    "10",
];
const PRESENTATIONS: &[&str] = &["mailbox", "immediate", "fifo", "fifo_relaxed"];

fn settings_page(config: &Shared, toasts: &adw::ToastOverlay) -> gtk::Widget {
    let page = adw::PreferencesPage::new();
    let c = config.borrow().clone();

    let performance = adw::PreferencesGroup::builder()
        .title("Performance")
        .build();
    let cap = adw::SpinRow::with_range(0.0, 9999.0, 1.0);
    cap.set_title("Frame cap");
    cap.set_subtitle("9999 is uncapped; 0 keeps Roblox's own setting");
    cap.set_value(f64::from(c.frame_cap));
    {
        let config = config.clone();
        cap.connect_value_notify(move |row| {
            config.borrow_mut().frame_cap = row.value() as u32;
            save(&config);
        });
    }
    performance.add(&cap);
    let shared = config.clone();
    performance.add(&combo_row(
        "Graphics quality",
        "Set before every launch",
        QUALITIES,
        u32::from(c.quality),
        move |i| {
            shared.borrow_mut().quality = i as u8;
            save(&shared);
        },
    ));
    let shared = config.clone();
    let current = PRESENTATIONS
        .iter()
        .position(|p| *p == c.presentation)
        .unwrap_or(0) as u32;
    performance.add(&combo_row(
        "Presentation",
        "mailbox: no tearing; immediate: lowest latency",
        PRESENTATIONS,
        current,
        move |i| {
            shared.borrow_mut().presentation = PRESENTATIONS[i as usize].to_string();
            save(&shared);
        },
    ));
    let shared = config.clone();
    performance.add(&switch_row(
        "VSync",
        "Caps frames to the monitor",
        c.vsync,
        move |on| {
            shared.borrow_mut().vsync = on;
            save(&shared);
        },
    ));
    let shared = config.clone();
    performance.add(&switch_row(
        "GameMode",
        "Run through gamemoderun",
        c.gamemode,
        move |on| {
            shared.borrow_mut().gamemode = on;
            save(&shared);
        },
    ));
    let shared = config.clone();
    performance.add(&switch_row(
        "MangoHud",
        "Show the MangoHud overlay",
        c.mangohud,
        move |on| {
            shared.borrow_mut().mangohud = on;
            save(&shared);
        },
    ));
    page.add(&performance);

    page.add(&flags_group(config, toasts));

    let discord = adw::PreferencesGroup::builder()
        .title("Discord")
        .description("Shows the game you are in on your Discord profile")
        .build();
    let shared = config.clone();
    discord.add(&switch_row(
        "Show on Discord",
        "",
        c.discord.enabled,
        move |on| {
            shared.borrow_mut().discord.enabled = on;
            save(&shared);
        },
    ));
    let id = adw::EntryRow::builder()
        .title("Own application ID (optional)")
        .text(&c.discord.app_id)
        .show_apply_button(true)
        .build();
    {
        let config = config.clone();
        let toasts = toasts.clone();
        id.connect_apply(move |row| {
            config.borrow_mut().discord.app_id = row.text().trim().to_string();
            save(&config);
            toasts.add_toast(adw::Toast::new("Saved; it takes effect on the next launch"));
        });
    }
    discord.add(&id);
    let shared = config.clone();
    discord.add(&switch_row(
        "Show the game",
        "Name, creator and icon; off shows only Roblox",
        c.discord.show_game,
        move |on| {
            shared.borrow_mut().discord.show_game = on;
            save(&shared);
        },
    ));
    let shared = config.clone();
    discord.add(&switch_row(
        "Game page button",
        "",
        c.discord.buttons,
        move |on| {
            shared.borrow_mut().discord.buttons = on;
            save(&shared);
        },
    ));
    let shared = config.clone();
    discord.add(&switch_row(
        "Let games set their status",
        "BloxstrapRPC messages from the game",
        c.discord.game_status,
        move |on| {
            shared.borrow_mut().discord.game_status = on;
            save(&shared);
        },
    ));
    page.add(&discord);

    let machine = adw::PreferencesGroup::builder()
        .title("This machine")
        .build();
    let shared = config.clone();
    machine.add(&switch_row(
        "NVIDIA only",
        "Keep Vulkan on the NVIDIA driver when it is installed",
        c.nvidia_only,
        move |on| {
            shared.borrow_mut().nvidia_only = on;
            save(&shared);
        },
    ));
    let shared = config.clone();
    machine.add(&switch_row(
        "Pause video wallpaper",
        "Through mpvpaper's pause list",
        c.pause_wallpaper,
        move |on| {
            shared.borrow_mut().pause_wallpaper = on;
            save(&shared);
        },
    ));
    let shared = config.clone();
    machine.add(&switch_row(
        "Control for copy and paste",
        "Ctrl+A/C/V/X/Z act as Command, as the Mac client expects",
        c.control_edit_keys,
        move |on| {
            shared.borrow_mut().control_edit_keys = on;
            save(&shared);
        },
    ));
    let shared = config.clone();
    machine.add(&switch_row(
        "Update Roblox automatically",
        "Check before every launch",
        c.auto_update,
        move |on| {
            shared.borrow_mut().auto_update = on;
            save(&shared);
        },
    ));
    page.add(&machine);

    let rosemary = adw::PreferencesGroup::builder().title("Rosemary").build();
    let path = adw::EntryRow::builder()
        .title("Rosemary binary (blank: Ginrose's own, then PATH)")
        .text(&c.rosemary)
        .show_apply_button(true)
        .build();
    {
        let config = config.clone();
        let toasts = toasts.clone();
        path.connect_apply(move |row| {
            config.borrow_mut().rosemary = row.text().trim().to_string();
            save(&config);
            let found = config.borrow().rosemary_path();
            toasts.add_toast(adw::Toast::new(&match found {
                Some(p) => format!("Using {}", p.display()),
                None => "That file does not exist".to_string(),
            }));
        });
    }
    rosemary.add(&path);
    let links = adw::ActionRow::builder()
        .title("Open Roblox links with Ginrose")
        .subtitle(if desktop::is_default() {
            "Ginrose opens them now"
        } else {
            "Another program opens them now"
        })
        .build();
    let take = gtk::Button::with_label("Use Ginrose");
    take.set_valign(gtk::Align::Center);
    {
        let toasts = toasts.clone();
        let links = links.clone();
        take.connect_clicked(move |_| match desktop::install(true) {
            Ok(()) => {
                links.set_subtitle("Ginrose opens them now");
                toasts.add_toast(adw::Toast::new("Roblox links now open with Ginrose"));
            }
            Err(e) => toasts.add_toast(adw::Toast::new(&e)),
        });
    }
    links.add_suffix(&take);
    rosemary.add(&links);
    page.add(&rosemary);
    page.upcast()
}

/// One row per allowlisted flag: a switch that puts it in the file, and for a
/// number, the value it is set to.
fn flags_group(config: &Shared, toasts: &adw::ToastOverlay) -> adw::PreferencesGroup {
    let group = adw::PreferencesGroup::builder()
        .title("FastFlags")
        .description("Only Roblox's allowlist; the client ignores anything else")
        .build();
    let preset = gtk::Button::with_label("Fast preset");
    preset.set_valign(gtk::Align::Center);
    group.set_header_suffix(Some(&preset));
    let mut switches = Vec::new();
    for flag in flags::ALLOWLIST {
        let row = adw::ActionRow::builder()
            .title(flag.title)
            .subtitle(flag.help)
            .build();
        row.set_tooltip_text(Some(flag.name));
        let current = config.borrow().flags.get(flag.name).cloned();
        let toggle = gtk::Switch::builder()
            .valign(gtk::Align::Center)
            .active(current.is_some())
            .build();
        let spin = (flag.kind == flags::Kind::Int).then(|| {
            let spin = gtk::SpinButton::with_range(0.0, 100000.0, 1.0);
            spin.set_valign(gtk::Align::Center);
            spin.set_value(
                current
                    .as_ref()
                    .and_then(Value::as_i64)
                    .or(flag.fast)
                    .unwrap_or(0) as f64,
            );
            spin.set_sensitive(current.is_some());
            row.add_suffix(&spin);
            spin
        });
        row.add_suffix(&toggle);
        let value_of = {
            let spin = spin.clone();
            move || match &spin {
                Some(spin) => Value::from(spin.value() as i64),
                None => Value::from(true),
            }
        };
        let store = {
            let config = config.clone();
            let name = flag.name.to_string();
            let value_of = value_of.clone();
            move |on: bool| {
                let mut c = config.borrow_mut();
                match on {
                    true => c.flags.insert(name.clone(), value_of()),
                    false => c.flags.remove(&name),
                };
                drop(c);
                save(&config);
            }
        };
        {
            let store = store.clone();
            let spin = spin.clone();
            toggle.connect_active_notify(move |t| {
                if let Some(spin) = &spin {
                    spin.set_sensitive(t.is_active());
                }
                store(t.is_active());
            });
        }
        if let Some(spin) = &spin {
            let toggle = toggle.clone();
            spin.connect_value_changed(move |_| {
                if toggle.is_active() {
                    store(true);
                }
            });
        }
        switches.push((flag, toggle, spin));
        group.add(&row);
    }
    let toasts = toasts.clone();
    preset.connect_clicked(move |_| {
        for (flag, toggle, spin) in &switches {
            if let Some(fast) = flag.fast {
                if let Some(spin) = spin {
                    spin.set_value(fast as f64);
                }
                toggle.set_active(true);
            }
        }
        toasts.add_toast(adw::Toast::new("Anti-aliasing and grass turned off"));
    });
    group
}

fn profiles_page(config: &Shared, toasts: &adw::ToastOverlay) -> gtk::Widget {
    let page = adw::PreferencesPage::new();
    let add_group = adw::PreferencesGroup::builder()
        .title("Per-game profiles")
        .description("Applied when a link names the game. Joining through a friend has no game in the link, so it uses the defaults.")
        .build();
    let entry = adw::EntryRow::builder()
        .title("Game link or place ID")
        .show_apply_button(true)
        .build();
    add_group.add(&entry);
    page.add(&add_group);
    let list = adw::PreferencesGroup::new();
    page.add(&list);
    let rows: Rc<RefCell<Vec<gtk::Widget>>> = Rc::default();
    let refresh: Rc<dyn Fn()> = {
        let config = config.clone();
        let list = list.clone();
        let rows = rows.clone();
        Rc::new(move || {
            for row in rows.borrow_mut().drain(..) {
                list.remove(&row);
            }
            let profiles = config.borrow().profiles.clone();
            for profile in profiles {
                let row = profile_row(&config, &profile);
                list.add(&row);
                rows.borrow_mut().push(row.upcast());
            }
        })
    };
    refresh();
    let config = config.clone();
    let toasts = toasts.clone();
    entry.connect_apply(move |entry| {
        let text = entry.text().to_string();
        let Some(place_id) = text
            .split(|c: char| !c.is_ascii_digit())
            .find(|part| part.len() >= 3)
            .and_then(|part| part.parse::<u64>().ok())
        else {
            toasts.add_toast(adw::Toast::new("No place ID in that"));
            return;
        };
        if config.borrow().profile(place_id).is_some() {
            toasts.add_toast(adw::Toast::new("That game already has a profile"));
            return;
        }
        entry.set_text("");
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(game_name(place_id));
        });
        let (config, refresh) = (config.clone(), refresh.clone());
        glib::timeout_add_local(Duration::from_millis(100), move || match rx.try_recv() {
            Ok(name) => {
                config.borrow_mut().profiles.push(Profile {
                    place_id,
                    name: name.unwrap_or_else(|| format!("Place {place_id}")),
                    ..Profile::default()
                });
                save(&config);
                refresh();
                glib::ControlFlow::Break
            }
            Err(mpsc::TryRecvError::Empty) => glib::ControlFlow::Continue,
            Err(_) => glib::ControlFlow::Break,
        });
    });
    page.upcast()
}

fn game_name(place_id: u64) -> Option<String> {
    let agent = roblox::agent();
    let universe: Value = agent
        .get(format!(
            "https://apis.roblox.com/universes/v1/places/{place_id}/universe"
        ))
        .call()
        .ok()?
        .body_mut()
        .read_json()
        .ok()?;
    let universe = universe["universeId"].as_u64()?;
    let games: Value = agent
        .get(format!(
            "https://games.roblox.com/v1/games?universeIds={universe}"
        ))
        .call()
        .ok()?
        .body_mut()
        .read_json()
        .ok()?;
    games["data"][0]["name"].as_str().map(String::from)
}

const FLAG_PRESETS: &[&str] = &["Default flags", "Fast preset", "No flags"];

fn profile_row(config: &Shared, profile: &Profile) -> adw::ExpanderRow {
    let row = adw::ExpanderRow::builder()
        .title(glib::markup_escape_text(&profile.name).as_str())
        .subtitle(format!("Place {}", profile.place_id))
        .build();
    let id = profile.place_id;
    let edit = {
        let config = config.clone();
        move |change: &dyn Fn(&mut Profile)| {
            if let Some(p) = config
                .borrow_mut()
                .profiles
                .iter_mut()
                .find(|p| p.place_id == id)
            {
                change(p);
            }
            save(&config);
        }
    };
    let e = edit.clone();
    row.add_row(&combo_row(
        "Graphics quality",
        "",
        &["Default", "1", "2", "3", "4", "5", "6", "7", "8", "9", "10"],
        u32::from(profile.quality),
        move |i| e(&|p| p.quality = i as u8),
    ));
    let cap = adw::SpinRow::with_range(0.0, 9999.0, 1.0);
    cap.set_title("Frame cap");
    cap.set_subtitle("0 keeps the default");
    cap.set_value(f64::from(profile.frame_cap));
    let e = edit.clone();
    cap.connect_value_notify(move |row| {
        let value = row.value() as u32;
        e(&|p| p.frame_cap = value)
    });
    row.add_row(&cap);
    let current = match profile.flags.is_empty() {
        true => 0,
        false if profile.flags.values().all(Value::is_null) => 2,
        false => 1,
    };
    let e = edit.clone();
    row.add_row(&combo_row(
        "FastFlags",
        "Fast preset turns off anti-aliasing and grass for this game",
        FLAG_PRESETS,
        current,
        move |i| {
            e(&|p| {
                p.flags.clear();
                for flag in flags::ALLOWLIST {
                    match (i, flag.fast) {
                        (1, Some(fast)) => {
                            p.flags.insert(flag.name.into(), Value::from(fast));
                        }
                        (2, _) => {
                            p.flags.insert(flag.name.into(), Value::Null);
                        }
                        _ => {}
                    }
                }
            })
        },
    ));
    let remove = gtk::Button::from_icon_name("user-trash-symbolic");
    remove.set_valign(gtk::Align::Center);
    remove.add_css_class("flat");
    remove.set_tooltip_text(Some("Remove this profile"));
    {
        let config = config.clone();
        let row = row.clone();
        remove.connect_clicked(move |_| {
            config.borrow_mut().profiles.retain(|p| p.place_id != id);
            save(&config);
            row.set_visible(false);
        });
    }
    row.add_suffix(&remove);
    row
}
