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
    model: gio::ListStore,
}

impl SongInfo {
    fn new(sender: mpsc::Sender<StateUpdateKind>) -> Self {
        let container = gtk::Box::new(gtk::Orientation::Vertical, 16);
        let album_art = gtk::Image::new();
        let song_text = gtk::Label::new(None);
        song_text.set_justify(gtk::Justification::Center);
        song_text.set_line_wrap(true);
        container.add(&album_art);
        container.add(&song_text);

        let action_bar = gtk::Box::new(gtk::Orientation::Horizontal, 16);
        action_bar.set_halign(gtk::Align::Center);

        let control_previous_song = gtk::Button::from_icon_name(
            Some("media-skip-backward-symbolic"),
            gtk::IconSize::SmallToolbar,
        );
        action_bar.add(&control_previous_song);
        let sender1 = sender.clone();
        control_previous_song.connect_clicked(move |_| {
            let mut sender = sender1.clone();
            sender
                .try_send(StateUpdateKind::PlaybackStateChange(
                    PlaybackStateChange::SkipBackwards,
                ))
                .expect("Couldn't notify thread");
        });

        let control_start_song = gtk::Button::from_icon_name(
            Some("media-playback-start-symbolic"),
            gtk::IconSize::SmallToolbar,
        );
        action_bar.add(&control_start_song);
        let sender1 = sender.clone();
        control_start_song.connect_clicked(move |_| {
            let mut sender = sender1.clone();
            sender
                .try_send(StateUpdateKind::PlaybackStateChange(
                    PlaybackStateChange::StartPlayback,
                ))
                .expect("Couldn't notify thread");
        });

        let control_pause_song = gtk::Button::from_icon_name(
            Some("media-playback-pause-symbolic"),
            gtk::IconSize::SmallToolbar,
        );
        action_bar.add(&control_pause_song);
        let sender1 = sender.clone();
        control_pause_song.connect_clicked(move |_| {
            let mut sender = sender1.clone();
            sender
                .try_send(StateUpdateKind::PlaybackStateChange(
                    PlaybackStateChange::PausePlayback,
                ))
                .expect("Couldn't notify thread");
        });

        let control_stop_song = gtk::Button::from_icon_name(
            Some("media-playback-stop-symbolic"),
            gtk::IconSize::SmallToolbar,
        );
        action_bar.add(&control_stop_song);
        let sender1 = sender.clone();
        control_stop_song.connect_clicked(move |_| {
            let mut sender = sender1.clone();
            sender
                .try_send(StateUpdateKind::PlaybackStateChange(
                    PlaybackStateChange::StopPlayback,
                ))
                .expect("Couldn't notify thread");
        });

        let control_next_song = gtk::Button::from_icon_name(
            Some("media-skip-forward-symbolic"),
            gtk::IconSize::SmallToolbar,
        );
        action_bar.add(&control_next_song);
        let sender1 = sender.clone();
        control_next_song.connect_clicked(move |_| {
            let mut sender = sender1.clone();
            sender
                .try_send(StateUpdateKind::PlaybackStateChange(
                    PlaybackStateChange::SkipForwards,
                ))
                .expect("Couldn't notify thread");
        });

        let model = gio::ListStore::new(SongObject::static_type());
        let listbox = gtk::ListBox::new();
        let sender1 = sender.clone();
        listbox.bind_model(Some(&model), move |item| {
            let sender = sender1.clone();

            let box_ = gtk::ListBoxRow::new();
            let item = item
                .downcast_ref::<SongObject>()
                .expect("Row data is of wrong type");

            let grid = gtk::Grid::builder().column_homogeneous(true).build();

            let remove_individual_song = gtk::Button::from_icon_name(
                Some("list-remove-symbolic"),
                gtk::IconSize::SmallToolbar,
            );
            remove_individual_song.set_visible(true);
            let index = item.property::<u32>("index");
            remove_individual_song.connect_clicked(move |_| {
                let mut sender = sender.clone();
                sender
                    .try_send(StateUpdateKind::QueueDeleteRequest(index))
                    .expect("Couldn't notify thread");

                // We don't actually get notified by `mpd`, but we can pretend
                // that we did!
                sender
                    .try_send(StateUpdateKind::MpdEvent)
                    .expect("Couldn't notify thread");
            });
            grid.attach(&remove_individual_song, 0, 0, 1, 1);

            let title_label = gtk::Label::new(None);
            title_label.set_line_wrap(true);
            item.bind_property("title", &title_label, "label")
                .flags(glib::BindingFlags::DEFAULT | glib::BindingFlags::SYNC_CREATE)
                .build();
            grid.attach(&title_label, 1, 0, 1, 1);

            title_label.set_visible(true); // why?

            let album_label = gtk::Label::new(None);
            album_label.set_line_wrap(true);
            item.bind_property("album", &album_label, "label")
                .flags(glib::BindingFlags::DEFAULT | glib::BindingFlags::SYNC_CREATE)
                .build();
            grid.attach(&album_label, 2, 0, 1, 1);

            album_label.set_visible(true); // why?

            let artist_label = gtk::Label::new(None);
            artist_label.set_line_wrap(true);
            item.bind_property("artist", &artist_label, "label")
                .flags(glib::BindingFlags::DEFAULT | glib::BindingFlags::SYNC_CREATE)
                .build();
            grid.attach(&artist_label, 3, 0, 1, 1);

            artist_label.set_visible(true); // why?

            grid.set_visible(true); // why?

            box_.add(&grid);

            box_.upcast::<gtk::Widget>()
        });

        let scrolled_window =
            gtk::ScrolledWindow::new(gtk::Adjustment::NONE, gtk::Adjustment::NONE);
        scrolled_window.add(&listbox);
        scrolled_window.set_vexpand(true);

        container.add(&action_bar);
        container.add(&scrolled_window);

        container.show_all();

        SongInfo {
            container,
            album_art,
            song_text,
            model,
        }
    }

