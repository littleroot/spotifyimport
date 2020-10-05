# spotifyimport

`spotifyimport` is a command line program to import songs into Spotify from
your Apple Music library.

Songs will added to "Liked Songs" in Spotify.  The program reads Apple Music
library data from [scrobbl.es][api_doc] API response JSON, which should be fed
into stdin.

Run with the `--help` flag to print help information.

By default, the program runs in "dry run" mode (it doesn't actually add songs
to Spotify, but just prints what it would do). Use the `--mutate` flag to
actually add songs.

The list of songs that fail to be added, if any, will be written to
`failures_<timestamp>.json`.

[api_doc]: https://scrobbl.es/doc/api/v1/scrobbled
