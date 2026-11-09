//! Ginrose Studio's window: open Studio and its folders, update or repair it,
//! and settings that take whatever the user gives them.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::rc::Rc;

use adw::prelude::*;
use gtk::gio;
use serde_json::Value;

use crate::config::Config;
use crate::setup::in_background;
use crate::ui::{
    Message, PRESENTATIONS, Shared, butter_row, combo_row, receive, save, start, switch_row,
};
use crate::{desktop, flags, launch, machine, paths, roblox, rosemary, studio};

/// The name the main window goes by, so a second `ginrose-studio` brings it
/// forward instead of opening another.
const MAIN: &str = "studio-main";

pub fn main_window(app: &adw::Application) {
    if let Some(window) = app.windows().into_iter().find(|w| w.widget_name() == MAIN) {
        window.present();
        return;
    }
    let config: Shared = Rc::new(RefCell::new(Config::load()));
    let toasts = adw::ToastOverlay::new();
    let stack = adw::ViewStack::new();
    stack.add_titled_with_icon(
        &studio_page(&config, &toasts, app),
        Some("studio"),
        "Studio",
        "media-playback-start-symbolic",
    );
    stack.add_titled_with_icon(
        &settings_page(&config, &toasts),
        Some("settings"),
        "Settings",
        "emblem-system-symbolic",
    );
    stack.add_titled_with_icon(
        &advanced_page(&config, &toasts),
        Some("advanced"),
        "Advanced",
        "applications-engineering-symbolic",
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
        .title("Ginrose Studio")
        .default_width(760)
        .default_height(680)
        .content(&view)
        .build();
    window.set_widget_name(MAIN);
    window.present();
}

fn describe() -> String {
    let studio = roblox::installed().map_or("Roblox Studio not installed yet".to_string(), |b| {
        format!("Roblox Studio {}", b.version)
    });
    let rosemary = Config::load()
        .rosemary_path()
        .map_or("Rosemary not found".to_string(), |p| {
            format!("Rosemary: {}", p.display())
        });
    format!("{studio}\n{rosemary}")
}

/// Open a folder of the prefix in the file manager. The prefix is made on the
/// first launch, and making its folders before that would leave Rosemary a
/// half-made one, so until then there is nothing to open.
fn open_folder(widget: &impl IsA<gtk::Widget>, toasts: &adw::ToastOverlay, folder: PathBuf) {
    if !paths::prefix().join("root").is_dir() {
        toasts.add_toast(adw::Toast::new(
            "Open Roblox Studio once first; its folders are made on the first launch",
        ));
        return;
    }
    if let Err(e) = std::fs::create_dir_all(&folder) {
        toasts.add_toast(adw::Toast::new(&e.to_string()));
        return;
    }
    let window = widget.root().and_downcast::<gtk::Window>();
    let toasts = toasts.clone();
    gtk::FileLauncher::new(Some(&gio::File::for_path(&folder))).launch(
        window.as_ref(),
        None::<&gio::Cancellable>,
        move |result| {
            if result.is_err() {
                toasts.add_toast(adw::Toast::new(&format!("Folder: {}", folder.display())));
            }
        },
    );
}

fn studio_page(config: &Shared, toasts: &adw::ToastOverlay, app: &adw::Application) -> gtk::Widget {
    let status = adw::StatusPage::builder().title("Ginrose Studio").build();
    status.set_paintable(crate::setup::logo(128).paintable().as_ref());
    status.set_description(Some(&describe()));
    let open = gtk::Button::with_label("Open Roblox Studio");
    open.add_css_class("suggested-action");
    open.add_css_class("pill");
    open.set_halign(gtk::Align::Center);
    let bar = gtk::ProgressBar::new();
    bar.set_visible(false);
    let line = gtk::Label::new(None);
    line.add_css_class("dim-label");
    line.set_wrap(true);

    let folders = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    folders.set_halign(gtk::Align::Center);
    let plugins = gtk::Button::with_label("Plugins folder");
    let documents = gtk::Button::with_label("Documents");
    let update = gtk::Button::with_label("Update");
    let repair = gtk::Button::with_label("Repair…");
    for button in [&plugins, &documents, &update, &repair] {
        button.add_css_class("flat");
        folders.append(button);
    }
    plugins.set_tooltip_text(Some("Where Studio loads your local plugins from"));
    documents.set_tooltip_text(Some(
        "The prefix's Documents folder, with Studio's autosaves",
    ));
    repair.set_tooltip_text(Some(
        "Download Studio again and put the prefix's system files back",
    ));

    let banners = gtk::Box::new(gtk::Orientation::Vertical, 6);
    let column = gtk::Box::new(gtk::Orientation::Vertical, 12);
    column.set_halign(gtk::Align::Center);
    column.set_width_request(380);
    column.append(&open);
    column.append(&bar);
    column.append(&line);
    column.append(&folders);
    column.append(&banners);
    status.set_child(Some(&column));
    for warning in machine::check(&config.borrow(), None) {
        let banner = adw::Banner::new(&format!("{}: {}", warning.title, warning.detail));
        banner.set_revealed(true);
        banners.append(&banner);
    }

    {
        let toasts = toasts.clone();
        plugins.connect_clicked(move |b| open_folder(b, &toasts, paths::studio_plugins()));
    }
    {
        let toasts = toasts.clone();
        documents.connect_clicked(move |b| open_folder(b, &toasts, paths::guest_documents()));
    }
    {
        let (bar, line, status) = (bar.clone(), line.clone(), status.clone());
        update.connect_clicked(move |button| {
            button.set_sensitive(false);
            let (button, line_done, status) = (button.clone(), line.clone(), status.clone());
            in_background(
                &bar,
                &line,
                |progress| roblox::ensure(true, progress),
                move |result| {
                    button.set_sensitive(true);
                    line_done.set_text(&match result {
                        Ok(build) => format!("Roblox Studio {} is the current one", build.version),
                        Err(e) => e,
                    });
                    status.set_description(Some(&describe()));
                },
            );
        });
    }
    {
        let (bar, line, status) = (bar.clone(), line.clone(), status.clone());
        repair.connect_clicked(move |button| {
            let dialog = adw::AlertDialog::new(
                Some("Repair Roblox Studio?"),
                Some("This closes Roblox Studio, downloads it again and puts the prefix's system files back. Your places, plugins and settings are kept."),
            );
            dialog.add_responses(&[("cancel", "Cancel"), ("repair", "Repair")]);
            dialog.set_response_appearance("repair", adw::ResponseAppearance::Destructive);
            dialog.set_default_response(Some("cancel"));
            let parent = button.root();
            let (bar, line, status, button) =
                (bar.clone(), line.clone(), status.clone(), button.clone());
            dialog.connect_response(None, move |_, response| {
                if response != "repair" {
                    return;
                }
                button.set_sensitive(false);
                let (button, line_done, status) = (button.clone(), line.clone(), status.clone());
                in_background(&bar, &line, launch::repair, move |result| {
                    button.set_sensitive(true);
                    line_done.set_text(&match result {
                        Ok(build) => format!("Roblox Studio {} is installed again", build.version),
                        Err(e) => e,
                    });
                    status.set_description(Some(&describe()));
                });
            });
            dialog.present(parent.as_ref());
        });
    }

    let config = config.clone();
    let toasts = toasts.clone();
    let app = app.clone();
    let page = status.clone();
    open.connect_clicked(move |button| {
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
                // Studio may be opened again while it runs: that brings it
                // forward, as on a Mac.
                Message::Started(_) => {
                    bar.set_visible(false);
                    button.set_sensitive(true);
                    line.set_text("Roblox Studio is running");
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
                    if launch::running_builds().is_empty() {
                        line.set_text("");
                    }
                    hold.borrow_mut().take();
                }
            },
        );
    });
    status.upcast()
}

