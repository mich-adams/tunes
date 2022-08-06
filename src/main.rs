use futures::{channel::mpsc, StreamExt};
use gtk::prelude::*;
use gtk::{gdk_pixbuf, gio, glib};
use libhandy::prelude::*;
use libhandy::{ApplicationWindow, HeaderBar};
use mpd::idle::Idle;
use mpd::Client;

struct TunesUI {
    header_bar: HeaderBar,
    // album_art: Image,
    // queue_switcher: Notebook,
}

/// Produce a short status line for the current state of `conn`.
fn header_title(conn: &mut mpd::client::Client) -> anyhow::Result<String> {
    let status = conn.status();
    let state_descriptor = match status?.state {
        mpd::status::State::Stop => "[STOPPED]",
        mpd::status::State::Pause => "[PAUSED]",
        mpd::status::State::Play => "[PLAYING]",
    };
    if let Some(song) = conn.currentsong()? {
        Ok(format!(
            "{} {} - {}",
            state_descriptor,
            song.title.unwrap_or_else(|| "Untitled".into()),
            song.artist.unwrap_or_else(|| "Untitled".into()),
        ))
    } else {
        Ok("Tunes: No Song".into())
    }
}

/// View for information about the currently playing song.
struct SongInfo {
    container: gtk::Box,
    album_art: gtk::Image,
    song_text: gtk::Label,
}

impl SongInfo {
    fn new() -> Self {
        let container = gtk::Box::new(gtk::Orientation::Vertical, 16);
        let album_art = gtk::Image::new();
        let song_text = gtk::Label::new(None);
        song_text.set_justify(gtk::Justification::Center);
        song_text.set_line_wrap(true);
        container.add(&album_art);
        container.add(&song_text);
        SongInfo {
            container,
            album_art,
            song_text,
        }
    }

    fn update(&self, conn: &mut mpd::Client) -> anyhow::Result<()> {
        if let Some(song) = conn.currentsong()? {
            // If we've been allocated a window, pick the greatest dimension
            // (width or height) and divide that dimension by two to get the
            // size (in pixels) that we'll scale the album art to. Otherwise, we
            // default to 128.
            let album_art_size = std::cmp::max(
                self.container
                    .window()
                    .map(|x| x.width() / 2)
                    .unwrap_or(128),
                self.container
                    .window()
                    .map(|x| x.height() / 2)
                    .unwrap_or(128),
            );

            let image_data = conn.albumart(&song)?;
            let image_pixbuf = gdk_pixbuf::Pixbuf::from_stream(
                &gio::MemoryInputStream::from_bytes(&glib::Bytes::from(&image_data)),
                gio::Cancellable::NONE,
            )
            .ok()
            .and_then(|x| {
                x.scale_simple(
                    album_art_size,
                    album_art_size,
                    gtk::gdk_pixbuf::InterpType::Hyper,
                )
            });
            self.album_art.set_pixbuf(image_pixbuf.as_ref());

            let title = song
                .title
                .as_ref()
                .map(|x| x.as_str())
                .unwrap_or("[Unknown]");
            let artist = song
                .artist
                .as_ref()
                .map(|x| x.as_str())
                .unwrap_or("[Unknown]");
            let album = song
                .tags
                .get("Album")
                .map(|x| x.as_str())
                .unwrap_or("[Unknown]");
            let text = format!("{}\n{} - {}", title, artist, album);
            self.song_text.set_text(&text);

            let attr_list = gtk::pango::AttrList::new();

            // Scale the title of the song the most.
            let mut attr = gtk::pango::AttrFloat::new_scale(2.0);
            attr.set_start_index(0);
            attr.set_end_index(title.len() as u32);
            attr_list.insert(attr);

            // And still make the other info reasonably large.
            let mut attr = gtk::pango::AttrFloat::new_scale(1.5);
            attr.set_start_index(title.len() as u32 + 1);
            attr_list.insert(attr);

            self.song_text.set_attributes(Some(&attr_list));
        }

        Ok(())
    }
}

impl AsRef<gtk::Widget> for SongInfo {
    fn as_ref(&self) -> &gtk::Widget {
        self.container.upcast_ref()
    }
}

glib::wrapper! {
    pub struct SongObject(ObjectSubclass<imp::SongObject>);
}

impl SongObject {
    pub fn new(song: &mpd::song::Song) -> Self {
        glib::Object::new(&[
            (
                "title",
                &song
                    .name
                    .as_ref()
                    .map(|x| x.clone())
                    .unwrap_or_else(|| "[Untitled]".into()),
            ),
            (
                "artist",
                &song
                    .artist
                    .as_ref()
                    .map(|x| x.clone())
                    .unwrap_or_else(|| "[No Artist]".into()),
            ),
            (
                "album",
                &song
                    .tags
                    .get("Album")
                    .map(|x| x.clone())
                    .unwrap_or_else(|| "[Untitled]".into()),
            ),
        ])
        .expect("Failed to create `SongObject`.")
    }
}

