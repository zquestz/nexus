//! URI parser for nexus:// scheme
//!
//! Supports URIs of the form:
//! ```text
//! nexus://[user[:password]@]host[:port][/path]
//! ```
//!
//! Path intents:
//! - `/chat/#channel` - Open/focus channel tab
//! - `/chat/user` - Open/focus user message tab
//! - `/files/path` - Open Files panel to path
//! - `/news` - Open News panel
//! - `/info` - Open Server Info panel

use std::fmt;

use percent_encoding::{AsciiSet, NON_ALPHANUMERIC, percent_decode_str, percent_encode};

/// Default BBS port
const DEFAULT_PORT: u16 = 7500;

/// Parsed nexus:// URI
#[derive(Clone, PartialEq)]
pub struct NexusUri {
    /// Optional username for authentication
    pub user: Option<String>,
    /// Optional password for authentication (only valid with user)
    pub password: Option<String>,
    /// Server hostname or IP address
    pub host: String,
    /// Server port (defaults to 7500)
    pub port: u16,
    /// Optional path intent
    pub path: Option<NexusPath>,
}

impl NexusUri {
    /// Check if this URI has credentials
    pub fn has_credentials(&self) -> bool {
        self.user.is_some()
    }
}

impl fmt::Debug for NexusUri {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("NexusUri")
            .field("user", &self.user)
            .field("password", &self.password.as_ref().map(|_| "[REDACTED]"))
            .field("host", &self.host)
            .field("port", &self.port)
            .field("path", &self.path)
            .finish()
    }
}

impl fmt::Display for NexusUri {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "nexus://")?;

        if let Some(ref user) = self.user {
            write!(f, "{}", url_encode_userinfo(user))?;
            if let Some(ref pass) = self.password {
                write!(f, ":{}", url_encode_userinfo(pass))?;
            }
            write!(f, "@")?;
        }

        // IPv6 addresses need brackets
        if self.host.contains(':') {
            write!(f, "[{}]", self.host)?;
        } else {
            write!(f, "{}", self.host)?;
        }

        if self.port != DEFAULT_PORT {
            write!(f, ":{}", self.port)?;
        }

        if let Some(ref path) = self.path {
            write!(f, "{}", path)?;
        }

        Ok(())
    }
}

/// Path intent within a nexus:// URI
#[derive(Debug, Clone, PartialEq)]
pub enum NexusPath {
    /// Open/focus chat panel, optionally a specific tab
    Chat {
        /// Target name (channel name or username), None = just show chat
        target: Option<String>,
        /// True if target is a channel (starts with #)
        is_channel: bool,
    },
    /// Open Files panel to a path
    Files {
        /// Path within the file area
        path: String,
    },
    /// Open News panel
    News,
    /// Open Server Info panel
    Info,
}

impl fmt::Display for NexusPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NexusPath::Chat { target, is_channel } => match target {
                Some(t) if *is_channel => write!(f, "/chat/#{}", t),
                Some(t) => write!(f, "/chat/{}", t),
                None => write!(f, "/chat"),
            },
            NexusPath::Files { path } => write!(f, "/files/{}", path),
            NexusPath::News => write!(f, "/news"),
            NexusPath::Info => write!(f, "/info"),
        }
    }
}

/// Error type for URI parsing
#[derive(Debug, Clone, PartialEq)]
pub enum ParseError {
    /// URI doesn't start with nexus://
    InvalidScheme,
    /// Missing host component
    MissingHost,
    /// Invalid port number
    InvalidPort,
    /// Invalid URI format
    InvalidFormat(String),
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ParseError::InvalidScheme => write!(f, "URI must start with nexus://"),
            ParseError::MissingHost => write!(f, "Missing host in URI"),
            ParseError::InvalidPort => write!(f, "Invalid port number"),
            ParseError::InvalidFormat(msg) => write!(f, "Invalid URI format: {}", msg),
        }
    }
}

impl std::error::Error for ParseError {}