const STUDIO_PRESENTATIONS: &[&str] = &[
    "Rosemary's own",
    "mailbox",
    "immediate",
    "fifo",
    "fifo_relaxed",
];

fn settings_page(config: &Shared, toasts: &adw::ToastOverlay) -> gtk::Widget {
    let page = adw::PreferencesPage::new();
    let c = config.borrow().clone();
    let flatpak = paths::in_flatpak();

    let rosemary = adw::PreferencesGroup::builder()
        .title("Rosemary")
        .description("Any build you like, inside the Flatpak too, as long as it runs there")
        .build();
    let path = adw::EntryRow::builder()
        .title(match flatpak {
            true => "Rosemary binary (blank: the one the Flatpak comes with)",
            false => "Rosemary binary (blank: Ginrose Studio's own, then PATH)",
        })
        .text(&c.rosemary)
        .show_apply_button(true)
        .build();
    let apply_path = {
        let config = config.clone();
        let toasts = toasts.clone();
        move |text: String| {
            config.borrow_mut().rosemary = text;
            save(&config);
            let found = config.borrow().rosemary_path();
            toasts.add_toast(adw::Toast::new(&match found {
                Some(p) => format!("Using {}", p.display()),
                None => "That file does not exist".to_string(),
            }));
        }
    };
    {
        let apply_path = apply_path.clone();
        path.connect_apply(move |row| apply_path(row.text().trim().to_string()));
    }
    let choose = gtk::Button::from_icon_name("document-open-symbolic");
    choose.set_valign(gtk::Align::Center);
    choose.add_css_class("flat");
    choose.set_tooltip_text(Some("Choose a Rosemary binary"));
    {
        let (path, apply_path) = (path.clone(), apply_path.clone());
        choose.connect_clicked(move |button| {
            let dialog = gtk::FileDialog::builder().title("Choose Rosemary").build();
            let window = button.root().and_downcast::<gtk::Window>();
            let (path, apply_path) = (path.clone(), apply_path.clone());
            dialog.open(window.as_ref(), None::<&gio::Cancellable>, move |result| {
                if let Ok(file) = result
                    && let Some(chosen) = file.path()
                {
                    let text = chosen.to_string_lossy().into_owned();
                    path.set_text(&text);
                    apply_path(text);
                }
            });
        });
    }
    path.add_suffix(&choose);
    rosemary.add(&path);
    {
        let path = path.clone();
        if let Some(row) = butter_row(&c.rosemary, move |text| {
            path.set_text(&text);
            apply_path(text);
        }) {
            rosemary.add(&row);
        }
    }
    if !flatpak {
        let row = adw::ActionRow::builder()
            .title("Download Rosemary")
            .subtitle("Into Ginrose Studio's own folder, used when the field above is blank")
            .build();
        let bar = gtk::ProgressBar::new();
        bar.set_valign(gtk::Align::Center);
        bar.set_visible(false);
        let line = gtk::Label::new(None);
        line.set_visible(false);
        // The download's progress text shows as the row's subtitle.
        {
            let row = row.clone();
            line.connect_label_notify(move |line| row.set_subtitle(&line.text()));
        }
        let official = gtk::Button::with_label("Official");
        let fork = gtk::Button::with_label("Ginrose's");
        for (button, source) in [
            (official.clone(), rosemary::Source::Official),
            (fork.clone(), rosemary::Source::Ginrose),
        ] {
            button.set_valign(gtk::Align::Center);
            let (bar, line, row) = (bar.clone(), line.clone(), row.clone());
            let both = [official.clone(), fork.clone()];
            button.connect_clicked(move |_| {
                for b in &both {
                    b.set_sensitive(false);
                }
                let (both, row_done) = (both.clone(), row.clone());
                in_background(
                    &bar,
                    &line,
                    move |progress| rosemary::download(source, progress),
                    move |result| {
                        for b in &both {
                            b.set_sensitive(true);
                        }
                        row_done.set_subtitle(&match result {
                            Ok(path) => format!("Ready at {}", path.display()),
                            Err(e) => e,
                        });
                    },
                );
            });
        }
        row.add_suffix(&bar);
        row.add_suffix(&official);
        row.add_suffix(&fork);
        rosemary.add(&row);
    }
    let links = adw::ActionRow::builder()
        .title("Open Studio links with Ginrose Studio")
        .subtitle(if desktop::is_default() {
            "Ginrose Studio opens them now"
        } else {
            "Another program opens them now; Studio's browser login needs Ginrose Studio"
        })
        .build();
    if flatpak {
        links.set_subtitle(&format!(
            "Run on the host: {}",
            desktop::host_default_command()
        ));
        links.set_subtitle_selectable(true);
    } else {
        let take = gtk::Button::with_label("Use Ginrose Studio");
        take.set_valign(gtk::Align::Center);
        let toasts = toasts.clone();
        let links_row = links.clone();
        take.connect_clicked(move |_| match desktop::install(true) {
            Ok(()) => {
                links_row.set_subtitle("Ginrose Studio opens them now");
                toasts.add_toast(adw::Toast::new("Studio links now open with Ginrose Studio"));
            }
            Err(e) => toasts.add_toast(adw::Toast::new(&e)),
        });
        links.add_suffix(&take);
    }
    rosemary.add(&links);
    page.add(&rosemary);

    let studio = adw::PreferencesGroup::builder().title("Studio").build();
    let shared = config.clone();
    studio.add(&switch_row(
        "Update Roblox Studio automatically",
        "Check before opening it, unless one is already running",
        c.auto_update,
        move |on| {
            shared.borrow_mut().auto_update = on;
            save(&shared);
        },
    ));
    let shared = config.clone();
    studio.add(&switch_row(
        "Control as Command",
        "Ctrl does what Command does on a Mac, as in Studio on Windows",
        c.control_edit_keys,
        move |on| {
            shared.borrow_mut().control_edit_keys = on;
            save(&shared);
        },
    ));
    let shared = config.clone();
    studio.add(&switch_row(
        "VSync",
        "Off draws as fast as it can, which an editor rarely needs",
        c.vsync,
        move |on| {
            shared.borrow_mut().vsync = on;
            save(&shared);
        },
    ));
    let shared = config.clone();
    let current = PRESENTATIONS
        .iter()
        .position(|p| *p == c.presentation)
        .map_or(0, |i| i + 1) as u32;
    studio.add(&combo_row(
        "Presentation",
        "ROSEMARY_METAL_PRESENTATION_MODE",
        STUDIO_PRESENTATIONS,
        current,
        move |i| {
            shared.borrow_mut().presentation = match i {
                0 => String::new(),
                i => PRESENTATIONS[i as usize - 1].to_string(),
            };
            save(&shared);
        },
    ));
    let shared = config.clone();
    studio.add(&switch_row(
        "GameMode",
        "Run through GameMode, for playtesting",
        c.gamemode,
        move |on| {
            shared.borrow_mut().gamemode = on;
            save(&shared);
        },
    ));
    let shared = config.clone();
    studio.add(&switch_row(
        "MangoHud",
        "Show the MangoHud overlay",
        c.mangohud,
        move |on| {
            shared.borrow_mut().mangohud = on;
            save(&shared);
        },
    ));
    page.add(&studio);

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
    if !flatpak {
        let shared = config.clone();
        machine.add(&switch_row(
            "Pause video wallpaper",
            "Through mpvpaper's pause list, while Rosemary runs",
            c.pause_wallpaper,
            move |on| {
                shared.borrow_mut().pause_wallpaper = on;
                save(&shared);
            },
        ));
    }
    page.add(&machine);
    page.upcast()
}

