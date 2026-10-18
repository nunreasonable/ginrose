//! The first-run setup: Rosemary, Roblox, Discord and the desktop, one page
//! each, then the launcher.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::mpsc;
use std::time::Duration;

use adw::prelude::*;
use gtk::{gdk, gio, glib};

use crate::config::Config;
use crate::{desktop, roblox, rosemary};

const LOGO: &[u8] = include_bytes!("../assets/icons/256.png");

pub fn logo(size: i32) -> gtk::Image {
    let texture = gdk::Texture::from_bytes(&glib::Bytes::from_static(LOGO)).ok();
    let image = gtk::Image::from_paintable(texture.as_ref());
    image.set_pixel_size(size);
    image
}

fn page(title: &str, text: &str) -> (gtk::Box, gtk::Box) {
    let outer = gtk::Box::new(gtk::Orientation::Vertical, 18);
    outer.set_valign(gtk::Align::Center);
    outer.set_margin_start(36);
    outer.set_margin_end(36);
    outer.set_margin_top(24);
    outer.set_margin_bottom(24);
    let heading = gtk::Label::new(Some(title));
    heading.add_css_class("title-1");
    let body = gtk::Label::new(Some(text));
    body.set_wrap(true);
    body.set_justify(gtk::Justification::Center);
    body.add_css_class("dim-label");
    outer.append(&heading);
    outer.append(&body);
    let inner = gtk::Box::new(gtk::Orientation::Vertical, 12);
    inner.set_halign(gtk::Align::Center);
    inner.set_width_request(380);
    outer.append(&inner);
    (outer, inner)
}

/// Run `work` on a thread, showing its progress in `bar` and `line`, and call
/// `done` with its result on the main loop.
fn in_background<T: Send + 'static>(
    bar: &gtk::ProgressBar,
    line: &gtk::Label,
    work: impl FnOnce(&dyn Fn(&str, Option<f64>)) -> Result<T, String> + Send + 'static,
    done: impl FnOnce(Result<T, String>) + 'static,
) {
    enum Step<T> {
        Progress(String, Option<f64>),
        Done(Result<T, String>),
    }
    let (tx, rx) = mpsc::channel::<Step<T>>();
    std::thread::spawn(move || {
        let progress_tx = tx.clone();
        let result = work(&move |text: &str, fraction: Option<f64>| {
            let _ = progress_tx.send(Step::Progress(text.to_string(), fraction));
        });
        let _ = tx.send(Step::Done(result));
    });
    bar.set_visible(true);
    let (bar, line) = (bar.clone(), line.clone());
    let done = RefCell::new(Some(done));
    glib::timeout_add_local(Duration::from_millis(100), move || {
        loop {
            match rx.try_recv() {
                Ok(Step::Progress(text, fraction)) => {
                    line.set_text(&text);
                    match fraction {
                        Some(f) => bar.set_fraction(f),
                        None => bar.pulse(),
                    }
                }
                Ok(Step::Done(result)) => {
                    bar.set_visible(false);
                    if let Some(done) = done.borrow_mut().take() {
                        done(result);
                    }
                    return glib::ControlFlow::Break;
                }
                Err(mpsc::TryRecvError::Empty) => return glib::ControlFlow::Continue,
                Err(_) => return glib::ControlFlow::Break,
            }
        }
    });
}

