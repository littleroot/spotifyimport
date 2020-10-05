use anyhow::{bail, Context, Error};
use chrono;
use futures::future::join_all;
use getopts::Options;
use log::*;
use logosaurus::{self, Logger, L_LEVEL, L_TIME};
use reqwest::Client as HttpClient;
use serde::{Deserialize, Serialize};
use serde_json;
use spmc;
use spotifyimport::access_token::{self, TokenResponse};
use std::env;
use std::fmt;
use std::fs::File;
use std::io;
use std::io::BufReader;
use std::process;
use std::sync::Arc;
use std::sync::Mutex;
use tokio;
use tokio::sync::mpsc;

#[tokio::main]
async fn main() {
    let logger = Logger::builder()
        .set_prefix("spotifyimport: ")
        .set_flags(L_LEVEL | L_TIME)
        .set_level(LevelFilter::Info)
        .build();
    logosaurus::init(logger).unwrap();

    if let Err(e) = run().await {
        error!("{:#}", e);
        process::exit(1);
    }
}

const N_WORKERS: u32 = 16;

fn print_help() {
    eprintln!(
        "usage: {} [--mutate] <sp_dc> <sp_key>",
        env::args().nth(0).unwrap()
    );
}

#[derive(Debug)]
enum AddStatus {
    Added(Song, String),   // song, id
    Skipped(Song, String), // song, reason
}

async fn run() -> Result<(), Error> {
    // parse arguments
    let sp_dc = match env::args().skip(1).nth(0) {
        Some(t) => t,
        None => {
            print_help();
            process::exit(2);
        }
    };
    let sp_key = match env::args().skip(1).nth(1) {
        Some(t) => t,
        None => {
            print_help();
            process::exit(2);
        }
    };

    // parse flags
    let mut opts = Options::new();
    opts.optflag("", "mutate", "actually make changes (add songs)");
    opts.optflag("h", "help", "print help information");
    let matches = match opts.parse(&env::args().skip(1).collect::<Vec<String>>()) {
        Ok(m) => m,
        Err(f) => {
            eprintln!("{}", f);
            print_help();
            process::exit(2);
        }
    };
    if matches.opt_present("h") {
        print_help();
        process::exit(0);
    }
    let mutate = matches.opt_present("mutate");

    let http_client = HttpClient::new();

    // NOTE: the expiry seems to be 1 hour, which should suffice for our purposes.
    let TokenResponse { access_token, .. } =
        access_token::fetch(http_client.clone(), &sp_dc, &sp_key)
            .await
            .context("fetch access token")?;

    // read scrobbled songs
    let r = BufReader::new(io::stdin());
    let s: Scrobbled = serde_json::from_reader(r).context("json deserialize")?;
    let songs = s.songs;

    // work channel
    let (mut tx, rx) = spmc::channel::<Song>();
    let mut handles = Vec::new();

    let (added_tx, mut added_rx) = mpsc::channel::<AddStatus>(1);

    // send work along channel
    handles.push(tokio::spawn(async move {
        for song in songs {
            tx.send(song).unwrap();
        }
    }));

    // consume work from channel
    for _ in 0..N_WORKERS {
        let rx = rx.clone();
        let mut added_tx = added_tx.clone();
        let http_client = http_client.clone();
        let token = access_token.clone();

        handles.push(tokio::spawn(async move {
            loop {
                match rx.recv() {
                    Ok(song) => match search_spotify_track(&http_client, &token, &song).await {
                        Ok(id) => {
                            if mutate {
                                if let Err(e) =
                                    add_spotify_liked_track(&http_client, &token, &id).await
                                {
                                    added_tx
                                        .send(AddStatus::Skipped(
                                            song,
                                            format!("{}: {}", "add track", e),
                                        ))
                                        .await
                                        .unwrap();
                                    continue;
                                }
                            }
                            added_tx
                                .send(AddStatus::Added(song, String::from(id)))
                                .await
                                .unwrap();
                        }
                        Err(e) => {
                            added_tx
                                .send(AddStatus::Skipped(
                                    song,
                                    format!("{}: {}", "search track", e),
                                ))
                                .await
                                .unwrap();
                        }
                    },
                    Err(_) => break,
                }
            }
        }));
    }

    drop(added_tx);

    // collect added/failure info
    let added = Arc::new(Mutex::new(0));
    let total = s.total;
    let failed_songs: Arc<Mutex<Vec<Song>>> = Arc::new(Mutex::new(Vec::new()));

    let added_clone = Arc::clone(&added);
    let failed_songs_clone = Arc::clone(&failed_songs);

    handles.push(tokio::spawn(async move {
        loop {
            match added_rx.recv().await {
                Some(AddStatus::Added(song, id)) => {
                    info!("added {} {}", song, id);
                    let mut v = added_clone.lock().unwrap();
                    *v += 1;
                }
                Some(AddStatus::Skipped(song, reason)) => {
                    failed_songs_clone.lock().unwrap().push(song.clone());
                    error!("{}; skipped {}", reason, song);
                }
                None => {
                    break;
                }
            }
        }
    }));

    join_all(handles).await;

    let failure_filename = format!("failures_{}.json", chrono::offset::Local::now().timestamp(),);

    info!(
        "total songs: {}, added: {}, skipped songs written to: {}",
        total,
        added.lock().unwrap(),
        failure_filename,
    );

    let f = File::create(failure_filename).context("create output file")?;
    let failed_vec = Arc::try_unwrap(failed_songs).unwrap().into_inner().unwrap();
    serde_json::to_writer_pretty(f, &failed_vec).context("write failed songs")?;

    Ok(())
}