/// A group of `name = value` rows, each with a button that removes it, kept
/// in step with the config by `refresh`.
struct KeyedList {
    group: adw::PreferencesGroup,
    rows: RefCell<Vec<adw::ActionRow>>,
    items: Box<dyn Fn() -> Vec<(String, String)>>,
    remove: Rc<dyn Fn(&str)>,
}

impl KeyedList {
    fn refresh(&self) {
        for row in self.rows.borrow_mut().drain(..) {
            self.group.remove(&row);
        }
        for (name, value) in (self.items)() {
            let row = adw::ActionRow::builder()
                .title(&name)
                .subtitle(&value)
                .use_markup(false)
                .subtitle_selectable(true)
                .build();
            let delete = gtk::Button::from_icon_name("user-trash-symbolic");
            delete.set_valign(gtk::Align::Center);
            delete.add_css_class("flat");
            delete.set_tooltip_text(Some("Remove"));
            let (remove, group, row_ref) = (self.remove.clone(), self.group.clone(), row.clone());
            delete.connect_clicked(move |_| {
                remove(&name);
                group.remove(&row_ref);
            });
            row.add_suffix(&delete);
            self.group.add(&row);
            self.rows.borrow_mut().push(row);
        }
    }
}

fn flag_text(value: &Value) -> String {
    match value {
        Value::String(text) => format!("\"{text}\""),
        other => other.to_string(),
    }
}