    fn update_album_art(&self, conn: &mut mpd::Client) -> anyhow::Result<()> {
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
        }
        Ok(())
    }

    fn update(&self, conn: &mut mpd::Client) -> anyhow::Result<()> {
        self.update_album_art(conn)?;

        if let Some(song) = conn.currentsong()? {
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

        self.model.remove_all();
        for (i, song) in conn.queue()?.iter().enumerate() {
            let object = SongObject::new(&song);
            object.set_index(i.try_into().unwrap());
            self.model.insert(i.try_into().unwrap(), &object)
        }

        Ok(())
    }
}

impl AsRef<gtk::Widget> for SongInfo {
    fn as_ref(&self) -> &gtk::Widget {
        self.container.upcast_ref()
    }
}

/// View for selecting songs to add to the queue.
struct QueryInfo {
    container: gtk::Box,
    model: gio::ListStore,
}

impl QueryInfo {
    fn new(sender: mpsc::Sender<StateUpdateKind>) -> Self {
        let container = gtk::Box::new(gtk::Orientation::Vertical, 2);

        let query_input = gtk::Entry::builder().visible(true).build();
        let sender1 = sender.clone();
        query_input.connect_key_press_event(move |widget, _| {
            let mut sender = sender1.clone(); // Interesting ownership puzzle :D
            sender
                .try_send(StateUpdateKind::QueryUpdateEvent(widget.text().into()))
                .expect("Couldn't notify thread");
            gtk::Inhibit(false)
        });

        let model = gio::ListStore::new(SongObject::static_type());
        let listbox = gtk::ListBox::new();
        let sender1 = sender.clone();
        listbox.bind_model(Some(&model), move |item| {
            let sender = sender1.clone();

            let box_ = gtk::ListBoxRow::new();
            let item = item
                .downcast_ref::<SongObject>()
                .expect("Row data is of wrong type");

            let grid = gtk::Grid::builder().column_homogeneous(true).build();

            let add_individual_song =
                gtk::Button::from_icon_name(Some("list-add-symbolic"), gtk::IconSize::SmallToolbar);
            add_individual_song.set_visible(true);
            let filename = item.property::<String>("filename");
            add_individual_song.connect_clicked(move |_| {
                let filename = filename.clone();
                let mut sender = sender.clone();
                sender
                    .try_send(StateUpdateKind::QueueAddRequest(filename))
                    .expect("Couldn't notify thread");
            });
            grid.attach(&add_individual_song, 0, 0, 1, 1);

            let title_label = gtk::Label::new(None);
            title_label.set_line_wrap(true);
            item.bind_property("title", &title_label, "label")
                .flags(glib::BindingFlags::DEFAULT | glib::BindingFlags::SYNC_CREATE)
                .build();
            grid.attach(&title_label, 1, 0, 1, 1);

            title_label.set_visible(true); // why?

            let album_label = gtk::Label::new(None);
            album_label.set_line_wrap(true);
            item.bind_property("album", &album_label, "label")
                .flags(glib::BindingFlags::DEFAULT | glib::BindingFlags::SYNC_CREATE)
                .build();
            grid.attach(&album_label, 2, 0, 1, 1);

            album_label.set_visible(true); // why?

            let artist_label = gtk::Label::new(None);
            artist_label.set_line_wrap(true);
            item.bind_property("artist", &artist_label, "label")
                .flags(glib::BindingFlags::DEFAULT | glib::BindingFlags::SYNC_CREATE)
                .build();
            grid.attach(&artist_label, 3, 0, 1, 1);

            artist_label.set_visible(true); // why?

            grid.set_visible(true); // why?

            box_.add(&grid);

            box_.upcast::<gtk::Widget>()
        });

        let scrolled_window =
            gtk::ScrolledWindow::new(gtk::Adjustment::NONE, gtk::Adjustment::NONE);
        scrolled_window.add(&listbox);
        scrolled_window.set_vexpand(true);

        container.add(&query_input);
        container.add(&scrolled_window);

        QueryInfo { container, model }
    }
}