/// Parse a nexus:// URI string
pub fn parse(uri: &str) -> Result<NexusUri, ParseError> {
    // Check and strip scheme
    let uri = uri
        .strip_prefix("nexus://")
        .ok_or(ParseError::InvalidScheme)?;

    // Split into authority and path
    let (authority, path_str) = match uri.find('/') {
        Some(idx) => (&uri[..idx], Some(&uri[idx..])),
        None => (uri, None),
    };

    // Parse authority: [user[:password]@]host[:port]
    let (userinfo, hostport) = match authority.rfind('@') {
        Some(idx) => (Some(&authority[..idx]), &authority[idx + 1..]),
        None => (None, authority),
    };

    // Parse userinfo if present
    let (user, password) = if let Some(userinfo) = userinfo {
        // URL-decode the userinfo components
        match userinfo.find(':') {
            Some(idx) => (
                Some(url_decode(&userinfo[..idx])),
                Some(url_decode(&userinfo[idx + 1..])),
            ),
            None => (Some(url_decode(userinfo)), None),
        }
    } else {
        (None, None)
    };

    // Parse host and port, handling IPv6 addresses in brackets
    let (host, port) = parse_host_port(hostport)?;

    if host.is_empty() {
        return Err(ParseError::MissingHost);
    }

    // Parse path intent
    let path = if let Some(path_str) = path_str {
        parse_path(path_str)?
    } else {
        None
    };

    Ok(NexusUri {
        user,
        password,
        host,
        port,
        path,
    })
}

/// Check if a string looks like an IPv6 address (contains multiple colons)
fn looks_like_ipv6(s: &str) -> bool {
    // IPv6 addresses have at least 2 colons (e.g., ::1, 2001:db8::1)
    // Yggdrasil addresses look like 202:e7f:a50e:d03b:e13e:75f1:24c9:58bc
    s.chars().filter(|&c| c == ':').count() >= 2
}

/// Parse host and port from hostport string, handling IPv6 brackets
fn parse_host_port(hostport: &str) -> Result<(String, u16), ParseError> {
    if hostport.starts_with('[') {
        // IPv6 address in brackets: [::1]:7500 or [::1]
        let end_bracket = hostport
            .find(']')
            .ok_or_else(|| ParseError::InvalidFormat("Unclosed bracket in IPv6 address".into()))?;

        let host = hostport[1..end_bracket].to_string();
        let after_bracket = &hostport[end_bracket + 1..];

        let port = if let Some(port_str) = after_bracket.strip_prefix(':') {
            port_str.parse().map_err(|_| ParseError::InvalidPort)?
        } else if after_bracket.is_empty() {
            DEFAULT_PORT
        } else {
            return Err(ParseError::InvalidFormat(
                "Invalid characters after IPv6 address".into(),
            ));
        };

        Ok((host, port))
    } else if looks_like_ipv6(hostport) {
        // Unbracketed IPv6 address (e.g., Yggdrasil: 202:e7f:a50e:d03b:e13e:75f1:24c9:58bc)
        // IPv6 can't have a port without brackets, so the whole thing is the host
        Ok((hostport.to_string(), DEFAULT_PORT))
    } else {
        // IPv4 or hostname: example.com:7500 or example.com
        match hostport.rfind(':') {
            Some(idx) => {
                let host = hostport[..idx].to_string();
                let port = hostport[idx + 1..]
                    .parse()
                    .map_err(|_| ParseError::InvalidPort)?;
                Ok((host, port))
            }
            None => Ok((hostport.to_string(), DEFAULT_PORT)),
        }
    }
}

