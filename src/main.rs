// Copyright © 2021-2022 Jakob L. Kreuze <zerodaysfordays@sdf.org>
//
// This file is part of Tunes.
//
// Tunes is free software; you can redistribute it and/or modify it
// under the terms of the GNU Affero General Public License as
// published by the Free Software Foundation; either version 3 of the
// License, or (at your option) any later version.
//
// Tunes is distributed in the hope that it will be useful, but WITHOUT
// ANY WARRANTY; without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General
// Public License for more details.
//
// You should have received a copy of the GNU Affero General Public
// License along with Tunes. If not, see <http://www.gnu.org/licenses/>.

use futures::{channel::mpsc, StreamExt};
use glib::clone;
use gtk::prelude::*;
use gtk::subclass::prelude::ObjectSubclassExt;
use gtk::{gdk_pixbuf, gio, glib, pango};
use libhandy::prelude::*;
use libhandy::{ApplicationWindow, HeaderBar};
use mpd::idle::Idle;
use mpd::Client;

const MPD_HOST: &str = "127.0.0.1:6600";

fn main() {
    let application = gtk::Application::builder()
        .application_id("space.jakob.Tunes")
        .build();

    // We have to wait until the `activate` signal is fired before we can do our
    // setup.
    application.connect_activate(|app| {
        // Our event-handling code will look a bit like what's common in SDL
        // with their `SDLPollEvent` interface, in the sense that we'll have all
        // of the different sub-systems of this application notify the main
        // event loop by way of a channel.
        let (sender, mut receiver) = mpsc::channel(1024);

        // Load all of the mobile UI support code from `libhandy`.
        libhandy::init();

        // `mpd` will notify us of events. Let's spin up a thread to listen for
        // those notifications, and shuttle them through a channel as they
        // arrive.
        std::thread::spawn(clone!(@strong sender => move || {
            let mut conn = Client::connect(MPD_HOST).unwrap();
            while let Ok(_subsystems) = conn.wait(&[mpd::idle::Subsystem::Player]) {
                let mut sender = sender.clone();
                sender
                    .try_send(StateUpdateKind::MpdEvent)
                    .expect("Couldn't notify thread");
            }
        }));

        // We'll connect to the MPD daemon here so we can populate the UI with
        // some information from the current state.
        let mut conn = Client::connect(MPD_HOST).unwrap();

        // We'll have two "views" in our application: one for viewing and
        // manipulating the current `mpd` queue, and another for searching for
        // songs to add to the queue. In GTK, we can handle switching between
        // these different views using a Stack.
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

        // The `HeaderBar` is a GTK concept that libhandy plays nicely with. On
        // desktop, the elements for switching stack views will show up there.
        // On mobile, it will show up in a `ViewSwitcherBar` at the bottom.
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

        // The window needs a single child, so we'll join the header bar, the
        // stack, and the view switcher into a single box.
        let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
        content.set_vexpand(true);
        content.add(&header_bar);
        content.add(&stack);
        content.add(&view_switcher_bar);

        // Finally, the window. It's tied to a child, which we made above, and
        // the GtkApplication that we declared at the beginning of `main`.
        let window = ApplicationWindow::builder()
            .default_width(350)
            .default_height(70)
            .modal(true)
            .child(&content)
            .build();
        window.set_application(Some(app));
        window.show_all();

        // This isn't perfect (it won't run when the window gets its initial
        // size), but this is how we notify that the album art display should be
        // resized.
        window.connect_configure_event(clone!(@strong sender => move |_, _| {
            let mut sender = sender.clone();
            sender
                .try_send(StateUpdateKind::WindowResizeEvent)
                .expect("Couldn't notify thread");
            false
        }));

        // Now that everything's been allocated a window, let's go ahead and
        // update the widgets.
        song_info
            .update(&mut conn)
            .expect("Couldn't update song info");

        // The following code will fill the search view with every song in the
        // database. If you have a music library as big as mine, it will
        // negatively impact startup time. This could be done in, for example, a
        // worker thread, but I've just omitted it because I don't want this
        // example to be more complex than it has to be.
        //
        // let mut query = mpd::Query::new();
        // query.and(mpd::Term::Any, "");
        // let songs = conn.search(&query, (0, 65535));
        // for song in songs.unwrap() {
        //     query_info.model.insert(0, &SongObject::new(&song));
        // }

        // Finally, we'll start the "main event loop" we've been talking about
        // in the main context of the application.
        let main_context = gtk::glib::MainContext::default();
        main_context.spawn_local(async move {
            let mut conn = Client::connect(MPD_HOST).unwrap();
            while let Some(event_type) = receiver.next().await {
                match event_type {
                    StateUpdateKind::MpdEvent => {
                        if let Ok(title) = header_title(&mut conn) {
                            header_bar.set_title(Some(&title));
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

                        // Start from a blank slate.
                        query_info.model.remove_all();

                        // Query on all fields, case-insensitively, for the text
                        // that the user input.
                        let mut query = mpd::Query::new();
                        query.and(mpd::Term::Any, &query_string);
                        let songs = conn.search(&query, (0, 65535));

                        // Insert them all into the model. This is reversed,
                        // which I don't consider to be a big deal. It's far
                        // less complex than adding it in order, which you will
                        // see below in the code that handles the queue.
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

/// Take action on `conn` based on a `PlaybackStateChange` notification
fn dispatch_playback_state_change(
    conn: &mut mpd::Client,
    action: PlaybackStateChange,
) -> anyhow::Result<()> {
    use PlaybackStateChange::*;
    match action {
        SkipBackwards => conn.prev()?,
        SkipForwards => conn.next()?,
        Start => conn.play()?,
        Stop => conn.stop()?,
        Pause => conn.pause(true)?,
    }
    Ok(())
}

/// Kind of event we can notify the UI future about
#[derive(Debug)]
enum StateUpdateKind {
    MpdEvent,
    WindowResizeEvent,
    QueryUpdateEvent(String),
    QueueAddRequest(String),
    QueueDeleteRequest(u32),
    PlaybackStateChange(PlaybackStateChange),
}

/// A simple action that affects playback state.
#[derive(Debug)]
enum PlaybackStateChange {
    Start,
    Stop,
    Pause,
    SkipBackwards,
    SkipForwards,
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
        song_text.set_line_wrap_mode(pango::WrapMode::WordChar);
        container.add(&album_art);
        container.add(&song_text);

        let action_bar = gtk::Box::new(gtk::Orientation::Horizontal, 16);
        action_bar.set_halign(gtk::Align::Center);

        let control_previous_song = gtk::Button::from_icon_name(
            Some("media-skip-backward-symbolic"),
            gtk::IconSize::SmallToolbar,
        );
        action_bar.add(&control_previous_song);
        control_previous_song.connect_clicked(clone!(@strong sender => move |_| {
            let mut sender = sender.clone();
            sender
                .try_send(StateUpdateKind::PlaybackStateChange(
                    PlaybackStateChange::SkipBackwards,
                ))
                .expect("Couldn't notify thread");
        }));

        let control_start_song = gtk::Button::from_icon_name(
            Some("media-playback-start-symbolic"),
            gtk::IconSize::SmallToolbar,
        );
        action_bar.add(&control_start_song);
        control_start_song.connect_clicked(clone!(@strong sender => move |_| {
            let mut sender = sender.clone();
            sender
                .try_send(StateUpdateKind::PlaybackStateChange(
                    PlaybackStateChange::Start,
                ))
                .expect("Couldn't notify thread");
        }));

        let control_pause_song = gtk::Button::from_icon_name(
            Some("media-playback-pause-symbolic"),
            gtk::IconSize::SmallToolbar,
        );
        action_bar.add(&control_pause_song);
        control_pause_song.connect_clicked(clone!(@strong sender => move |_| {
            let mut sender = sender.clone();
            sender
                .try_send(StateUpdateKind::PlaybackStateChange(
                    PlaybackStateChange::Pause,
                ))
                .expect("Couldn't notify thread");
        }));

        let control_stop_song = gtk::Button::from_icon_name(
            Some("media-playback-stop-symbolic"),
            gtk::IconSize::SmallToolbar,
        );
        action_bar.add(&control_stop_song);
        control_stop_song.connect_clicked(clone!(@strong sender => move |_| {
            let mut sender = sender.clone();
            sender
                .try_send(StateUpdateKind::PlaybackStateChange(
                    PlaybackStateChange::Stop,
                ))
                .expect("Couldn't notify thread");
        }));

        let control_next_song = gtk::Button::from_icon_name(
            Some("media-skip-forward-symbolic"),
            gtk::IconSize::SmallToolbar,
        );
        action_bar.add(&control_next_song);
        control_next_song.connect_clicked(clone!(@strong sender => move |_| {
            let mut sender = sender.clone();
            sender
                .try_send(StateUpdateKind::PlaybackStateChange(
                    PlaybackStateChange::SkipForwards,
                ))
                .expect("Couldn't notify thread");
        }));

        let model = gio::ListStore::new(SongObject::static_type());
        let listbox = gtk::ListBox::new();
        listbox.bind_model(
            Some(&model),
            clone!(@strong sender => move |item| {
                let sender = sender.clone();

                let box_ = gtk::ListBoxRow::new();
                let item = item
                    .downcast_ref::<SongObject>()
                    .expect("Row data is of wrong type");

                let grid = gtk::Grid::builder().column_homogeneous(true).build();

                let remove_individual_song = gtk::Button::from_icon_name(
                    Some("list-remove-symbolic"),
                    gtk::IconSize::SmallToolbar,
                );
                let index = item.property::<u32>("index");
                remove_individual_song.connect_clicked(move |_| {
                    let mut sender = sender.clone();
                    sender
                        .try_send(StateUpdateKind::QueueDeleteRequest(index))
                        .expect("Couldn't notify thread");
                    sender
                        .try_send(StateUpdateKind::MpdEvent)
                        .expect("Couldn't notify thread");
                });
                grid.attach(&remove_individual_song, 0, 0, 1, 1);

                let title_label = gtk::Label::new(None);
                title_label.set_line_wrap(true);
                title_label.set_line_wrap_mode(pango::WrapMode::WordChar);
                item.bind_property("title", &title_label, "label")
                    .flags(glib::BindingFlags::DEFAULT | glib::BindingFlags::SYNC_CREATE)
                    .build();
                grid.attach(&title_label, 1, 0, 1, 1);


                let album_label = gtk::Label::new(None);
                album_label.set_line_wrap(true);
                album_label.set_line_wrap_mode(pango::WrapMode::WordChar);
                item.bind_property("album", &album_label, "label")
                    .flags(glib::BindingFlags::DEFAULT | glib::BindingFlags::SYNC_CREATE)
                    .build();
                grid.attach(&album_label, 2, 0, 1, 1);


                let artist_label = gtk::Label::new(None);
                artist_label.set_line_wrap(true);
                artist_label.set_line_wrap_mode(pango::WrapMode::WordChar);
                item.bind_property("artist", &artist_label, "label")
                    .flags(glib::BindingFlags::DEFAULT | glib::BindingFlags::SYNC_CREATE)
                    .build();
                grid.attach(&artist_label, 3, 0, 1, 1);

                grid.show_all();
                box_.add(&grid);
                box_.upcast::<gtk::Widget>()
            }),
        );

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
            // If we've been allocated a window, pick the least dimension (width
            // or height) and divide that dimension by two to get the size (in
            // pixels) that we'll scale the album art to. Otherwise, we default
            // to 128.
            let album_art_size = std::cmp::min(
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
            let title = song.title.as_deref().unwrap_or("[Unknown]");
            let artist = song.artist.as_deref().unwrap_or("[Unknown]");
            let album = song
                .tags
                .get("Album")
                .map(|x| x.as_str())
                .unwrap_or("[Unknown]");
            let text = format!("{}\n{} - {}", title, artist, album);
            self.song_text.set_text(&text);

            // We'll use `pango` attributes to make the display look nice and
            // pretty. Scale the title of the song the most, and still make the
            // other info reasonably large.
            let attr_list = gtk::pango::AttrList::new();
            let mut attr = gtk::pango::AttrFloat::new_scale(2.0);
            attr.set_start_index(0);
            attr.set_end_index(title.len() as u32);
            attr_list.insert(attr);
            let mut attr = gtk::pango::AttrFloat::new_scale(1.5);
            attr.set_start_index(title.len() as u32 + 1);
            attr_list.insert(attr);

            self.song_text.set_attributes(Some(&attr_list));
        }

        self.model.remove_all();
        for (i, song) in conn.queue()?.iter().enumerate() {
            let index = i.try_into().unwrap();
            let object = SongObject::new(song);
            object.set_index(index);
            self.model.insert(index, &object)
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
        query_input.connect_key_press_event(clone!(@strong sender => move |widget, _| {
            let mut sender = sender.clone();
            sender
                .try_send(StateUpdateKind::QueryUpdateEvent(widget.text().into()))
                .expect("Couldn't notify thread");
            gtk::Inhibit(false)
        }));

        let model = gio::ListStore::new(SongObject::static_type());
        let listbox = gtk::ListBox::new();
        listbox.bind_model(Some(&model), clone!(@strong sender => move |item| {
            let sender = sender.clone();

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
                sender
                    .try_send(StateUpdateKind::MpdEvent)
                    .expect("Couldn't notify thread");
            });
            grid.attach(&add_individual_song, 0, 0, 1, 1);

            let title_label = gtk::Label::new(None);
            title_label.set_line_wrap(true);
            title_label.set_line_wrap_mode(pango::WrapMode::WordChar);
            item.bind_property("title", &title_label, "label")
                .flags(glib::BindingFlags::DEFAULT | glib::BindingFlags::SYNC_CREATE)
                .build();
            grid.attach(&title_label, 1, 0, 1, 1);

            let album_label = gtk::Label::new(None);
            album_label.set_line_wrap(true);
            album_label.set_line_wrap_mode(pango::WrapMode::WordChar);
            item.bind_property("album", &album_label, "label")
                .flags(glib::BindingFlags::DEFAULT | glib::BindingFlags::SYNC_CREATE)
                .build();
            grid.attach(&album_label, 2, 0, 1, 1);

            let artist_label = gtk::Label::new(None);
            artist_label.set_line_wrap(true);
            artist_label.set_line_wrap_mode(pango::WrapMode::WordChar);
            item.bind_property("artist", &artist_label, "label")
                .flags(glib::BindingFlags::DEFAULT | glib::BindingFlags::SYNC_CREATE)
                .build();
            grid.attach(&artist_label, 3, 0, 1, 1);

            grid.show_all();
            box_.add(&grid);
            box_.upcast::<gtk::Widget>()
        }));

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

// Unfortunately, to use the `ListStore` interface, we'll need to represent our
// data as an actual `glib` object. This is a little hairy in Rust, involving a
// fair bit of boilerplate, but not too terrible.
glib::wrapper! {
    pub struct SongObject(ObjectSubclass<imp::SongObject>);
}

impl SongObject {
    pub fn new(song: &mpd::song::Song) -> Self {
        glib::Object::new(&[
            ("filename", &song.file.clone()),
            (
                "title",
                &song
                    .title
                    .as_ref()
                    .cloned()
                    .unwrap_or_else(|| "[Untitled]".into()),
            ),
            (
                "artist",
                &song
                    .artist
                    .as_ref()
                    .cloned()
                    .unwrap_or_else(|| "[No Artist]".into()),
            ),
            (
                "album",
                &song
                    .tags
                    .get("Album")
                    .cloned()
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

// These class "implementations" are typically done in a separate
// file/directory. I wanted to keep the example self-contained.
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
