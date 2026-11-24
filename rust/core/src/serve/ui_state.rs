//! The remembered UI state the server draws on the first paint (TAN-880).
//!
//! A browser keeps a few choices that change the look of a screen: which
//! folders of the notes tree are shut, and later the open tabs and the
//! Relations pane. They used to live in `localStorage`, which the server
//! cannot read, so the page painted its default and a script then rearranged
//! it. Each state now lives in one cookie, `tandem_<port>_<state>`, that the
//! browser sends with every request, so the server draws the final state at
//! once. The port is in the name because cookies ignore ports where
//! `localStorage` does not: two serves on one host must not share a state.
//!
//! A cookie value is input, never trusted: a damaged, oversized or foreign
//! value reads as absent, and a path in it that the vault does not hold is
//! dropped by the caller matching it against the vault's own rows.

use std::collections::HashSet;

use axum::http::{HeaderMap, HeaderValue, header};
use serde::{Deserialize, Serialize};

/// The most an encoded cookie value may hold. Browsers keep about 4096 bytes
/// of name and value together; a longer value reads as absent.
pub(super) const MAX_ENCODED: usize = 3800;

/// The name prefix of this serve's state cookies: `tandem_<port>_`.
pub(super) fn prefix(port: u16) -> String {
    format!("tandem_{port}_")
}

/// The decoded value of the `<prefix><state>` cookie, over every `Cookie`
/// header, or `None` when it is absent, oversized or not valid UTF-8.
pub(super) fn raw(headers: &HeaderMap, prefix: &str, state: &str) -> Option<String> {
    let name = format!("{prefix}{state}");
    let encoded = headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(';'))
        .filter_map(|pair| pair.trim().split_once('='))
        .find(|(cookie, _)| *cookie == name)
        .map(|(_, value)| value)?;
    if encoded.len() > MAX_ENCODED {
        return None;
    }
    percent_decode(encoded)
}

/// One state's value, parsed; any failure is the absent state.
pub(super) fn read<T: for<'de> Deserialize<'de>>(
    headers: &HeaderMap,
    prefix: &str,
    state: &str,
) -> Option<T> {
    serde_json::from_str(&raw(headers, prefix, state)?).ok()
}

/// The `Set-Cookie` line for a server-written state: the attributes the page's
/// own helper writes (a year, `Path=/`, `SameSite=Lax`), no `Secure` so a
/// plain-http LAN serve keeps it, no `HttpOnly` so the script can read it.
#[allow(dead_code)]
pub(super) fn set_cookie<T: Serialize>(
    prefix: &str,
    state: &str,
    value: &T,
) -> Option<HeaderValue> {
    let json = serde_json::to_string(value).ok()?;
    let encoded = percent_encode(&json);
    if encoded.len() > MAX_ENCODED {
        return None;
    }
    HeaderValue::from_str(&format!(
        "{prefix}{state}={encoded}; Path=/; Max-Age=31536000; SameSite=Lax"
    ))
    .ok()
}

fn percent_encode(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || b"-_.!~*'()".contains(&byte) {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

fn percent_decode(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut at = 0;
    while at < bytes.len() {
        if bytes[at] == b'%' {
            let pair = text.get(at + 1..at + 3)?;
            out.push(u8::from_str_radix(pair, 16).ok()?);
            at += 3;
        } else {
            out.push(bytes[at]);
            at += 1;
        }
    }
    String::from_utf8(out).ok()
}

/// Whether a tree cookie's folders are shut or open unless `except` names them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub(super) enum Fold {
    Shut,
    Open,
}

/// The `tree` state: `{"default":"shut"|"open","except":[folder paths]}`, plus
/// `open`, the note the page last revealed. A note that opens shows its
/// ancestors once; the same note drawn again keeps the stored layout exactly.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub(super) struct TreeState {
    pub default: Fold,
    #[serde(default)]
    pub except: Vec<String>,
    #[serde(default)]
    pub open: Option<String>,
}

/// The folder paths above a note path: `a/b/c.md` gives `a` and `a/b`.
pub(super) fn ancestors(path: &str) -> Vec<String> {
    let parts: Vec<&str> = path.split('/').collect();
    (1..parts.len()).map(|n| parts[..n].join("/")).collect()
}

