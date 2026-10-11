use crate::{ErrorCode, ServiceError, ServiceResult};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DecimalU64(pub u64);

impl Serialize for DecimalU64 {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0.to_string())
    }
}
impl<'de> Deserialize<'de> for DecimalU64 {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        text.parse().map(Self).map_err(serde::de::Error::custom)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RosterHome {
    pub lot_id: u32,
    pub location: u32,
    pub name: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RosterEntry {
    pub avatar_id: u32,
    pub shard_name: String,
    pub name: String,
    pub description: String,
    pub head_key: Option<DecimalU64>,
    pub body_key: Option<DecimalU64>,
    pub appearance: Option<String>,
    pub home: Option<RosterHome>,
    pub money: Option<i64>,
    pub motives: Option<[i16; 8]>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Shard {
    pub id: u32,
    pub name: String,
    pub rank: i32,
    pub map: String,
    pub status: String,
}

/// Secrets intentionally do not implement Serialize or expose their contents through Debug.
#[derive(Clone)]
pub struct SecretString(String);
impl SecretString {
    pub fn new(value: String) -> Self {
        Self(value)
    }
    pub fn expose(&self) -> &str {
        &self.0
    }
}
impl std::fmt::Debug for SecretString {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("[redacted]")
    }
}

#[derive(Clone, Debug)]
pub struct OAuthGrant {
    pub token: SecretString,
    pub expires_in: u32,
}
#[derive(Clone, Debug)]
pub struct CitySelection {
    pub address: String,
    pub ticket: SecretString,
    pub player_id: u32,
    pub avatar_id: u32,
}

/// Transport safety quota, not a limit on account identities. Oversized responses fail whole.
pub const MAX_HTTP_BODY: usize = 8 * 1024 * 1024;

pub fn check_http(status: u16, body: &[u8]) -> ServiceResult<()> {
    if body.len() > MAX_HTTP_BODY {
        return Err(ServiceError::new(
            ErrorCode::ResponseTooLarge,
            "Service response exceeds the transport byte budget",
        ));
    }
    if matches!(status, 401 | 403) {
        return Err(ServiceError::new(
            ErrorCode::Unauthorized,
            "The service did not authorize this request",
        ));
    }
    if !(200..300).contains(&status) {
        return Err(ServiceError::new(
            ErrorCode::Transport,
            "The service returned an unsuccessful HTTP status",
        ));
    }
    Ok(())
}

fn invalid() -> ServiceError {
    ServiceError::new(
        ErrorCode::InvalidResponse,
        "The service returned an invalid response",
    )
}

pub fn parse_oauth(status: u16, body: &[u8]) -> ServiceResult<OAuthGrant> {
    check_http(status, body)?;
    #[derive(Deserialize)]
    struct Response {
        access_token: Option<String>,
        expires_in: Option<u32>,
        error: Option<String>,
        error_description: Option<String>,
    }
    let response: Response = serde_json::from_slice(body).map_err(|_| invalid())?;
    if response.error.is_some() {
        return Err(
            if response.error_description.as_deref() == Some("account_locked") {
                ServiceError::new(
                    ErrorCode::AccountLocked,
                    "The account is temporarily locked by the service",
                )
            } else {
                ServiceError::new(
                    ErrorCode::AuthenticationFailed,
                    "The service did not accept these credentials",
                )
            },
        );
    }
    let token = response
        .access_token
        .filter(|x| !x.is_empty() && x.len() <= 16_384 && x.bytes().all(|b| b.is_ascii_graphic()))
        .ok_or_else(invalid)?;
    let expires_in = response.expires_in.filter(|x| *x > 0).ok_or_else(invalid)?;
    Ok(OAuthGrant {
        token: SecretString::new(token),
        expires_in,
    })
}

/// Validate root, nesting, complete document and DTD rejection before serde decoding.
fn xml_root(body: &[u8]) -> ServiceResult<String> {
    use quick_xml::{Reader, events::Event};
    let mut reader = Reader::from_reader(body);
    let mut depth = 0usize;
    let mut root = None;
    let mut closed = false;
    loop {
        match reader.read_event().map_err(|_| invalid())? {
            Event::Start(e) => {
                if closed {
                    return Err(invalid());
                }
                if depth == 0 {
                    root =
                        Some(String::from_utf8(e.name().as_ref().to_vec()).map_err(|_| invalid())?);
                }
                depth += 1;
                if depth > 16 {
                    return Err(invalid());
                }
            }
            Event::Empty(e) => {
                if closed {
                    return Err(invalid());
                }
                if depth == 0 {
                    root =
                        Some(String::from_utf8(e.name().as_ref().to_vec()).map_err(|_| invalid())?);
                    closed = true;
                }
            }
            Event::End(_) => {
                depth = depth.checked_sub(1).ok_or_else(invalid)?;
                if depth == 0 {
                    closed = true;
                }
            }
            Event::Text(e) => {
                if depth == 0 && e.as_ref().iter().any(|b| !b.is_ascii_whitespace()) {
                    return Err(invalid());
                }
            }
            Event::DocType(_) | Event::CData(_) if depth == 0 => return Err(invalid()),
            Event::DocType(_) => return Err(invalid()),
            Event::Eof => break,
            _ => {}
        }
    }
    if depth != 0 || !closed {
        return Err(invalid());
    }
    root.ok_or_else(invalid)
}

fn parse_xml<T: serde::de::DeserializeOwned>(
    status: u16,
    body: &[u8],
    root: &str,
) -> ServiceResult<T> {
    check_http(status, body)?;
    let found = xml_root(body)?;
    if found == "Error-Message" {
        return Err(ServiceError::new(
            ErrorCode::Rejected,
            "The original service rejected this request",
        ));
    }
    if found != root {
        return Err(invalid());
    }
    quick_xml::de::from_reader(body).map_err(|_| invalid())
}

pub fn parse_roster(status: u16, body: &[u8]) -> ServiceResult<Vec<RosterEntry>> {
    #[derive(Deserialize)]
    struct List {
        #[serde(rename = "Avatar-Data", default)]
        avatars: Vec<AvatarXml>,
    }
    #[derive(Deserialize)]
    struct AvatarXml {
        #[serde(rename = "AvatarID")]
        id: u32,
        #[serde(rename = "Name")]
        name: String,
        #[serde(rename = "Shard-Name")]
        shard: String,
        #[serde(rename = "Head")]
        head: Option<DecimalU64>,
        #[serde(rename = "Body")]
        body: Option<DecimalU64>,
        #[serde(rename = "Appearance")]
        appearance: Option<String>,
        #[serde(rename = "Description", default)]
        description: String,
        #[serde(rename = "LotId")]
        lot_id: Option<u32>,
        #[serde(rename = "LotName")]
        lot_name: Option<String>,
        #[serde(rename = "LotLocation")]
        location: Option<u32>,
    }
    let list: List = parse_xml(status, body, "The-Sims-Online")?;
    let mut seen = std::collections::HashSet::new();
    list.avatars
        .into_iter()
        .map(|a| {
            if a.id == 0 || a.name.is_empty() || a.shard.is_empty() || !seen.insert(a.id) {
                return Err(invalid());
            }
            let home = match (a.lot_id, a.location, a.lot_name) {
                (None, None, None) => None,
                (Some(lot_id), Some(location), Some(name)) => Some(RosterHome {
                    lot_id,
                    location,
                    name,
                }),
                _ => return Err(invalid()),
            };
            Ok(RosterEntry {
                avatar_id: a.id,
                shard_name: a.shard,
                name: a.name,
                description: a.description,
                head_key: a.head,
                body_key: a.body,
                appearance: a.appearance,
                home,
                money: None,
                motives: None,
            })
        })
        .collect()
}

pub fn parse_shards(status: u16, body: &[u8]) -> ServiceResult<Vec<Shard>> {
    #[derive(Deserialize)]
    struct List {
        #[serde(rename = "Shard-Status", default)]
        shards: Vec<ShardXml>,
    }
    #[derive(Deserialize)]
    struct ShardXml {
        #[serde(rename = "Id")]
        id: u32,
        #[serde(rename = "Name")]
        name: String,
        #[serde(rename = "Rank")]
        rank: i32,
        #[serde(rename = "Map")]
        map: String,
        #[serde(rename = "Status")]
        status: String,
    }
    let list: List = parse_xml(status, body, "Shard-Status-List")?;
    let mut seen = std::collections::HashSet::new();
    list.shards
        .into_iter()
        .map(|s| {
            if s.id == 0
                || s.name.is_empty()
                || !seen.insert(s.id)
                || !matches!(
                    s.status.as_str(),
                    "Up" | "Down" | "Busy" | "Full" | "Closed" | "Frontier"
                )
            {
                return Err(invalid());
            }
            Ok(Shard {
                id: s.id,
                name: s.name,
                rank: s.rank,
                map: s.map,
                status: s.status,
            })
        })
        .collect()
}

pub fn parse_city_selection(status: u16, body: &[u8]) -> ServiceResult<CitySelection> {
    #[derive(Deserialize)]
    struct Selection {
        #[serde(rename = "Connection-Address")]
        address: String,
        #[serde(rename = "Authorization-Ticket")]
        ticket: String,
        #[serde(rename = "PlayerID")]
        player_id: u32,
        #[serde(rename = "AvatarID")]
        avatar_id: u32,
    }
    let s: Selection = parse_xml(status, body, "Shard-Selection")?;
    if s.address.is_empty()
        || s.address.len() > 256
        || s.ticket.len() != 32
        || !s.ticket.bytes().all(|b| b.is_ascii_hexdigit())
        || s.player_id == 0
    {
        return Err(invalid());
    }
    Ok(CitySelection {
        address: s.address,
        ticket: SecretString::new(s.ticket),
        player_id: s.player_id,
        avatar_id: s.avatar_id,
    })
}
