use anyhow::{bail, Context, Error};
use log::*;
use logosaurus::{self, Logger, L_LEVEL, L_TIME};
use reqwest::Client as HttpClient;
use serde::Deserialize;
use serde_json;
use spmc;
use std::fmt;
use std::io;
use std::io::BufReader;
use std::process;
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

async fn run() -> Result<(), Error> {
    // read scrobbled songs
    let r = BufReader::new(io::stdin());
    let s: Scrobbled = serde_json::from_reader(r).context("json deserialize")?;
    let songs = s.songs;

    let http_client = HttpClient::new();

    let (mut tx, rx) = spmc::channel::<Song>();
    let mut handles = Vec::new();

    handles.push(tokio::spawn(async move {
        for song in songs {
            tx.send(song).unwrap();
        }
    }));

    for _ in 0..N_WORKERS {
        let rx = rx.clone();
        let http_client = http_client.clone();
        handles.push(tokio::spawn(async move {
            loop {
                let http_client = http_client.clone();
                match rx.recv() {
                    Ok(song) => match search_spotify_track(http_client, &song).await {
                        Ok(_) => {
                            info!("adding song {}", song);
                        }
                        Err(e) => error!("{}; skipping {}", e, song),
                    },
                    Err(_) => break,
                }
            }
        }));
    }

    for handle in handles {
        handle.await.unwrap();
    }

    Ok(())
}

async fn search_spotify_track(c: HttpClient, song: &Song) -> Result<SpotifyUri, Error> {
    const URL: &str = "https://api.spotify.com/v1/search";
    let q = SearchQuery {
        track: song.title.clone(),
        artist: song.artist_name.clone(),
        album: song.album_title.clone(),
    }
    .encode();

    let req = c
        .get(URL)
        .query(&[("q", &q[..]), ("type", "track"), ("limit", "1")])
        .build()
        .context("build request")?;

    let rsp = c.execute(req).await.context("execute request")?;
    if rsp.status() != 200 {
        bail!("bad status code: {}", rsp.status());
    }

    let rsp: SearchResponse = rsp.json().await.context("json deserialize")?;
    if rsp.tracks.items.is_empty() {
        bail!("found zero tracks");
    }

    Ok(rsp.tracks.items[0].uri.clone())
}

struct SearchQuery {
    track: String,
    artist: String,
    album: String,
}

impl SearchQuery {
    fn encode(&self) -> String {
        let mut buf = String::new();
        if !self.track.is_empty() {
            buf.push_str(&format!("track:{} ", self.track));
        }
        if !self.artist.is_empty() {
            buf.push_str(&format!("artist:{} ", self.artist));
        }
        if !self.album.is_empty() {
            buf.push_str(&format!("album:{} ", self.album));
        }
        buf
    }
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
