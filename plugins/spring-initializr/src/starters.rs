//! Bounded explicit server metadata requests. Existing-project writes use add_starters.
use openidea_plugin::{Error, HttpRequest, host};
use serde_json::Value;
const ACCEPT: &str = "application/vnd.initializr.v2.2+json";

pub(crate) fn get(path: &str) -> Result<Value, Error> {
    let url = format!("{}{path}", crate::START);
    let response = host::fetch(&HttpRequest::get(&url).header("Accept", ACCEPT))?;
    if response.status != 200 {
        return Err(Error::Failed(format!("{url} answered {}", response.status)));
    }

    if response.body.len() > crate::session::MAX_STATE {
        return Err(Error::Invalid("Server response exceeds 2 MiB; previous catalogue retained.".into()));
    }
    let text = std::str::from_utf8(&response.body).map_err(|_| Error::Invalid("Server response is not UTF-8 JSON; previous catalogue retained.".into()))?;
    crate::strict_json::parse(text, crate::session::MAX_STATE)
}
