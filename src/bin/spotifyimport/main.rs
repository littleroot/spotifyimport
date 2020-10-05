use anyhow::{bail, Context, Error};
use log::*;
use logosaurus::{self, Logger, L_LEVEL, L_TIME};
use reqwest::Client as HttpClient;
use serde::Deserialize;
use serde_json;
use spmc;
use spotifyimport::access_token::{self, TokenResponse};
use std::env;
use std::fmt;
use std::io;
use std::io::BufReader;
use std::process;
use std::sync::mpsc;
use tokio;

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
        "usage: {} [flags] <sp_dc> <sp_key>",
        env::args().nth(0).unwrap()
    );
}

enum AddStatus {
    Added,
    Skipped,
}

async fn run() -> Result<(), Error> {
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

    let dry_run = true;

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

    // found counts
    let (added_tx, added_rx) = mpsc::channel::<AddStatus>();
    let mut added = 0;
    let total = s.total;

    // send work along channel
    handles.push(tokio::spawn(async move {
        for song in songs {
            tx.send(song).unwrap();
        }
    }));

    // consume work from channel
    for _ in 0..N_WORKERS {
        let rx = rx.clone();
        let added_tx = added_tx.clone();
        let http_client = http_client.clone();
        let token = access_token.clone();

        handles.push(tokio::spawn(async move {
            loop {
                match rx.recv() {
                    Ok(song) => match search_spotify_track(&http_client, &token, &song).await {
                        Ok(id) => {
                            if !dry_run {
                                if let Err(e) =
                                    add_spotify_liked_track(&http_client, &token, &id).await
                                {
                                    error!("add track: {}; skipped {}", e, song);
                                    continue;
                                }
                            }
                            added_tx.send(AddStatus::Added).unwrap();
                            info!("added {} {}", song, id);
                        }
                        Err(e) => {
                            added_tx.send(AddStatus::Skipped).unwrap();
                            error!("search track: {}; skipped {}", e, song);
                        }
                    },
                    Err(_) => break,
                }
            }
        }));
    }

    drop(added_tx);

    for handle in handles {
        handle.await.unwrap();
    }

    loop {
        match added_rx.recv() {
            Ok(AddStatus::Added) => {
                added += 1;
            }
            Ok(_) => {}
            Err(_) => break,
        }
    }

    info!("total songs: {}, added: {}", total, added);
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
        album = match album.strip_suffix(suffix) {
            Some(s) => s,
            None => album,
        };
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

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
struct Song {
    album_title: String,
    artist_name: String,
    title: String,
    year: u32,
    loved: bool,
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