fn advanced_page(config: &Shared, toasts: &adw::ToastOverlay) -> gtk::Widget {
    let page = adw::PreferencesPage::new();

    // FastFlags.
    let flags_group = adw::PreferencesGroup::builder()
        .title("FastFlags")
        .description("Any flag, written as given into Studio's ClientAppSettings.json before each launch; Studio reads it as it starts")
        .build();
    let paste = gtk::Button::with_label("Paste JSON");
    paste.set_valign(gtk::Align::Center);
    paste.add_css_class("flat");
    paste.set_tooltip_text(Some("Add every flag of a JSON list on the clipboard"));
    flags_group.set_header_suffix(Some(&paste));
    let add_flag = adw::EntryRow::builder()
        .title("Add a flag: Name=Value")
        .show_apply_button(true)
        .build();
    flags_group.add(&add_flag);
    let flag_list = Rc::new(KeyedList {
        group: flags_group.clone(),
        rows: RefCell::default(),
        items: Box::new({
            let config = config.clone();
            move || {
                config
                    .borrow()
                    .flags
                    .iter()
                    .map(|(name, value)| (name.clone(), flag_text(value)))
                    .collect()
            }
        }),
        remove: Rc::new({
            let config = config.clone();
            move |name: &str| {
                config.borrow_mut().flags.remove(name);
                save(&config);
            }
        }),
    });
    flag_list.refresh();
    let add_flags = {
        let (config, flag_list) = (config.clone(), flag_list.clone());
        move |new: BTreeMap<String, Value>| {
            config.borrow_mut().flags.extend(new);
            save(&config);
            flag_list.refresh();
        }
    };
    {
        let (toasts, add_flags) = (toasts.clone(), add_flags.clone());
        add_flag.connect_apply(move |entry| match flags::parse_entry(&entry.text()) {
            Some((name, value)) => {
                entry.set_text("");
                add_flags(BTreeMap::from([(name, value)]));
            }
            None => toasts.add_toast(adw::Toast::new("Write it as Name=Value")),
        });
    }
    {
        let toasts = toasts.clone();
        paste.connect_clicked(move |button| {
            let (toasts, add_flags) = (toasts.clone(), add_flags.clone());
            button
                .clipboard()
                .read_text_async(None::<&gio::Cancellable>, move |result| {
                    let text = result
                        .ok()
                        .flatten()
                        .map(|t| t.to_string())
                        .unwrap_or_default();
                    match flags::parse_json(&text) {
                        Ok(new) => {
                            let count = new.len();
                            add_flags(new);
                            toasts.add_toast(adw::Toast::new(&format!("Added {count} flags")));
                        }
                        Err(e) => toasts.add_toast(adw::Toast::new(&format!("Clipboard: {e}"))),
                    }
                });
        });
    }
    page.add(&flags_group);

    // Environment.
    let env_group = adw::PreferencesGroup::builder()
        .title("Environment")
        .description("Set for Rosemary and Studio after Ginrose Studio's own variables, so these win. Nothing is filtered: ROSEMARY_*, DYLD_*, LD_* and Vulkan layers all go through")
        .build();
    let add_var = adw::EntryRow::builder()
        .title("Add a variable: NAME=value")
        .show_apply_button(true)
        .build();
    env_group.add(&add_var);
    let env_list = Rc::new(KeyedList {
        group: env_group.clone(),
        rows: RefCell::default(),
        items: Box::new({
            let config = config.clone();
            move || {
                config
                    .borrow()
                    .extra_env
                    .iter()
                    .map(|(name, value)| (name.clone(), value.clone()))
                    .collect()
            }
        }),
        remove: Rc::new({
            let config = config.clone();
            move |name: &str| {
                config.borrow_mut().extra_env.remove(name);
                save(&config);
            }
        }),
    });
    env_list.refresh();
    {
        let (config, toasts) = (config.clone(), toasts.clone());
        add_var.connect_apply(move |entry| {
            let text = entry.text();
            match text.split_once('=') {
                Some((name, value))
                    if !name.trim().is_empty() && !name.trim().contains(['=', '\0']) =>
                {
                    config
                        .borrow_mut()
                        .extra_env
                        .insert(name.trim().to_string(), value.to_string());
                    save(&config);
                    entry.set_text("");
                    env_list.refresh();
                }
                _ => toasts.add_toast(adw::Toast::new("Write it as NAME=value")),
            }
        });
    }
    page.add(&env_group);

    // Launch arguments.
    let args_group = adw::PreferencesGroup::builder()
        .title("Launch arguments")
        .description("Passed to Studio after the app, split like a shell would")
        .build();
    let args = adw::EntryRow::builder()
        .title("Arguments")
        .text(studio::join_args(&config.borrow().launch_args))
        .show_apply_button(true)
        .build();
    {
        let (config, toasts) = (config.clone(), toasts.clone());
        args.connect_apply(move |row| match studio::split_args(&row.text()) {
            Ok(list) => {
                config.borrow_mut().launch_args = list;
                save(&config);
                toasts.add_toast(adw::Toast::new("Saved; it takes effect on the next launch"));
            }
            Err(e) => toasts.add_toast(adw::Toast::new(&e)),
        });
    }
    args_group.add(&args);
    page.add(&args_group);

    // Where things are.
    let places = adw::PreferencesGroup::builder().title("Folders").build();
    for (title, path) in [
        ("Prefix", paths::prefix()),
        ("Plugins", paths::studio_plugins()),
        ("Logs", paths::logs_dir()),
        ("Settings", paths::config_file()),
    ] {
        let row = adw::ActionRow::builder()
            .title(title)
            .subtitle(path.to_string_lossy())
            .use_markup(false)
            .subtitle_selectable(true)
            .build();
        places.add(&row);
    }
    page.add(&places);
    page.upcast()
}
