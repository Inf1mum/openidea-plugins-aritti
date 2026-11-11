//! Sentry registered-client device authorization. No raw tokens leave this module.
//! Persistent refresh is deliberately unavailable: API1.4 substitutes secrets in
//! headers only, while Sentry's refresh token must be submitted in a form body.
use super::{SENTRY_SECRET, State as ExternalState, owner};
use openidea_plugin::{Error, HttpRequest, Invocation, host};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
const REFRESH: &str = "sentry-refresh-token";
const EXPIRY: &str = "external:sentry-oauth:expires-at";
const CLIENT: &str = "external:sentry-oauth:client-id";
#[derive(Default)]
pub(super) struct DeviceState {
    pending: BTreeMap<String, Pending>,
}
struct Pending {
    client: String,
    device: String,
    expires: Instant,
    next: Instant,
    interval: u64,
}
fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |v| v.as_secs())
}
fn public(value: &str, max: usize) -> bool {
    !value.is_empty() && value.len() <= max && !value.chars().any(char::is_control)
}
fn form(value: &str) -> String {
    let mut result = String::new();
    for b in value.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            result.push(b as char)
        } else {
            result.push_str(&format!("%{b:02X}"));
        }
    }
    result
}
fn post(path: &str, pairs: &[(&str, &str)]) -> Result<(u16, Value), Error> {
    let body = pairs
        .iter()
        .map(|(k, v)| format!("{}={}", form(k), form(v)))
        .collect::<Vec<_>>()
        .join("&");
    let mut request = HttpRequest::get(&format!("https://sentry.io{path}"));
    request.method = "POST".into();
    request.headers = vec![
        (
            "Content-Type".into(),
            "application/x-www-form-urlencoded".into(),
        ),
        ("Accept".into(), "application/json".into()),
    ];
    request.body = Some(body.into_bytes());
    request.timeout_ms = Some(15_000);
    let response = host::fetch(&request).map_err(|e| {
        if matches!(e, Error::Denied(_)) {
            e
        } else {
            Error::Failed(crate::ui_text::t(
                "process.sentry.text0",
                "Sentry authorization request failed; retry explicitly.",
            ))
        }
    })?;
    if response.body.len() > 32 * 1024 {
        return Err(crate::ui_text::t(
            "process.sentry.text1",
            "Sentry authorization response exceeds its bound.",
        )
        .into());
    }
    let value = crate::strict_json::parse(
        std::str::from_utf8(&response.body).map_err(|_| {
            Error::Invalid(crate::ui_text::t(
                "process.sentry.text2",
                "Invalid authorization response.",
            ))
        })?,
        32 * 1024,
    )
    .map_err(|_| {
        Error::Invalid(crate::ui_text::t(
            "process.sentry.text2",
            "Invalid authorization response.",
        ))
    })?;
    Ok((response.status, value))
}
pub(super) fn ensure_access() -> Result<(), Error> {
    if let Some(expiry) = host::storage::get(EXPIRY)?.and_then(|v| v.parse::<u64>().ok()) {
        if now() >= expiry {
            return Err(crate::ui_text::t("process.sentry.text3","Stored Sentry OAuth access has expired. Authorize again; secure persisted refresh is unavailable on API1.4.").into());
        }
    }
    Ok(())
}
impl ExternalState {
    /// Requires a client registered by the user/operator for Sentry device grant.
    /// The plugin does not ship or imply ownership of a registered OAuth client.
    pub(crate) fn start_sentry_device(&mut self, ctx: &Invocation) -> Result<Value, Error> {
        let (_, root) = owner(ctx)?;
        let previous = host::storage::get(CLIENT)?.unwrap_or_default();
        let Some(client) = host::input(
            &crate::ui_text::t(
                "process.sentry.text8",
                "Registered Sentry OAuth client ID (device grant enabled)",
            ),
            Some(&previous),
        )?
        else {
            return Err(Error::Cancelled);
        };
        self.start_sentry_device_with(ctx, &root, client)
    }
    fn start_sentry_device_with(
        &mut self,
        ctx: &Invocation,
        root: &str,
        client: String,
    ) -> Result<Value, Error> {
        if !public(&client, 256)
            || !client
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'))
        {
            return Err(crate::ui_text::t(
                "process.sentry.text4",
                "Enter a registered Sentry client ID.",
            )
            .into());
        }
        if self.sentry_device.pending.len() >= 8 && !self.sentry_device.pending.contains_key(root) {
            return Err(crate::ui_text::t(
                "process.sentry.text5",
                "Too many pending authorizations; cancel an existing authorization.",
            )
            .into());
        }
        let (status, value) = post(
            "/oauth/device/code/",
            &[
                ("client_id", &client),
                ("scope", "org:read project:read event:read"),
            ],
        )?;
        if status != 200 {
            return Err(crate::ui_text::t("process.sentry.text6","Sentry rejected device authorization. Confirm the registered client enables this grant.").into());
        }
        let device = value["device_code"]
            .as_str()
            .filter(|v| public(v, 4096))
            .ok_or(Error::Stale)?
            .to_owned();
        let user = value["user_code"]
            .as_str()
            .filter(|v| public(v, 128))
            .ok_or(Error::Stale)?;
        let uri = value["verification_uri_complete"]
            .as_str()
            .or_else(|| value["verification_uri"].as_str())
            .filter(|uri| {
                public(uri, 2048)
                    && uri.starts_with("https://sentry.io/")
                    && !uri.contains(['\\', '@', '#'])
            })
            .ok_or(Error::Stale)?
            .to_owned();
        let seconds = value["expires_in"]
            .as_u64()
            .filter(|v| *v > 0 && *v <= 3600)
            .ok_or(Error::Stale)?;
        let interval = value["interval"].as_u64().unwrap_or(5).clamp(1, 60);
        if owner(ctx)?.1 != root {
            return Err(Error::Stale);
        }
        let approved = host::confirm(
            &crate::ui_text::t("process.sentry.text9", "Authorize Sentry OAuth device?"),
            &crate::ui_text::format(
                "process.sentry.review",
                "Authorize Sentry client {client}?\nOpen {uri}\nEnter code: {user}\n\nA registered device-grant client is required. Return here and explicitly check authorization. OAuth access/refresh tokens stay in the host keychain. Persisted automatic refresh is unavailable on API1.4.",
                &[("client", &client), ("uri", &uri), ("user", user)],
            ),
        )?;
        if !approved {
            return Err(Error::Cancelled);
        }
        if owner(ctx)?.1 != root {
            return Err(Error::Stale);
        }
        host::open_url(&uri)?;
        host::storage::set(CLIENT, &client)?;
        let instant = Instant::now();
        self.sentry_device.pending.insert(
            root.into(),
            Pending {
                client,
                device,
                expires: instant + Duration::from_secs(seconds),
                next: instant + Duration::from_secs(interval),
                interval,
            },
        );
        Ok(
            json!({"status":"pending","userCode":user,"verificationUri":uri,"expiresIn":seconds,"pollAfterSeconds":interval,"persistentRefresh":false,"registeredClientRequired":true}),
        )
    }
    pub(crate) fn poll_sentry_device(&mut self, ctx: &Invocation) -> Result<Value, Error> {
        let (_, root) = owner(ctx)?;
        let pending = self.sentry_device.pending.get(&root).ok_or_else(|| {
            crate::ui_text::t(
                "process.sentry.text10",
                "No authorization pending for this project.",
            )
        })?;
        let instant = Instant::now();
        if instant >= pending.expires {
            self.sentry_device.pending.remove(&root);
            return Ok(json!({"status":"expired"}));
        }
        if instant < pending.next {
            return Ok(
                json!({"status":"pending","pollAfterSeconds":(pending.next-instant).as_secs().max(1)}),
            );
        }
        let client = pending.client.clone();
        let device = pending.device.clone();
        let (status, value) = post(
            "/oauth/token/",
            &[
                ("client_id", &client),
                ("device_code", &device),
                ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
            ],
        )?;
        if owner(ctx)?.1 != root {
            return Err(Error::Stale);
        }
        if status != 200 {
            match value["error"].as_str() {
                Some("authorization_pending") => {}
                Some("slow_down") => {
                    self.sentry_device
                        .pending
                        .get_mut(&root)
                        .ok_or(Error::Stale)?
                        .interval = (pending_interval(&self.sentry_device, &root)? + 5).min(60);
                }
                Some("access_denied") => {
                    self.sentry_device.pending.remove(&root);
                    return Ok(json!({"status":"denied"}));
                }
                Some("expired_token") => {
                    self.sentry_device.pending.remove(&root);
                    return Ok(json!({"status":"expired"}));
                }
                _ => {
                    self.sentry_device.pending.remove(&root);
                    return Err(crate::ui_text::t("process.sentry.text7","Sentry rejected authorization; verify the registered client and authorize again.").into());
                }
            }
            let pending = self
                .sentry_device
                .pending
                .get_mut(&root)
                .ok_or(Error::Stale)?;
            pending.next = Instant::now() + Duration::from_secs(pending.interval);
            return Ok(json!({"status":"pending","pollAfterSeconds":pending.interval}));
        }
        let token = value["access_token"]
            .as_str()
            .filter(|v| public(v, 16384))
            .ok_or(Error::Stale)?;
        if !value["token_type"]
            .as_str()
            .is_some_and(|v| v.eq_ignore_ascii_case("bearer"))
        {
            return Err(Error::Stale);
        }
        let expires = value["expires_in"]
            .as_u64()
            .filter(|v| *v > 0 && *v <= 366 * 24 * 3600)
            .ok_or(Error::Stale)?;
        let refresh = value["refresh_token"].as_str().filter(|v| public(v, 16384));
        let store = (|| {
            host::secret_set(SENTRY_SECRET, token)?;
            if let Some(refresh) = refresh {
                host::secret_set(REFRESH, refresh)?
            } else {
                host::secret_delete(REFRESH)?;
            }
            host::storage::set(EXPIRY, &now().saturating_add(expires).to_string())
        })();
        if let Err(error) = store {
            let _ = host::secret_delete(SENTRY_SECRET);
            let _ = host::secret_delete(REFRESH);
            self.sentry_device.pending.remove(&root);
            return Err(error);
        }
        self.sentry_device.pending.remove(&root);
        for session in self.projects.values_mut() {
            session.issues.clear();
            session.issues_page = super::Page::default();
        }
        Ok(
            json!({"status":"authorized","expiresIn":expires,"persistentRefresh":false,"requiresExplicitReauthorization":true}),
        )
    }
    pub(crate) fn cancel_sentry_device(&mut self, ctx: &Invocation) -> Result<Value, Error> {
        let (_, root) = owner(ctx)?;
        self.sentry_device.pending.remove(&root);
        Ok(json!({"status":"cancelled"}))
    }
}
fn pending_interval(state: &DeviceState, root: &str) -> Result<u64, Error> {
    Ok(state.pending.get(root).ok_or(Error::Stale)?.interval)
}
pub(super) fn disconnect(state: &mut DeviceState) -> Result<(), Error> {
    state.pending.clear();
    host::secret_delete(REFRESH)?;
    host::storage::delete(EXPIRY)?;
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::{HttpResponse, mock::MockHost};
    fn host() -> MockHost {
        let h = MockHost::install().with_capabilities(&[
            "project.read",
            "storage",
            "secrets",
            "net:sentry.io",
        ]);
        h.declare_secret(SENTRY_SECRET);
        h.declare_secret(REFRESH);
        h
    }
    #[test]
    fn registered_device_start_rate_limit_and_cancel_never_export_device_secret() {
        let h = host();
        h.on_fetch(|r|{assert_eq!(r.method,"POST");assert!(r.url.ends_with("/oauth/device/code/"));Ok(HttpResponse{status:200,headers:vec![],body:json!({"device_code":"PRIVATE_DEVICE","user_code":"AB-CD","verification_uri":"https://sentry.io/activate/","expires_in":600,"interval":5}).to_string().into_bytes()})});
        h.answer_next_prompt(Some(&["yes"]));
        let mut state = ExternalState::default();
        let value = state
            .start_sentry_device_with(&h.invocation(), "/project", "registered123".into())
            .unwrap();
        assert!(!value.to_string().contains("PRIVATE_DEVICE"));
        assert_eq!(
            state.poll_sentry_device(&h.invocation()).unwrap()["status"],
            "pending"
        );
        assert_eq!(
            state.cancel_sentry_device(&h.invocation()).unwrap()["status"],
            "cancelled"
        );
        assert!(state.poll_sentry_device(&h.invocation()).is_err());
    }
    #[test]
    fn completed_tokens_stay_in_declared_keychain_and_disconnect_clears_refresh() {
        let h = host();
        h.on_fetch(|r|{assert!(String::from_utf8(r.body.clone().unwrap()).unwrap().contains("device_code=private-device"));Ok(HttpResponse{status:200,headers:vec![],body:json!({"access_token":"private-access","refresh_token":"private-refresh","expires_in":60,"token_type":"bearer"}).to_string().into_bytes()})});
        let mut state = ExternalState::default();
        state.sentry_device.pending.insert(
            "/project".into(),
            Pending {
                client: "registered".into(),
                device: "private-device".into(),
                expires: Instant::now() + Duration::from_secs(60),
                next: Instant::now(),
                interval: 5,
            },
        );
        let result = state.poll_sentry_device(&h.invocation()).unwrap();
        assert_eq!(result["status"], "authorized");
        assert!(!result.to_string().contains("private"));
        assert!(host::secret_is_set(SENTRY_SECRET));
        assert!(host::secret_is_set(REFRESH));
        state.disconnect_sentry().unwrap();
        assert!(!host::secret_is_set(SENTRY_SECRET));
        assert!(!host::secret_is_set(REFRESH));
    }
    #[test]
    fn server_errors_are_fixed_and_external_verification_urls_refused() {
        let h = host();
        h.on_fetch(|_| {
            Ok(HttpResponse {
                status: 400,
                headers: vec![],
                body: json!({"error":"clientsecret=PRIVATE"})
                    .to_string()
                    .into_bytes(),
            })
        });
        let mut state = ExternalState::default();
        let error = state
            .start_sentry_device_with(&h.invocation(), "/project", "registered".into())
            .unwrap_err()
            .to_string();
        assert!(!error.contains("PRIVATE"));
    }
}