mod imp {
    use std::cell::RefCell;

    use glib::{ParamSpec, ParamSpecString, Value};
    use gtk::glib;
    use gtk::prelude::*;
    use gtk::subclass::prelude::*;
    use once_cell::sync::Lazy;

    // Object holding the state
    #[derive(Default)]
    pub struct SongObject {
        title: RefCell<String>,
        artist: RefCell<String>,
        album: RefCell<String>,
    }

    // The central trait for subclassing a GObject
    #[glib::object_subclass]
    impl ObjectSubclass for SongObject {
        const NAME: &'static str = "TunesSongObject";
        type Type = super::SongObject;
    }

    // Trait shared by all GObjects
    impl ObjectImpl for SongObject {
        fn properties() -> &'static [ParamSpec] {
            static PROPERTIES: Lazy<Vec<ParamSpec>> = Lazy::new(|| {
                vec![
                    ParamSpecString::builder("title").build(),
                    ParamSpecString::builder("artist").build(),
                    ParamSpecString::builder("album").build(),
                ]
            });
            PROPERTIES.as_ref()
        }

        fn set_property(&self, _obj: &Self::Type, _id: usize, value: &Value, pspec: &ParamSpec) {
            match pspec.name() {
                "title" => {
                    let input_number = value
                        .get()
                        .expect("The value needs to be of type `String`.");
                    self.title.replace(input_number);
                }
                "artist" => {
                    let input_number = value
                        .get()
                        .expect("The value needs to be of type `String`.");
                    self.title.replace(input_number);
                }
                "album" => {
                    let input_number = value
                        .get()
                        .expect("The value needs to be of type `String`.");
                    self.title.replace(input_number);
                }
                _ => unimplemented!(),
            }
        }

        fn property(&self, _obj: &Self::Type, _id: usize, pspec: &ParamSpec) -> Value {
            match pspec.name() {
                "title" => self.title.borrow().to_value(),
                "artist" => self.artist.borrow().to_value(),
                "album" => self.album.borrow().to_value(),
                _ => unimplemented!(),
            }
        }
    }
}

fn main() {
    let application = gtk::Application::builder()
        .application_id("space.jakob.Tunes")
        .build();

    application.connect_activate(|app| {
        libhandy::init();

        let mut conn = Client::connect("127.0.0.1:6600").unwrap();

        // conn.volume(100).unwrap();
        // conn.load("My Lounge Playlist", ..).unwrap();
        // conn.play().unwrap();

        let stack = gtk::Stack::new();
        stack.set_expand(true);
        let song_info = SongInfo::new();
        stack.add_named(song_info.as_ref(), "current_song");
        stack.set_child_title(song_info.as_ref(), Some("Now Playing"));
        stack.set_child_icon_name(song_info.as_ref(), Some("audio-speakers-symbolic"));

        let header_bar = HeaderBar::builder()
            .show_close_button(true)
            .title(&header_title(&mut conn).unwrap())
            .build();
        let view_switcher_title = libhandy::ViewSwitcherTitle::builder()
            .title("Tunes")
            .stack(&stack)
            .build();
        header_bar.add(&view_switcher_title);

        let view_switcher_bar = libhandy::ViewSwitcherBar::builder()
            .visible(true)
            .can_focus(false)
            .stack(&stack)
            .reveal(true)
            .build();

        let ui = TunesUI { header_bar };

        // Combine the content in a box
        let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
        content.set_vexpand(true);
        // Handy's ApplicationWindow does not include a HeaderBar
        content.add(&ui.header_bar);
        content.add(&stack);
        content.add(&view_switcher_bar);

        let window = ApplicationWindow::builder()
            .default_width(350)
            .default_height(70)
            .modal(true)
            // add content to window
            .child(&content)
            .build();
        window.set_application(Some(app));
        window.show_all();

        // Now that everything's been allocated a window, let's go ahead and
        // update the widgets.
        song_info
            .update(&mut conn)
            .expect("Couldn't update song info");

        let (mut sender, mut receiver) = mpsc::channel(1000);
        std::thread::spawn(move || loop {
            let mut conn = Client::connect("127.0.0.1:6600").unwrap();
            if let Ok(_subsystems) = conn.wait(&[mpd::idle::Subsystem::Player]) {
                sender.try_send(true).expect("Couldn't notify thread");
            } else {
                sender.try_send(false).expect("Couldn't notify thread");
                break;
            }
        });

        let main_context = gtk::glib::MainContext::default();
        main_context.spawn_local(async move {
            let mut conn = Client::connect("127.0.0.1:6600").unwrap();
            while let Some(_item) = receiver.next().await {
                if let Ok(title) = header_title(&mut conn) {
                    ui.header_bar.set_title(Some(&title));
                    song_info
                        .update(&mut conn)
                        .expect("Couldn't update song info");
                }
            }
        });
    });

    application.run();
}