/// Parse the path component into a NexusPath
fn parse_path(path: &str) -> Result<Option<NexusPath>, ParseError> {
    // Handle empty path or just "/"
    if path.is_empty() || path == "/" {
        return Ok(None);
    }

    // URL-decode the path
    let path = url_decode(path);

    // Split path into segments
    let path = path.strip_prefix('/').unwrap_or(&path);
    let mut segments = path.splitn(2, '/');

    let first = segments.next().unwrap_or("");
    let rest = segments.next().unwrap_or("");

    match first.to_lowercase().as_str() {
        "chat" => {
            if rest.is_empty() {
                // /chat alone - just go to chat panel
                return Ok(Some(NexusPath::Chat {
                    target: None,
                    is_channel: false,
                }));
            }

            // Check for channel prefix (#)
            let (target, is_channel) = if let Some(channel) = rest.strip_prefix('#') {
                (channel.to_string(), true)
            } else {
                (rest.to_string(), false)
            };

            if target.is_empty() {
                return Ok(Some(NexusPath::Chat {
                    target: None,
                    is_channel: false,
                }));
            }

            Ok(Some(NexusPath::Chat {
                target: Some(target),
                is_channel,
            }))
        }
        "files" => {
            // rest can be empty (root) or a path
            Ok(Some(NexusPath::Files {
                path: rest.to_string(),
            }))
        }
        "news" => Ok(Some(NexusPath::News)),
        "info" => Ok(Some(NexusPath::Info)),
        _ => {
            // Unknown path type - just connect without intent
            Ok(None)
        }
    }
}

/// URL decoding for percent-encoded characters
///
/// Properly handles multi-byte UTF-8 sequences (e.g., %C3%A9 → é)
fn url_decode(s: &str) -> String {
    percent_decode_str(s).decode_utf8_lossy().into_owned()
}

/// Characters to encode in userinfo (everything except unreserved per RFC 3986)
/// Unreserved: A-Z a-z 0-9 - . _ ~
const USERINFO_ENCODE_SET: &AsciiSet = &NON_ALPHANUMERIC
    .remove(b'-')
    .remove(b'.')
    .remove(b'_')
    .remove(b'~');

/// Characters to encode in path (everything except unreserved + slash per RFC 3986)
/// Unreserved: A-Z a-z 0-9 - . _ ~ /
const PATH_ENCODE_SET: &AsciiSet = &NON_ALPHANUMERIC
    .remove(b'-')
    .remove(b'.')
    .remove(b'_')
    .remove(b'~')
    .remove(b'/');

/// Percent-encode a string for use in URI userinfo (username or password)
fn url_encode_userinfo(s: &str) -> String {
    percent_encode(s.as_bytes(), USERINFO_ENCODE_SET).to_string()
}

/// Percent-encode a string for use in URI path
pub fn url_encode_path(s: &str) -> String {
    percent_encode(s.as_bytes(), PATH_ENCODE_SET).to_string()
}

