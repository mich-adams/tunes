use futures::{channel::mpsc, StreamExt};
use gtk::prelude::*;
use libhandy::prelude::*;
use libhandy::{ApplicationWindow, HeaderBar};
use mpd::idle::Idle;
use mpd::Client;

struct TunesUI {
    header_bar: HeaderBar,
    // album_art: Image,
    // queue_switcher: Notebook,
}

fn header_title(conn: &mut mpd::client::Client) -> mpd::error::Result<String> {
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
        Ok("Tunes".into())
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
        container.add(&album_art);
        container.add(&song_text);

        SongInfo {
            container,
            album_art,
            song_text,
        }
    }

    fn update(&self) -> mpd::error::Result<()> {
        let mut conn = Client::connect("127.0.0.1:6600").unwrap();
        if let Some(song) = conn.currentsong()? {
            let image_data = conn.albumart(&song).unwrap();
            let image_pixbuf = gtk::gdk_pixbuf::Pixbuf::from_stream(
                &gtk::gio::MemoryInputStream::from_bytes(&gtk::glib::Bytes::from(&image_data)),
                gtk::gio::Cancellable::NONE,
            )
            .ok()
            .and_then(|x| x.scale_simple(128, 128, gtk::gdk_pixbuf::InterpType::Hyper));
            self.album_art.set_pixbuf(image_pixbuf.as_ref());

            let ssdf = "[Unknown]".into();
            let title = song.title.unwrap_or_else(|| "[Unknown]".into());
            let album = song.tags.get("Album").unwrap_or(&ssdf);
            let artist = song.artist.unwrap_or_else(|| "[Unknown]".into());
            let string = format!("{}\n{} - {}", title, artist, album);
            self.song_text.set_text(&string);

            let attr_list = gtk::pango::AttrList::new();

            let mut attr = gtk::pango::AttrFloat::new_scale(2.0);
            attr.set_start_index(0);
            attr.set_end_index(title.len() as u32);
            attr_list.insert(attr);

            let mut attr = gtk::pango::AttrFloat::new_scale(1.5);
            attr.set_start_index(title.len() as u32 + 1);
            // attr.set_end_index(title.len() as u32 + 1 + album.len() as u32);
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
        // let (song_info_view, song_info_container) = ().unwrap();
        let song_info = SongInfo::new();
        song_info.update().expect("Couldn't update song info");
        stack.add_named(song_info.as_ref(), "Currently Playing");

        let header_bar = HeaderBar::builder()
            .show_close_button(true)
            .title(&header_title(&mut conn).unwrap())
            .build();
        let asdf = libhandy::ViewSwitcherTitle::builder()
            .title("Tunes")
            .stack(&stack)
            .build();
        header_bar.add(&asdf);

        let ui = TunesUI { header_bar };

        // Combine the content in a box
        let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
        // Handy's ApplicationWindow does not include a HeaderBar
        content.add(&ui.header_bar);
        content.add(&stack);

        let window = ApplicationWindow::builder()
            .default_width(350)
            .default_height(70)
            // add content to window
            .child(&content)
            .build();
        window.set_application(Some(app));
        window.show_all();

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
                    song_info.update().expect("Couldn't update song info");
                }
            }
        });
    });

    application.run();
}