impl AsRef<gtk::Widget> for QueryInfo {
    fn as_ref(&self) -> &gtk::Widget {
        self.container.upcast_ref()
    }
}

glib::wrapper! {
    pub struct SongObject(ObjectSubclass<imp::SongObject>);
}

use gtk::subclass::prelude::ObjectSubclassExt;
impl SongObject {
    pub fn new(song: &mpd::song::Song) -> Self {
        glib::Object::new(&[
            ("filename", &song.file.clone()),
            (
                "title",
                &song
                    .title
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

    pub fn set_index(&self, idx: u32) {
        let private = imp::SongObject::from_instance(self);
        private.index.set(idx);
    }
}

mod imp {
    use std::cell::{Cell, RefCell};

    use glib::{ParamSpec, ParamSpecString, Value};
    use gtk::glib;
    use gtk::prelude::*;
    use gtk::subclass::prelude::*;
    use once_cell::sync::Lazy;

    // Object holding the state
    #[derive(Default)]
    pub struct SongObject {
        filename: RefCell<String>,
        title: RefCell<String>,
        artist: RefCell<String>,
        album: RefCell<String>,
        pub(crate) index: Cell<u32>,
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
                    ParamSpecString::builder("filename").build(),
                    ParamSpecString::builder("title").build(),
                    ParamSpecString::builder("artist").build(),
                    ParamSpecString::builder("album").build(),
                    ParamSpecString::builder("index").build(),
                ]
            });
            PROPERTIES.as_ref()
        }

        fn set_property(&self, _obj: &Self::Type, _id: usize, value: &Value, pspec: &ParamSpec) {
            match pspec.name() {
                "filename" => {
                    let input = value
                        .get()
                        .expect("The value needs to be of type `String`.");
                    self.filename.replace(input);
                }
                "title" => {
                    let input = value
                        .get()
                        .expect("The value needs to be of type `String`.");
                    self.title.replace(input);
                }
                "artist" => {
                    let input = value
                        .get()
                        .expect("The value needs to be of type `String`.");
                    self.artist.replace(input);
                }
                "album" => {
                    let input = value
                        .get()
                        .expect("The value needs to be of type `String`.");
                    self.album.replace(input);
                }
                "index" => {
                    let input = value.get().expect("The value needs to be of type `u32`.");
                    self.index.replace(input);
                }
                _ => unimplemented!(),
            }
        }

        fn property(&self, _obj: &Self::Type, _id: usize, pspec: &ParamSpec) -> Value {
            match pspec.name() {
                "filename" => self.filename.borrow().to_value(),
                "title" => self.title.borrow().to_value(),
                "artist" => self.artist.borrow().to_value(),
                "album" => self.album.borrow().to_value(),
                "index" => self.index.get().to_value(),
                _ => unimplemented!(),
            }
        }
    }
}

/// Kind of event we can notify the UI future about
enum StateUpdateKind {
    MpdEvent,
    WindowResizeEvent,
    QueryUpdateEvent(String),
    QueueAddRequest(String),
    QueueDeleteRequest(u32),
    PlaybackStateChange(PlaybackStateChange),
}

#[derive(Debug)]
enum PlaybackStateChange {
    SkipBackwards,
    SkipForwards,
    StartPlayback,
    StopPlayback,
    PausePlayback,
}

