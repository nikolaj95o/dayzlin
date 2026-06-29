use crate::error::Error;
use crate::servers::{parse_servers, Server};
use crate::DAYZ_API;

pub async fn fetch_servers(client: &reqwest::Client) -> Result<Vec<Server>, Error> {
    let url = format!("{DAYZ_API}/launcher/servers/dayz");
    let resp = client
        .get(&url)
        .header("User-Agent", "dayzlin")
        .send()
        .await
        .map_err(|e| Error::Network(e.to_string()))?;
    let text = resp
        .text()
        .await
        .map_err(|e| Error::Network(e.to_string()))?;
    parse_servers(&text)
}
