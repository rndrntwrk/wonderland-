use wonderland_game_services::*;

#[derive(Clone)]
pub struct UpstreamClient {
    client: reqwest::Client,
    routes: ApiRoutes,
}
impl UpstreamClient {
    pub fn new(base: &str) -> ServiceResult<Self> {
        let routes = ApiRoutes::new(base)?;
        let loopback = matches!(
            routes.base().host_str(),
            Some("localhost" | "127.0.0.1" | "[::1]")
        );
        if routes.base().scheme() != "https" && !loopback {
            return Err(ServiceError::new(
                ErrorCode::InvalidRequest,
                "Remote account services require HTTPS",
            ));
        }
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(std::time::Duration::from_secs(5))
            .timeout(std::time::Duration::from_secs(20))
            .user_agent("WonderlandBrowserGateway/0.1")
            .build()
            .map_err(|_| transport())?;
        Ok(Self { client, routes })
    }
    pub async fn authenticate(&self, username: &str, password: &str) -> ServiceResult<OAuthGrant> {
        let body = oauth_form(username, password)?;
        let request = self
            .client
            .post(self.routes.oauth())
            .header(
                reqwest::header::CONTENT_TYPE,
                "application/x-www-form-urlencoded",
            )
            .body(body);
        let (status, body) = read(request).await?;
        parse_oauth(status, &body)
    }
    pub async fn roster(&self, token: &SecretString) -> ServiceResult<Vec<RosterEntry>> {
        let (status, body) = read(
            self.client
                .get(self.routes.roster())
                .bearer_auth(token.expose()),
        )
        .await?;
        parse_roster(status, &body)
    }
    pub async fn shards(&self) -> ServiceResult<Vec<Shard>> {
        let (status, body) = read(self.client.get(self.routes.shards())).await?;
        parse_shards(status, &body)
    }
    pub async fn select_city(
        &self,
        token: &SecretString,
        shard: &str,
        avatar: u32,
    ) -> ServiceResult<CitySelection> {
        let (status, body) = read(
            self.client
                .get(self.routes.selection(shard, avatar)?)
                .bearer_auth(token.expose()),
        )
        .await?;
        parse_city_selection(status, &body)
    }
    pub async fn query(&self, query: DirectoryQuery) -> ServiceResult<DirectoryResult> {
        let (status, body) = read(self.client.get(self.routes.query(&query)?)).await?;
        check_http(status, &body)?;
        let data: serde_json::Value = serde_json::from_slice(&body).map_err(|_| {
            ServiceError::new(
                ErrorCode::InvalidResponse,
                "The original directory returned invalid JSON",
            )
        })?;
        if !data.is_object() && !data.is_array() {
            return Err(ServiceError::new(
                ErrorCode::InvalidResponse,
                "The original directory returned an invalid result",
            ));
        }
        Ok(DirectoryResult { query, data })
    }
}

async fn read(request: reqwest::RequestBuilder) -> ServiceResult<(u16, Vec<u8>)> {
    let mut response = request.send().await.map_err(|_| transport())?;
    let status = response.status().as_u16();
    if response
        .content_length()
        .is_some_and(|n| n > MAX_HTTP_BODY as u64)
    {
        return Err(too_large());
    }
    let mut body = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|_| transport())? {
        if body
            .len()
            .checked_add(chunk.len())
            .is_none_or(|len| len > MAX_HTTP_BODY)
        {
            return Err(too_large());
        }
        body.extend_from_slice(&chunk);
    }
    Ok((status, body))
}
fn transport() -> ServiceError {
    ServiceError::new(
        ErrorCode::Transport,
        "The original account service could not be reached",
    )
}
fn too_large() -> ServiceError {
    ServiceError::new(
        ErrorCode::ResponseTooLarge,
        "The original service response exceeds the transport byte budget",
    )
}