/// Check if a string looks like a nexus:// URI
pub fn is_nexus_uri(s: &str) -> bool {
    s.starts_with("nexus://")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_host() {
        let uri = parse("nexus://example.com").unwrap();
        assert_eq!(uri.host, "example.com");
        assert_eq!(uri.port, DEFAULT_PORT);
        assert!(uri.user.is_none());
        assert!(uri.password.is_none());
        assert!(uri.path.is_none());
    }

    #[test]
    fn test_parse_host_with_port() {
        let uri = parse("nexus://example.com:8500").unwrap();
        assert_eq!(uri.host, "example.com");
        assert_eq!(uri.port, 8500);
    }

    #[test]
    fn test_parse_with_user() {
        let uri = parse("nexus://alice@example.com").unwrap();
        assert_eq!(uri.host, "example.com");
        assert_eq!(uri.user, Some("alice".to_string()));
        assert!(uri.password.is_none());
    }

    #[test]
    fn test_parse_with_user_and_password() {
        let uri = parse("nexus://shared_acct:sharedpass@example.com").unwrap();
        assert_eq!(uri.host, "example.com");
        assert_eq!(uri.user, Some("shared_acct".to_string()));
        assert_eq!(uri.password, Some("sharedpass".to_string()));
    }

    #[test]
    fn test_parse_ipv6() {
        let uri = parse("nexus://[::1]").unwrap();
        assert_eq!(uri.host, "::1");
        assert_eq!(uri.port, DEFAULT_PORT);

        let uri = parse("nexus://[::1]:8500").unwrap();
        assert_eq!(uri.host, "::1");
        assert_eq!(uri.port, 8500);

        let uri = parse("nexus://[2001:db8::1]:7500").unwrap();
        assert_eq!(uri.host, "2001:db8::1");
        assert_eq!(uri.port, 7500);
    }

    #[test]
    fn test_parse_ipv6_unbracketed() {
        // Yggdrasil-style unbracketed IPv6
        let uri = parse("nexus://202:e7f:a50e:d03b:e13e:75f1:24c9:58bc").unwrap();
        assert_eq!(uri.host, "202:e7f:a50e:d03b:e13e:75f1:24c9:58bc");
        assert_eq!(uri.port, DEFAULT_PORT);

        // With path
        let uri = parse("nexus://202:e7f:a50e:d03b:e13e:75f1:24c9:58bc/news").unwrap();
        assert_eq!(uri.host, "202:e7f:a50e:d03b:e13e:75f1:24c9:58bc");
        assert_eq!(uri.port, DEFAULT_PORT);
        assert_eq!(uri.path, Some(NexusPath::News));

        // Unbracketed ::1
        let uri = parse("nexus://::1").unwrap();
        assert_eq!(uri.host, "::1");
        assert_eq!(uri.port, DEFAULT_PORT);
    }

    #[test]
    fn test_parse_ipv6_with_user() {
        let uri = parse("nexus://alice@[::1]:8500").unwrap();
        assert_eq!(uri.host, "::1");
        assert_eq!(uri.port, 8500);
        assert_eq!(uri.user, Some("alice".to_string()));

        // Unbracketed with user
        let uri = parse("nexus://alice@202:e7f:a50e:d03b:e13e:75f1:24c9:58bc/files").unwrap();
        assert_eq!(uri.host, "202:e7f:a50e:d03b:e13e:75f1:24c9:58bc");
        assert_eq!(uri.user, Some("alice".to_string()));
        assert_eq!(
            uri.path,
            Some(NexusPath::Files {
                path: "".to_string()
            })
        );
    }

    #[test]
    fn test_parse_chat_channel() {
        let uri = parse("nexus://example.com/chat/#general").unwrap();
        assert_eq!(uri.host, "example.com");
        assert_eq!(
            uri.path,
            Some(NexusPath::Chat {
                target: Some("general".to_string()),
                is_channel: true
            })
        );
    }

    #[test]
    fn test_parse_chat_pm() {
        let uri = parse("nexus://example.com/chat/alice").unwrap();
        assert_eq!(uri.host, "example.com");
        assert_eq!(
            uri.path,
            Some(NexusPath::Chat {
                target: Some("alice".to_string()),
                is_channel: false
            })
        );
    }

    #[test]
    fn test_parse_chat_no_target() {
        let uri = parse("nexus://example.com/chat").unwrap();
        assert_eq!(uri.host, "example.com");
        assert_eq!(
            uri.path,
            Some(NexusPath::Chat {
                target: None,
                is_channel: false
            })
        );

        // With trailing slash
        let uri = parse("nexus://example.com/chat/").unwrap();
        assert_eq!(
            uri.path,
            Some(NexusPath::Chat {
                target: None,
                is_channel: false
            })
        );
    }

    #[test]
    fn test_parse_files() {
        let uri = parse("nexus://example.com/files/Music/song.mp3").unwrap();
        assert_eq!(uri.host, "example.com");
        assert_eq!(
            uri.path,
            Some(NexusPath::Files {
                path: "Music/song.mp3".to_string()
            })
        );

        // Empty files path (root)
        let uri = parse("nexus://example.com/files/").unwrap();
        assert_eq!(
            uri.path,
            Some(NexusPath::Files {
                path: "".to_string()
            })
        );

        // /files alone (no trailing slash)
        let uri = parse("nexus://example.com/files").unwrap();
        assert_eq!(
            uri.path,
            Some(NexusPath::Files {
                path: "".to_string()
            })
        );
    }

    #[test]
    fn test_parse_news() {
        let uri = parse("nexus://example.com/news").unwrap();
        assert_eq!(uri.path, Some(NexusPath::News));
    }

    #[test]
    fn test_parse_info() {
        let uri = parse("nexus://example.com/info").unwrap();
        assert_eq!(uri.path, Some(NexusPath::Info));
    }

    #[test]
    fn test_parse_url_encoded() {
        let uri = parse("nexus://user%40example@example.com").unwrap();
        assert_eq!(uri.user, Some("user@example".to_string()));

        let uri = parse("nexus://user:pass%3Aword@example.com").unwrap();
        assert_eq!(uri.password, Some("pass:word".to_string()));

        let uri = parse("nexus://example.com/chat/%23channel").unwrap();
        assert_eq!(
            uri.path,
            Some(NexusPath::Chat {
                target: Some("channel".to_string()),
                is_channel: true
            })
        );

        // UTF-8 encoded characters (é = %C3%A9 in UTF-8)
        let uri = parse("nexus://caf%C3%A9@example.com").unwrap();
        assert_eq!(uri.user, Some("café".to_string()));

        // UTF-8 in path
        let uri = parse("nexus://example.com/files/M%C3%BAsica").unwrap();
        assert_eq!(
            uri.path,
            Some(NexusPath::Files {
                path: "Música".to_string()
            })
        );
    }

    #[test]
    fn test_parse_case_insensitive_path() {
        let uri = parse("nexus://example.com/CHAT/#General").unwrap();
        assert_eq!(
            uri.path,
            Some(NexusPath::Chat {
                target: Some("General".to_string()),
                is_channel: true
            })
        );

        let uri = parse("nexus://example.com/NEWS").unwrap();
        assert_eq!(uri.path, Some(NexusPath::News));
    }

    #[test]
    fn test_parse_errors() {
        assert_eq!(parse("http://example.com"), Err(ParseError::InvalidScheme));
        assert_eq!(parse("nexus://"), Err(ParseError::MissingHost));
        assert_eq!(parse("nexus://:8500"), Err(ParseError::MissingHost));
        assert_eq!(
            parse("nexus://example.com:notaport"),
            Err(ParseError::InvalidPort)
        );
    }

    #[test]
    fn test_display() {
        let uri = NexusUri {
            user: None,
            password: None,
            host: "example.com".to_string(),
            port: DEFAULT_PORT,
            path: None,
        };
        assert_eq!(uri.to_string(), "nexus://example.com");

        let uri = NexusUri {
            user: Some("alice".to_string()),
            password: Some("secret".to_string()),
            host: "example.com".to_string(),
            port: 8500,
            path: Some(NexusPath::Chat {
                target: Some("general".to_string()),
                is_channel: true,
            }),
        };
        assert_eq!(
            uri.to_string(),
            "nexus://alice:secret@example.com:8500/chat/#general"
        );

        let uri = NexusUri {
            user: None,
            password: None,
            host: "::1".to_string(),
            port: 8500,
            path: None,
        };
        assert_eq!(uri.to_string(), "nexus://[::1]:8500");

        // Special characters in credentials get encoded
        let uri = NexusUri {
            user: Some("user@domain".to_string()),
            password: Some("pass:word".to_string()),
            host: "example.com".to_string(),
            port: DEFAULT_PORT,
            path: None,
        };
        assert_eq!(
            uri.to_string(),
            "nexus://user%40domain:pass%3Aword@example.com"
        );

        // Round-trip test: parse → display → parse should give same result
        let original = parse("nexus://user%40domain:pass%3Aword@example.com").unwrap();
        let displayed = original.to_string();
        let reparsed = parse(&displayed).unwrap();
        assert_eq!(original, reparsed);
    }

    #[test]
    fn test_is_nexus_uri() {
        assert!(is_nexus_uri("nexus://example.com"));
        assert!(is_nexus_uri("nexus://example.com/chat/#general"));
        assert!(!is_nexus_uri("http://example.com"));
        assert!(!is_nexus_uri("https://example.com"));
        assert!(!is_nexus_uri("example.com"));
    }

    #[test]
    fn test_has_credentials() {
        let uri = parse("nexus://example.com").unwrap();
        assert!(!uri.has_credentials());

        let uri = parse("nexus://alice@example.com").unwrap();
        assert!(uri.has_credentials());

        let uri = parse("nexus://alice:pass@example.com").unwrap();
        assert!(uri.has_credentials());
    }

    #[test]
    fn test_full_uri() {
        let uri = parse("nexus://shared_acct:sharedpass@example.com:8500/chat/#lobby").unwrap();
        assert_eq!(uri.user, Some("shared_acct".to_string()));
        assert_eq!(uri.password, Some("sharedpass".to_string()));
        assert_eq!(uri.host, "example.com");
        assert_eq!(uri.port, 8500);
        assert_eq!(
            uri.path,
            Some(NexusPath::Chat {
                target: Some("lobby".to_string()),
                is_channel: true
            })
        );
    }

    #[test]
    fn test_url_encode_path_spaces() {
        assert_eq!(url_encode_path("hello world"), "hello%20world");
        assert_eq!(
            url_encode_path("path/to/my file.txt"),
            "path/to/my%20file.txt"
        );
    }

    #[test]
    fn test_url_encode_path_unicode() {
        assert_eq!(url_encode_path("Música"), "M%C3%BAsica");
        assert_eq!(url_encode_path("日本語"), "%E6%97%A5%E6%9C%AC%E8%AA%9E");
        assert_eq!(url_encode_path("café"), "caf%C3%A9");
    }

    #[test]
    fn test_url_encode_path_special_chars() {
        // These should be encoded
        assert_eq!(url_encode_path("file&name"), "file%26name");
        assert_eq!(url_encode_path("file#name"), "file%23name");
        assert_eq!(url_encode_path("file?name"), "file%3Fname");
        assert_eq!(url_encode_path("file=name"), "file%3Dname");
        assert_eq!(url_encode_path("file@name"), "file%40name");
        assert_eq!(url_encode_path("100%done"), "100%25done");
    }

    #[test]
    fn test_url_encode_path_preserves_slash() {
        assert_eq!(url_encode_path("path/to/file"), "path/to/file");
        assert_eq!(url_encode_path("/root/path/"), "/root/path/");
    }

    #[test]
    fn test_url_encode_path_unreserved_chars() {
        // Unreserved chars should NOT be encoded (RFC 3986)
        assert_eq!(url_encode_path("file-name"), "file-name");
        assert_eq!(url_encode_path("file.name"), "file.name");
        assert_eq!(url_encode_path("file_name"), "file_name");
        assert_eq!(url_encode_path("file~name"), "file~name");
    }

    #[test]
    fn test_url_decode_roundtrip() {
        let original = "Shared/Music/Café Songs/日本語 file.mp3";
        let encoded = url_encode_path(original);
        let decoded = url_decode(&encoded);
        assert_eq!(decoded, original);
    }

    #[test]
    fn test_ipv6_uri_roundtrip() {
        // Bracketed IPv6 parses correctly and displays with brackets
        let uri = parse("nexus://[::1]:8500/files/test").unwrap();
        assert_eq!(uri.host, "::1");
        assert_eq!(uri.to_string(), "nexus://[::1]:8500/files/test");

        // Yggdrasil-style unbracketed IPv6 also works
        let uri = parse("nexus://[202:e7f:a50e:d03b:e13e:75f1:24c9:58bc]/files/Music").unwrap();
        assert_eq!(uri.host, "202:e7f:a50e:d03b:e13e:75f1:24c9:58bc");
        assert_eq!(
            uri.to_string(),
            "nexus://[202:e7f:a50e:d03b:e13e:75f1:24c9:58bc]/files/Music"
        );
    }

    // =========================================================================
    // Comprehensive URI segment tests
    // =========================================================================

    #[test]
    fn test_userinfo_utf8_encoding() {
        // UTF-8 username and password should encode and decode correctly
        let uri = NexusUri {
            user: Some("用户".to_string()),     // Chinese "user"
            password: Some("密码".to_string()), // Chinese "password"
            host: "example.com".to_string(),
            port: DEFAULT_PORT,
            path: None,
        };
        let encoded = uri.to_string();
        // Verify it encodes (contains percent-encoded bytes)
        assert!(encoded.contains('%'));

        // Parse it back
        let parsed = parse(&encoded).unwrap();
        assert_eq!(parsed.user, Some("用户".to_string()));
        assert_eq!(parsed.password, Some("密码".to_string()));
    }

    #[test]
    fn test_userinfo_special_chars() {
        // Special chars that must be encoded in userinfo: : @ /
        let uri = NexusUri {
            user: Some("user@domain".to_string()),
            password: Some("pass:word/slash".to_string()),
            host: "example.com".to_string(),
            port: DEFAULT_PORT,
            path: None,
        };
        let encoded = uri.to_string();

        // @ : / should be encoded
        assert!(encoded.contains("%40")); // @
        assert!(encoded.contains("%3A")); // :
        assert!(encoded.contains("%2F")); // /

        // Parse it back
        let parsed = parse(&encoded).unwrap();
        assert_eq!(parsed.user, Some("user@domain".to_string()));
        assert_eq!(parsed.password, Some("pass:word/slash".to_string()));
    }

    #[test]
    fn test_userinfo_unreserved_chars() {
        // Unreserved chars should NOT be encoded: - . _ ~
        let uri = NexusUri {
            user: Some("user-name.test_account~1".to_string()),
            password: Some("pass-word.test_123~".to_string()),
            host: "example.com".to_string(),
            port: DEFAULT_PORT,
            path: None,
        };
        let encoded = uri.to_string();

        // These should appear literally, not encoded
        assert!(encoded.contains("user-name.test_account~1"));
        assert!(encoded.contains("pass-word.test_123~"));
    }

    #[test]
    fn test_path_utf8_encoding() {
        // UTF-8 path components
        let uri =
            parse("nexus://example.com/files/M%C3%BAsica/%E6%97%A5%E6%9C%AC%E8%AA%9E").unwrap();
        if let Some(NexusPath::Files { path }) = uri.path {
            assert_eq!(path, "Música/日本語");
        } else {
            panic!("Expected Files path");
        }
    }

    #[test]
    fn test_path_special_chars() {
        // Path with special chars that need encoding
        let original = "Music/Artist & Band/Song #1.mp3";
        let encoded = url_encode_path(original);
        // & and # should be encoded, / should not
        assert!(encoded.contains("%26")); // &
        assert!(encoded.contains("%23")); // #
        assert!(encoded.contains("/")); // / preserved
        assert!(!encoded.contains("%2F")); // / not encoded

        let decoded = url_decode(&encoded);
        assert_eq!(decoded, original);
    }

    #[test]
    fn test_host_not_encoded() {
        // Host should not have percent encoding applied
        let uri = parse("nexus://example.com/files/test").unwrap();
        assert_eq!(uri.host, "example.com");

        // IPv4 with dots
        let uri = parse("nexus://192.168.1.1/files/test").unwrap();
        assert_eq!(uri.host, "192.168.1.1");
    }

    #[test]
    fn test_full_uri_with_all_segments() {
        // Complete URI with all segments including UTF-8
        let uri = NexusUri {
            user: Some("用户".to_string()),
            password: Some("密码".to_string()),
            host: "::1".to_string(),
            port: 8500,
            path: Some(NexusPath::Files {
                path: "Música/日本語 file.mp3".to_string(),
            }),
        };

        let encoded = uri.to_string();
        let parsed = parse(&encoded).unwrap();

        assert_eq!(parsed.user, Some("用户".to_string()));
        assert_eq!(parsed.password, Some("密码".to_string()));
        assert_eq!(parsed.host, "::1");
        assert_eq!(parsed.port, 8500);
        if let Some(NexusPath::Files { path }) = parsed.path {
            assert_eq!(path, "Música/日本語 file.mp3");
        } else {
            panic!("Expected Files path");
        }
    }
}