fn main() {
    let application = gtk::Application::builder()
        .application_id("space.jakob.Tunes")
        .build();

    application.connect_activate(|app| {
        libhandy::init();

        let mut conn = Client::connect("127.0.0.1:6600").unwrap();
        let (sender, mut receiver) = mpsc::channel(1000);

        let mut sender1 = sender.clone();
        std::thread::spawn(move || loop {
            let mut conn = Client::connect("127.0.0.1:6600").unwrap();
            if let Ok(_subsystems) = conn.wait(&[mpd::idle::Subsystem::Player]) {
                sender1
                    .try_send(StateUpdateKind::MpdEvent)
                    .expect("Couldn't notify thread");
            } else {
                break;
            }
        });

        // conn.volume(100).unwrap();
        // conn.load("My Lounge Playlist", ..).unwrap();
        // conn.play().unwrap();

        let stack = gtk::Stack::new();
        stack.set_expand(true);

        let song_info = SongInfo::new(sender.clone());
        stack.add_named(song_info.as_ref(), "current_song");
        stack.set_child_title(song_info.as_ref(), Some("Now Playing"));
        stack.set_child_icon_name(song_info.as_ref(), Some("audio-speakers-symbolic"));

        let query_info = QueryInfo::new(sender.clone());
        stack.add_named(query_info.as_ref(), "query_songs");
        stack.set_child_title(query_info.as_ref(), Some("Search Database"));
        stack.set_child_icon_name(query_info.as_ref(), Some("system-search-symbolic"));

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

        window.connect_size_allocate(move |_, _| {
            let mut sender = sender.clone();
            sender
                .try_send(StateUpdateKind::WindowResizeEvent)
                .expect("Couldn't notify thread");
        });

        // Now that everything's been allocated a window, let's go ahead and
        // update the widgets.
        song_info
            .update(&mut conn)
            .expect("Couldn't update song info");

        let mut query = mpd::Query::new();
        query.and(mpd::Term::Any, "");
        let songs = conn.find(&mut query, (0, 65535));
        // println!("{:?}", songs);
        for song in songs.unwrap() {
            query_info.model.insert(0, &SongObject::new(&song));
        }

        let main_context = gtk::glib::MainContext::default();
        main_context.spawn_local(async move {
            let mut conn = Client::connect("127.0.0.1:6600").unwrap();
            while let Some(event_type) = receiver.next().await {
                match event_type {
                    StateUpdateKind::MpdEvent => {
                        if let Ok(title) = header_title(&mut conn) {
                            ui.header_bar.set_title(Some(&title));
                            song_info
                                .update(&mut conn)
                                .expect("Couldn't update song info");
                        }
                    }
                    StateUpdateKind::WindowResizeEvent => {
                        song_info
                            .update_album_art(&mut conn)
                            .expect("Couldn't update album art");
                    }
                    StateUpdateKind::QueryUpdateEvent(query_string) => {
                        // Let's not produce massive queries while the user is typing :)
                        if query_string.len() <= 2 {
                            continue;
                        }
                        query_info.model.remove_all();
                        let mut query = mpd::Query::new();
                        query.and(mpd::Term::Any, &query_string);
                        let songs = conn.search(&mut query, (0, 65535));
                        // println!("{:?}", songs);
                        for song in songs.unwrap() {
                            query_info.model.insert(0, &SongObject::new(&song));
                        }
                    }
                    StateUpdateKind::QueueDeleteRequest(index) => {
                        conn.delete(index).expect("Couldn't dequeue song");
                    }
                    StateUpdateKind::QueueAddRequest(filename) => {
                        conn.push_str(filename).expect("Couldn't queue song");
                    }
                    StateUpdateKind::PlaybackStateChange(action) => {
                        dispatch_playback_state_change(&mut conn, action)
                            .expect("Couldn't queue action");
                    }
                }
            }
        });
    });

    application.run();
}

fn dispatch_playback_state_change(
    conn: &mut mpd::Client,
    action: PlaybackStateChange,
) -> anyhow::Result<()> {
    use PlaybackStateChange::*;
    match action {
        SkipBackwards => conn.prev()?,
        SkipForwards => conn.next()?,
        StartPlayback => conn.play()?,
        StopPlayback => conn.stop()?,
        PausePlayback => conn.pause(true)?,
    }
    Ok(())
}
