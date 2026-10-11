//! Typed original HTTP routes. A query cannot supply an arbitrary upstream URL.
use crate::{ErrorCode, ServiceError, ServiceResult};
use serde::{Deserialize, Serialize};
use url::Url;

#[derive(Clone, Debug)]
pub struct ApiRoutes {
    base: Url,
}
impl ApiRoutes {
    pub fn new(base: &str) -> ServiceResult<Self> {
        let mut base = Url::parse(base)
            .map_err(|_| ServiceError::new(ErrorCode::InvalidRequest, "Invalid service URL"))?;
        if !matches!(base.scheme(), "http" | "https")
            || base.host_str().is_none()
            || !base.username().is_empty()
            || base.password().is_some()
            || base.query().is_some()
            || base.fragment().is_some()
        {
            return Err(ServiceError::new(
                ErrorCode::InvalidRequest,
                "Service URL must be HTTP(S), without credentials, query or fragment",
            ));
        }
        if !base.path().ends_with('/') {
            base.set_path(&format!("{}/", base.path()));
        }
        Ok(Self { base })
    }
    fn route(&self, segments: &[String]) -> Url {
        let mut url = self.base.clone();
        url.path_segments_mut()
            .expect("validated hierarchical HTTP URL")
            .pop_if_empty()
            .extend(segments);
        url
    }
    fn path(&self, path: &str) -> Url {
        self.route(&path.split('/').map(str::to_owned).collect::<Vec<_>>())
    }
    pub fn oauth(&self) -> Url {
        self.path("userapi/oauth/token")
    }
    pub fn roster(&self) -> Url {
        self.path("cityselector/app/AvatarDataServlet")
    }
    pub fn shards(&self) -> Url {
        self.path("cityselector/shard-status.jsp")
    }
    pub fn selection(&self, shard: &str, avatar_id: u32) -> ServiceResult<Url> {
        bounded_text(shard)?;
        let mut url = self.path("cityselector/app/ShardSelectorServlet");
        url.query_pairs_mut()
            .append_pair("shardName", shard)
            .append_pair("avatarId", &avatar_id.to_string());
        Ok(url)
    }
    pub fn query(&self, query: &DirectoryQuery) -> ServiceResult<Url> {
        use DirectoryQuery::*;
        let mut pairs: Vec<(&str, String)> = Vec::new();
        let segments = match query {
            Avatar { avatar_id } => vec!["userapi".into(), "avatars".into(), avatar_id.to_string()],
            Avatars { ids } => {
                pairs.push(("ids", id_list(ids)?));
                vec!["userapi".into(), "avatars".into()]
            }
            AvatarPage {
                shard_id,
                page,
                per_page,
            } => {
                page_valid(*page, *per_page)?;
                pairs.push(("avatars_on_page", per_page.to_string()));
                city(
                    *shard_id,
                    &["avatars".into(), "page".into(), page.to_string()],
                )
            }
            AvatarSearch { shard_id, name } => {
                bounded_text(name)?;
                city(*shard_id, &["avatars".into(), "name".into(), name.clone()])
            }
            NeighborhoodAvatars {
                shard_id,
                neighborhood_id,
            } => city(
                *shard_id,
                &[
                    "avatars".into(),
                    "neighborhood".into(),
                    neighborhood_id.to_string(),
                ],
            ),
            OnlineAvatars { compact } => {
                pairs.push(("compact", compact.to_string()));
                vec!["userapi".into(), "avatars".into(), "online".into()]
            }
            Lot { lot_id } => vec!["userapi".into(), "lots".into(), lot_id.to_string()],
            Lots { ids } => {
                pairs.push(("ids", id_list(ids)?));
                vec!["userapi".into(), "lots".into()]
            }
            LotPage {
                shard_id,
                page,
                per_page,
            } => {
                page_valid(*page, *per_page)?;
                pairs.push(("lots_on_page", per_page.to_string()));
                city(*shard_id, &["lots".into(), "page".into(), page.to_string()])
            }
            LotSearch { shard_id, name } => {
                bounded_text(name)?;
                city(*shard_id, &["lots".into(), "name".into(), name.clone()])
            }
            LotByLocation { shard_id, location } => city(
                *shard_id,
                &["lots".into(), "location".into(), location.to_string()],
            ),
            NeighborhoodLots {
                shard_id,
                neighborhood_id,
            } => city(
                *shard_id,
                &[
                    "lots".into(),
                    "neighborhood".into(),
                    neighborhood_id.to_string(),
                ],
            ),
            OnlineLots { shard_id } => city(*shard_id, &["lots".into(), "online".into()]),
            TopLots { shard_id, category } => {
                let mut p = vec!["lots".into(), "top100".into()];
                if let Some(category) = category {
                    bounded_text(category)?;
                    p.extend(["category".into(), category.clone()]);
                } else {
                    p.push("all".into());
                }
                city(*shard_id, &p)
            }
            Neighborhoods { shard_id } => city(*shard_id, &["neighborhoods".into(), "all".into()]),
            Neighborhood { neighborhood_id } => vec![
                "userapi".into(),
                "neighborhoods".into(),
                neighborhood_id.to_string(),
            ],
            NeighborhoodSearch { shard_id, name } => {
                bounded_text(name)?;
                city(
                    *shard_id,
                    &["neighborhoods".into(), "name".into(), name.clone()],
                )
            }
            Elections { neighborhood_id } => neighborhood(*neighborhood_id, &["elections".into()]),
            Bulletins { neighborhood_id } => neighborhood(*neighborhood_id, &["bulletins".into()]),
            Bulletin {
                neighborhood_id,
                bulletin_id,
            } => neighborhood(
                *neighborhood_id,
                &["bulletins".into(), bulletin_id.to_string()],
            ),
            BulletinsByType {
                neighborhood_id,
                bulletin_type,
            } => {
                bounded_text(bulletin_type)?;
                neighborhood(
                    *neighborhood_id,
                    &["bulletins".into(), "type".into(), bulletin_type.clone()],
                )
            }
        };
        let mut url = self.route(&segments);
        if !pairs.is_empty() {
            url.query_pairs_mut().extend_pairs(pairs);
        }
        Ok(url)
    }
    pub fn base(&self) -> &Url {
        &self.base
    }
}

