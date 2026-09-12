/// Re-export [`url::Url`] since it is an input type for callers of the API.
pub use url::Url;

/// Represents a video file or stream location. Can be either a file resource (a path) or a network
/// resource (a URL).
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub enum Location {
    /// File source.
    File(std::path::PathBuf),
    /// Network source.
    Network(Url),
}

impl Location {
    /// Coerce underlying location to a path.
    ///
    /// This will create a path with a URL in it (which is kind of weird but we use it to pass on
    /// URLs to ffmpeg).
    pub fn as_path(&self) -> &std::path::Path {
        match self {
            Location::File(path) => path.as_path(),
            Location::Network(url) => std::path::Path::new(url.as_str()),
        }
    }
}

impl From<&Location> for Location {
    fn from(value: &Location) -> Location {
        value.clone()
    }
}

impl From<std::path::PathBuf> for Location {
    fn from(value: std::path::PathBuf) -> Location {
        Location::File(value)
    }
}

impl From<&std::path::PathBuf> for Location {
    fn from(value: &std::path::PathBuf) -> Location {
        Location::File(value.clone())
    }
}

impl From<&std::path::Path> for Location {
    fn from(value: &std::path::Path) -> Location {
        Location::File(value.to_path_buf())
    }
}

impl From<Url> for Location {
    fn from(value: Url) -> Location {
        Location::Network(value)
    }
}

impl From<&Url> for Location {
    fn from(value: &Url) -> Location {
        Location::Network(value.clone())
    }
}

/// Classifies a string into a [`Location`].
///
/// The string is parsed as a URL: a parseable, non-`file` scheme becomes a
/// network source (`http`, `https`, `rtsp`, `rtmp`, `srt`, ... -- anything
/// libavformat itself accepts), a `file:` URL maps back to a filesystem path,
/// and anything else (including Windows drive paths such as `C:\video\a.mov`,
/// whose `C` would otherwise look like a scheme) is treated as a plain path.
fn location_from_str(value: &str) -> Location {
    match Url::parse(value) {
        Ok(url) if url.scheme() == "file" => {
            Location::File(url.to_file_path().unwrap_or_else(|_| value.into()))
        }
        // A one-character scheme is a Windows drive letter, not a protocol.
        Ok(url) if url.scheme().len() == 1 => Location::File(value.into()),
        Ok(url) => Location::Network(url),
        // Not parseable as a URL: a plain (possibly relative) filesystem path.
        Err(_) => Location::File(value.into()),
    }
}

impl From<&str> for Location {
    fn from(value: &str) -> Location {
        location_from_str(value)
    }
}

impl From<String> for Location {
    fn from(value: String) -> Location {
        location_from_str(&value)
    }
}

impl From<&String> for Location {
    fn from(value: &String) -> Location {
        location_from_str(value)
    }
}

impl std::fmt::Display for Location {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Location::File(path) => write!(f, "{}", path.display()),
            Location::Network(url) => write!(f, "{url}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_str_classification() {
        // Plain paths -> File.
        assert_eq!(
            Location::from("/tmp/aaa.mov"),
            Location::File("/tmp/aaa.mov".into())
        );
        assert_eq!(
            Location::from("assets/mp4.mp4"),
            Location::File("assets/mp4.mp4".into())
        );
        // Relative names with a colon still parse as URLs; document the edge.
        assert!(matches!(
            Location::from("protocol:style.mp4"),
            Location::Network(_)
        ));

        // Network schemes -> Network.
        assert!(matches!(
            Location::from("http://127.0.0.1:8088/aaa/video.mp4"),
            Location::Network(_)
        ));
        assert!(matches!(
            Location::from("rtsp://cam.local/stream"),
            Location::Network(_)
        ));
        assert!(matches!(
            Location::from("srt://host:9000"),
            Location::Network(_)
        ));

        // `file:` URLs map back to a plain path. On Unix, `file:///tmp/...` maps to
        // `/tmp/...`. On Windows, `file:///tmp/...` is not a valid Windows absolute
        // path, so `to_file_path()` fails and the raw URL string is kept as-is.
        let file_url = Location::from("file:///tmp/aaa.mov");
        #[cfg(not(windows))]
        let expected = Location::File("/tmp/aaa.mov".into());
        #[cfg(windows)]
        let expected = Location::File("file:///tmp/aaa.mov".into());
        assert_eq!(file_url, expected);

        // Windows drive letters are paths, not a "c:" protocol.
        assert_eq!(
            Location::from("C:\\video\\aaa.mov"),
            Location::File("C:\\video\\aaa.mov".into())
        );

        // Owned/borrowed string wrappers agree.
        let owned = String::from("http://example.com/a.mp4");
        assert_eq!(
            Location::from(owned.clone()),
            Location::from(owned.as_str())
        );
    }

    #[test]
    fn test_string_location_feeds_reader() {
        // The whole point: string literals plug straight into the decoding
        // entry points (compile-time check; no network access happens).
        fn assert_reader_source(_: impl Into<Location>) {}
        assert_reader_source("/tmp/aaa.mov");
        assert_reader_source("http://127.0.0.1:8088/aaa/video.mp4");
        assert_reader_source(String::from("/tmp/aaa.mov"));
        assert_reader_source(std::path::Path::new("/tmp/aaa.mov"));
        assert_reader_source(std::path::PathBuf::from("/tmp/aaa.mov"));
    }
}