async fn search_spotify_track(c: &HttpClient, token: &str, song: &Song) -> Result<String, Error> {
    let url = "https://api.spotify.com/v1/search";
    let q = search_query(&song.title, &song.artist_name, &song.album_title);

    let rsp = c
        .get(url)
        .header("authorization", format!("Bearer {}", token))
        .query(&[("q", &q[..]), ("type", "track"), ("limit", "1")])
        .send()
        .await
        .context("build and execute request")?;

    if rsp.status() != 200 {
        bail!("bad status code: {}", rsp.status());
    }

    let rsp: SearchResponse = rsp.json().await.context("json deserialize")?;
    if rsp.tracks.items.is_empty() {
        bail!("found zero tracks");
    }

    Ok(rsp.tracks.items[0].id.clone())
}

async fn add_spotify_liked_track(c: &HttpClient, token: &str, id: &str) -> Result<(), Error> {
    let url = "https://api.spotify.com/v1/me/tracks";
    let rsp = c
        .put(url)
        .header("authorization", format!("Bearer {}", token))
        .query(&["ids", id])
        .send()
        .await
        .context("build and execute request")?;

    if rsp.status() != 200 {
        bail!("bad status code: {}", rsp.status());
    }

    Ok(())
}

// Apple Music uses these suffixes, but Spotify doesn't.
const ALBUM_TRIM_SUFFIXES: &[&str] = &[
    " - EP",
    " - Single",
    " (Bonus Track Version)",
    " (Original Motion Picture Soundtrack)",
    " (Special Edition)",
    " (Deluxe Edition)",
    " (Deluxe Version)",
    " (Deluxe)",
    " (Deluxe Edition with Videos)",
    " (Extended Version)",
];

fn search_query(track: &str, artist: &str, album: &str) -> String {
    let mut buf = String::new();

    if !track.is_empty() {
        buf.push_str(&format!("track:{} ", track));
    }

    if !artist.is_empty() {
        buf.push_str(&format!("artist:{} ", artist));
    }

    let mut album = album;
    for suffix in ALBUM_TRIM_SUFFIXES {
        album = album.strip_suffix(suffix).unwrap_or(album);
    }
    if !album.is_empty() {
        buf.push_str(&format!("album:{} ", album));
    }

    buf
}

#[derive(Debug, Deserialize)]
struct SearchResponse {
    tracks: Tracks,
}

#[derive(Debug, Deserialize)]
struct Tracks {
    items: Vec<Item>,
}

#[derive(Debug, Deserialize)]
struct Item {
    id: String,
    uri: SpotifyUri,
}

type SpotifyUri = String;

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct Scrobbled {
    total: u32,
    songs: Vec<Song>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
struct Song {
    album_title: String,
    artist_name: String,
    title: String,
    year: u32,
    loved: bool,
    ident: String,
}

impl fmt::Display for Song {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "({}, {}, {})",
            self.title, self.artist_name, self.album_title
        )
    }
}