pub fn window(app: &adw::Application, finished: impl Fn() + 'static) {
    let config = Rc::new(RefCell::new(Config::load()));
    let stack = gtk::Stack::new();
    stack.set_transition_type(gtk::StackTransitionType::SlideLeftRight);

    // Welcome.
    let (welcome, inner) = page(
        "Welcome to Ginrose",
        "Ginrose runs the official macOS Roblox client on Linux through Rosemary, keeps it up to date and shows your games on Discord. The client itself is never modified.",
    );
    welcome.prepend(&logo(128));
    let _ = inner;
    stack.add_named(&welcome, Some("welcome"));

    // Rosemary.
    let (rosemary_page, inner) = page(
        "Rosemary",
        "Rosemary is what runs macOS programs on Linux. Use one you have, or download one. Ginrose's build adds voice chat and Ctrl for copy and paste, but needs SDL 3.4 and a recent distro; the official one runs anywhere.",
    );
    let found = gtk::Label::new(None);
    found.set_wrap(true);
    let describe_found = {
        let found = found.clone();
        let config = config.clone();
        move || match config.borrow().rosemary_path() {
            Some(path) => found.set_markup(&format!(
                "Using <b>{}</b>",
                glib::markup_escape_text(&path.to_string_lossy())
            )),
            None => found.set_text("No Rosemary found yet"),
        }
    };
    describe_found();
    let choose = gtk::Button::with_label("Choose a Rosemary binary…");
    let fetch = gtk::Button::with_label("Download the official Rosemary");
    fetch.add_css_class("suggested-action");
    let fetch_fork = gtk::Button::with_label("Download Ginrose's Rosemary");
    let bar = gtk::ProgressBar::new();
    bar.set_visible(false);
    let line = gtk::Label::new(None);
    line.add_css_class("dim-label");
    line.set_wrap(true);
    for widget in [
        found.upcast_ref::<gtk::Widget>(),
        choose.upcast_ref(),
        fetch.upcast_ref(),
        fetch_fork.upcast_ref(),
        bar.upcast_ref(),
        line.upcast_ref(),
    ] {
        inner.append(widget);
    }
    {
        let config = config.clone();
        let describe_found = describe_found.clone();
        choose.connect_clicked(move |button| {
            let dialog = gtk::FileDialog::builder().title("Choose Rosemary").build();
            let config = config.clone();
            let describe_found = describe_found.clone();
            let window = button.root().and_downcast::<gtk::Window>();
            dialog.open(window.as_ref(), None::<&gio::Cancellable>, move |result| {
                if let Ok(file) = result
                    && let Some(path) = file.path()
                {
                    config.borrow_mut().rosemary = path.to_string_lossy().into_owned();
                    let _ = config.borrow().save();
                    describe_found();
                }
            });
        });
    }
    for (button, source) in [
        (fetch.clone(), rosemary::Source::Official),
        (fetch_fork.clone(), rosemary::Source::Ginrose),
    ] {
        let config = config.clone();
        let describe_found = describe_found.clone();
        let (bar, line) = (bar.clone(), line.clone());
        let both = [fetch.clone(), fetch_fork.clone()];
        button.connect_clicked(move |_| {
            for b in &both {
                b.set_sensitive(false);
            }
            let config = config.clone();
            let describe_found = describe_found.clone();
            let (both, line_done) = (both.clone(), line.clone());
            in_background(
                &bar,
                &line,
                move |progress| rosemary::download(source, progress),
                move |result| {
                    for b in &both {
                        b.set_sensitive(true);
                    }
                    match result {
                        Ok(_) => {
                            config.borrow_mut().rosemary.clear();
                            let _ = config.borrow().save();
                            line_done.set_text("Rosemary is ready");
                        }
                        Err(e) => line_done.set_text(&e),
                    }
                    describe_found();
                },
            );
        });
    }
    stack.add_named(&rosemary_page, Some("rosemary"));

    // Roblox.
    let (roblox_page, inner) = page(
        "Roblox",
        "Ginrose downloads the macOS client straight from Roblox (about 150 MB). You can also skip this; it downloads on the first Play.",
    );
    let get = gtk::Button::with_label("Download Roblox now");
    get.add_css_class("suggested-action");
    let bar = gtk::ProgressBar::new();
    bar.set_visible(false);
    let line = gtk::Label::new(
        roblox::installed()
            .map(|b| format!("Roblox {} is installed", b.version))
            .as_deref(),
    );
    line.add_css_class("dim-label");
    line.set_wrap(true);
    inner.append(&get);
    inner.append(&bar);
    inner.append(&line);
    {
        let (bar, line) = (bar.clone(), line.clone());
        get.connect_clicked(move |button| {
            button.set_sensitive(false);
            let (button, line_done) = (button.clone(), line.clone());
            in_background(
                &bar,
                &line,
                |progress| roblox::ensure(true, progress),
                move |result| {
                    button.set_sensitive(true);
                    line_done.set_text(&match result {
                        Ok(build) => format!("Roblox {} is installed", build.version),
                        Err(e) => e,
                    });
                },
            );
        });
    }
    stack.add_named(&roblox_page, Some("roblox"));

    // Discord.
    let (discord_page, inner) = page(
        "Discord",
        "Show the game you are in on your Discord profile, with its icon and how long you have been playing. Discord has to be open for it to appear.",
    );
    let group = adw::PreferencesGroup::new();
    let enabled = adw::SwitchRow::builder()
        .title("Show on Discord")
        .active(config.borrow().discord.enabled)
        .build();
    group.add(&enabled);
    inner.append(&group);
    stack.add_named(&discord_page, Some("discord"));

    // Desktop.
    let (desktop_page, inner) = page(
        "Almost done",
        "Ginrose adds itself to your app menu. It can also open the Play buttons on roblox.com.",
    );
    let group = adw::PreferencesGroup::new();
    let links = adw::SwitchRow::builder()
        .title("Open Roblox links with Ginrose")
        .subtitle("Takes them over from whatever opens them now")
        .active(true)
        .build();
    if crate::paths::in_flatpak() {
        let hint = gtk::Label::new(Some(
            "If another app opens the Play buttons on roblox.com, run this once in a terminal to hand them to Ginrose:",
        ));
        hint.set_wrap(true);
        hint.set_xalign(0.0);
        let command = gtk::Label::new(Some(&desktop::host_default_command()));
        command.set_selectable(true);
        command.set_wrap(true);
        command.set_xalign(0.0);
        command.add_css_class("monospace");
        inner.append(&hint);
        inner.append(&command);
    } else {
        group.add(&links);
        inner.append(&group);
    }
    stack.add_named(&desktop_page, Some("desktop"));

    let pages: &[&str] = match crate::paths::in_flatpak() {
        true => &["welcome", "roblox", "discord", "desktop"],
        false => &["welcome", "rosemary", "roblox", "discord", "desktop"],
    };
    let index = Rc::new(Cell::new(0usize));
    let back = gtk::Button::with_label("Back");
    let next = gtk::Button::with_label("Next");
    next.add_css_class("suggested-action");
    let header = adw::HeaderBar::new();
    header.set_show_end_title_buttons(true);
    header.pack_start(&back);
    header.pack_end(&next);
    let view = adw::ToolbarView::new();
    view.add_top_bar(&header);
    view.set_content(Some(&stack));
    let window = adw::ApplicationWindow::builder()
        .application(app)
        .title("Set up Ginrose")
        .default_width(560)
        .default_height(560)
        .content(&view)
        .build();
    let show = {
        let (stack, back, next, index) = (stack.clone(), back.clone(), next.clone(), index.clone());
        move || {
            let i = index.get();
            stack.set_visible_child_name(pages[i]);
            back.set_sensitive(i > 0);
            next.set_label(if i + 1 == pages.len() {
                "Finish"
            } else {
                "Next"
            });
        }
    };
    show();
    {
        let (index, show) = (index.clone(), show.clone());
        back.connect_clicked(move |_| {
            index.set(index.get().saturating_sub(1));
            show();
        });
    }
    let finished = Rc::new(finished);
    {
        let window = window.clone();
        next.connect_clicked(move |_| {
            if index.get() + 1 < pages.len() {
                index.set(index.get() + 1);
                show();
                return;
            }
            {
                let mut c = config.borrow_mut();
                c.discord.enabled = enabled.is_active();
                c.setup_done = true;
            }
            let _ = config.borrow().save();
            if let Err(e) = desktop::install(links.is_active()) {
                eprintln!("ginrose: {e}");
            }
            window.close();
            finished();
        });
    }
    window.present();
}