pub fn oauth_form(username: &str, password: &str) -> ServiceResult<String> {
    if username.is_empty()
        || password.is_empty()
        || username.len() > 4096
        || password.len() > 16_384
    {
        return Err(ServiceError::new(
            ErrorCode::InvalidRequest,
            "Credentials are empty or exceed the transport byte budget",
        ));
    }
    Ok(url::form_urlencoded::Serializer::new(String::new())
        .append_pair("username", username)
        .append_pair("password", password)
        .append_pair("permission_level", "1")
        .finish())
}

fn bounded_text(text: &str) -> ServiceResult<()> {
    if text.is_empty()
        || text.len() > 4096
        || text == "."
        || text == ".."
        || text.chars().any(char::is_control)
    {
        return Err(ServiceError::new(
            ErrorCode::InvalidRequest,
            "Query text is empty or exceeds the transport budget",
        ));
    }
    Ok(())
}
fn page_valid(page: u32, per_page: u16) -> ServiceResult<()> {
    if page == 0 || !(1..=500).contains(&per_page) {
        return Err(ServiceError::new(
            ErrorCode::InvalidRequest,
            "Original directory pages start at 1 and contain 1–500 entries per response",
        ));
    }
    Ok(())
}
fn id_list(ids: &[u32]) -> ServiceResult<String> {
    if ids.is_empty() || ids.len() > 1024 {
        return Err(ServiceError::new(
            ErrorCode::InvalidRequest,
            "Split this query into smaller identifier batches",
        ));
    }
    Ok(ids.iter().map(u32::to_string).collect::<Vec<_>>().join(","))
}
fn city(shard: u32, tail: &[String]) -> Vec<String> {
    let mut p = vec!["userapi".into(), "city".into(), shard.to_string()];
    p.extend_from_slice(tail);
    p
}
fn neighborhood(id: u32, tail: &[String]) -> Vec<String> {
    let mut p = vec!["userapi".into(), "neighborhood".into(), id.to_string()];
    p.extend_from_slice(tail);
    p
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum DirectoryQuery {
    Avatar {
        avatar_id: u32,
    },
    Avatars {
        ids: Vec<u32>,
    },
    AvatarPage {
        shard_id: u32,
        page: u32,
        per_page: u16,
    },
    AvatarSearch {
        shard_id: u32,
        name: String,
    },
    NeighborhoodAvatars {
        shard_id: u32,
        neighborhood_id: u32,
    },
    OnlineAvatars {
        #[serde(default)]
        compact: bool,
    },
    Lot {
        lot_id: u32,
    },
    Lots {
        ids: Vec<u32>,
    },
    LotPage {
        shard_id: u32,
        page: u32,
        per_page: u16,
    },
    LotSearch {
        shard_id: u32,
        name: String,
    },
    LotByLocation {
        shard_id: u32,
        location: u32,
    },
    NeighborhoodLots {
        shard_id: u32,
        neighborhood_id: u32,
    },
    OnlineLots {
        shard_id: u32,
    },
    TopLots {
        shard_id: u32,
        category: Option<String>,
    },
    Neighborhoods {
        shard_id: u32,
    },
    Neighborhood {
        neighborhood_id: u32,
    },
    NeighborhoodSearch {
        shard_id: u32,
        name: String,
    },
    Elections {
        neighborhood_id: u32,
    },
    Bulletins {
        neighborhood_id: u32,
    },
    Bulletin {
        neighborhood_id: u32,
        bulletin_id: u32,
    },
    BulletinsByType {
        neighborhood_id: u32,
        bulletin_type: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DirectoryRequest {
    pub query: DirectoryQuery,
}

/// Original fields, pagination and totals are retained. Per-response safety bounds never truncate data.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DirectoryResult {
    pub query: DirectoryQuery,
    pub data: serde_json::Value,
}
