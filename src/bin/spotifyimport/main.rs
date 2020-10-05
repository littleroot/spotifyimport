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
    eprintln!("usage: {} <sp_dc> <sp_key>", env::args().nth(0).unwrap());
}

enum FoundStatus {
    Found,
    NotFound,
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
    let (found_tx, found_rx) = mpsc::channel::<FoundStatus>();
    let mut found = 0;
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
        let found_tx = found_tx.clone();
        let http_client = http_client.clone();
        let token = access_token.clone();

        handles.push(tokio::spawn(async move {
            loop {
                let http_client = http_client.clone();
                match rx.recv() {
                    Ok(song) => match search_spotify_track(http_client, &token, &song).await {
                        Ok(_) => {
                            info!("adding {}", song);
                            found_tx.send(FoundStatus::Found).unwrap();
                        }
                        Err(e) => {
                            error!("{}; skipping {}", e, song);
                            found_tx.send(FoundStatus::NotFound).unwrap();
                        }
                    },
                    Err(_) => break,
                }
            }
        }));
    }

    for handle in handles {
        handle.await.unwrap();
    }

    drop(found_tx);

    loop {
        match found_rx.recv() {
            Ok(FoundStatus::Found) => {
                found += 1;
            }
            Ok(_) => {}
            Err(_) => break,
        }
    }

    info!("total songs: {}, found: {}", total, found);
    Ok(())
}

async fn search_spotify_track(
    c: HttpClient,
    token: &str,
    song: &Song,
) -> Result<SpotifyUri, Error> {
    const URL: &str = "https://api.spotify.com/v1/search";
    let q = search_query(&song.title, &song.artist_name, &song.album_title);

    let rsp = c
        .get(URL)
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

    Ok(rsp.tracks.items[0].uri.clone())
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