/// The folders to draw shut, out of the vault's `folders`. Without a stored
/// state every folder is shut but the open note's ancestors; with one, the
/// stored layout, and a note that is not the last revealed one opens its
/// ancestors. A path the vault does not hold matches no folder and is dropped.
pub(super) fn shut_folders(
    state: Option<&TreeState>,
    folders: &[&str],
    open: Option<&str>,
) -> HashSet<String> {
    let above: Vec<String> = open.map(ancestors).unwrap_or_default();
    let mut shut: HashSet<String> = match state {
        None => folders.iter().map(|path| path.to_string()).collect(),
        Some(state) => {
            let named: HashSet<&str> = state.except.iter().map(String::as_str).collect();
            folders
                .iter()
                .filter(|path| named.contains(*path) == (state.default == Fold::Open))
                .map(|path| path.to_string())
                .collect()
        }
    };
    let reveal = match state {
        None => true,
        Some(state) => open.is_some() && state.open.as_deref() != open,
    };
    if reveal {
        for path in &above {
            shut.remove(path);
        }
    }
    shut
}

#[cfg(test)]
mod tests {
    use super::*;

    fn headers(cookies: &[&str]) -> HeaderMap {
        let mut map = HeaderMap::new();
        for cookie in cookies {
            map.append(header::COOKIE, HeaderValue::from_str(cookie).unwrap());
        }
        map
    }

    #[test]
    fn decodes_percent_escapes_and_utf8() {
        assert_eq!(percent_decode("a%2Fb%20%C3%A9").as_deref(), Some("a/b é"));
        assert_eq!(percent_decode("%ZZ"), None);
        assert_eq!(percent_decode("%C3"), None);
        assert_eq!(percent_decode("%2"), None);
    }

    #[test]
    fn round_trips_a_value() {
        let state = TreeState {
            default: Fold::Shut,
            except: vec!["é/b".to_string()],
            open: None,
        };
        let line = set_cookie("tandem_1_", "tree", &state).unwrap();
        let text = line.to_str().unwrap();
        assert!(text.contains("Path=/") && text.contains("SameSite=Lax"));
        assert!(!text.contains("Secure") && !text.contains("HttpOnly"));
        let pair = text.split(';').next().unwrap();
        let back: Option<TreeState> = read(&headers(&[pair]), "tandem_1_", "tree");
        assert_eq!(back, Some(state));
    }

    #[test]
    fn reads_across_several_cookie_headers() {
        let map = headers(&["a=1; b=2", "tandem_7_tree=%7B%22default%22%3A%22open%22%7D"]);
        let state: TreeState = read(&map, "tandem_7_", "tree").unwrap();
        assert_eq!(state.default, Fold::Open);
        assert!(state.except.is_empty());
    }

    #[test]
    fn another_ports_damaged_and_oversized_values_are_absent() {
        let good = "%7B%22default%22%3A%22open%22%7D";
        let other = headers(&[&format!("tandem_9_tree={good}")]);
        assert!(read::<TreeState>(&other, "tandem_7_", "tree").is_none());
        let damaged = headers(&["tandem_7_tree=%7Bnot-json"]);
        assert!(read::<TreeState>(&damaged, "tandem_7_", "tree").is_none());
        let big = format!("tandem_7_tree={}", "a".repeat(MAX_ENCODED + 1));
        assert!(raw(&headers(&[&big]), "tandem_7_", "tree").is_none());
    }

    #[test]
    fn no_state_shuts_all_but_the_open_notes_ancestors() {
        let folders = ["a", "a/b", "c"];
        let shut = shut_folders(None, &folders, Some("a/b/n.md"));
        assert_eq!(shut, HashSet::from(["c".to_string()]));
    }

    #[test]
    fn a_stored_state_is_kept_and_a_path_outside_the_vault_is_dropped() {
        let state = TreeState {
            default: Fold::Shut,
            except: vec!["a".to_string(), "../../etc".to_string()],
            open: Some("x.md".to_string()),
        };
        let folders = ["a", "a/b", "c"];
        let shut = shut_folders(Some(&state), &folders, Some("x.md"));
        assert_eq!(shut, HashSet::from(["a/b".to_string(), "c".to_string()]));
        let open_default = TreeState {
            default: Fold::Open,
            except: vec!["c".to_string(), "nope".to_string()],
            open: None,
        };
        let shut = shut_folders(Some(&open_default), &folders, None);
        assert_eq!(shut, HashSet::from(["c".to_string()]));
    }

    #[test]
    fn a_new_note_opens_its_ancestors_once() {
        let state = TreeState {
            default: Fold::Shut,
            except: vec![],
            open: Some("old.md".to_string()),
        };
        let folders = ["a", "a/b"];
        let shut = shut_folders(Some(&state), &folders, Some("a/b/n.md"));
        assert!(shut.is_empty());
        let same = TreeState {
            open: Some("a/b/n.md".to_string()),
            ..state
        };
        let shut = shut_folders(Some(&same), &folders, Some("a/b/n.md"));
        assert_eq!(shut.len(), 2);
    }
}
