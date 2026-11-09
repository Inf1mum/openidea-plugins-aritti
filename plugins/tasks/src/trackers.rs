//! Issue trackers, after the `TaskRepository` implementations of
//! intellij-community (GithubRepository, GitlabRepository, ForgejoRepository,
//! YouTrackRepository, JiraRestApiCloud3, TrelloRepository). Each is used
//! once the user has entered its token.

use std::collections::BTreeMap;

use openidea_plugin::{Error, HttpRequest, host};
use serde_json::{Value, json};

use crate::model::{Task,SourceReference};

const LIMIT: u32 = 50;

type Remotes = [(String, String)];

/// What a fetch looks at: `query` turns the listing into a search, `repositories` (by tracker id) override the settings.
pub struct Scope<'a> {
    pub remotes: &'a Remotes,
    pub query: Option<&'a str>,
    pub page: u32,
    pub repositories: &'a BTreeMap<String, Vec<String>>,
}

pub struct Tracker {
    pub id: &'static str,
    pub name: &'static str,
    secrets: &'static [&'static str],
    headers: &'static [(&'static str, &'static str)],
    fetch: fn(&Tracker, &Scope) -> Result<Vec<Task>, Error>,
}

const TRACKERS: [Tracker; 6] = [
    Tracker {
        id: "github",
        name: "GitHub",
        secrets: &["github-token"],
        headers: &[
            ("Authorization", "Bearer {{secret:github-token}}"),
            ("Accept", "application/vnd.github+json"),
            ("User-Agent", "openidea-tasks"),
        ],
        fetch: github,
    },
    Tracker {
        id: "gitlab",
        name: "GitLab",
        secrets: &["gitlab-token"],
        headers: &[("PRIVATE-TOKEN", "{{secret:gitlab-token}}")],
        fetch: gitlab,
    },
    Tracker {
        id: "forgejo",
        name: "Codeberg",
        secrets: &["forgejo-token"],
        headers: &[("Authorization", "Bearer {{secret:forgejo-token}}"), ("Accept", "application/json")],
        fetch: forgejo,
    },
    Tracker {
        id: "youtrack",
        name: "YouTrack",
        secrets: &["youtrack-token"],
        headers: &[("Authorization", "Bearer {{secret:youtrack-token}}"), ("Accept", "application/json")],
        fetch: youtrack,
    },
    Tracker {
        id: "jira",
        name: "Jira",
        secrets: &["jira-token"],
        headers: &[("Authorization", "Basic {{secret:jira-token}}"), ("Accept", "application/json")],
        fetch: jira,
    },
    Tracker {
        id: "trello",
        name: "Trello",
        secrets: &["trello-key", "trello-token"],
        headers: &[("Authorization", "OAuth oauth_consumer_key=\"{{secret:trello-key}}\", oauth_token=\"{{secret:trello-token}}\"")],
        fetch: trello,
    },
];

#[derive(Debug, Default)]
pub struct Fetched {
    pub issues: Vec<Task>,
    pub errors: Vec<String>,
    pub trackers: usize,
}

/// Open issues (or the search hits) of every configured tracker; one failing doesn't stop the others.
pub fn fetch_all(scope: &Scope) -> Fetched {
    let mut fetched = Fetched::default();

    for tracker in &TRACKERS {
        if !tracker.secrets.iter().all(|s| host::secret_is_set(s)) {
            continue;
        }

        fetched.trackers += 1;

        match (tracker.fetch)(tracker, scope) {
            Ok(issues) => fetched.issues.extend(issues),
            Err(e) => fetched.errors.push(format!("{}: {e}", tracker.name)),
        }
    }

    fetched
}

pub fn is_repository_tracker(id: &str) -> bool {
    matches!(id, "github" | "gitlab" | "forgejo")
}

pub fn tracker_name(id: &str) -> &str {
    TRACKERS.iter().find(|t| t.id == id).map_or(id, |t| t.name)
}

pub fn name(task: &Task) -> &str {
    let id = task.tracker.as_deref().unwrap_or_default();

    TRACKERS.iter().find(|t| t.id == id).map_or(id, |t| t.name)
}

/// The Close Task form offers "Update issue state" for these.
pub fn can_update_state(task: &Task) -> bool {
    matches!(task.tracker.as_deref(), Some("github" | "gitlab" | "forgejo" | "youtrack" | "jira"))
}

pub fn can_post_time(task: &Task) -> bool {
    matches!(task.tracker.as_deref(), Some("youtrack" | "jira"))
}

/// TaskRepository.setTaskState with the tracker's resolved state.
pub fn close_issue(task: &Task) -> Result<(), Error> {
    let (tracker, api) = issue_api(task)?;

    match tracker.id {
        "github" | "forgejo" => send(tracker, "PATCH", api, Some(json!({ "state": "closed" }))).map(drop),
        "gitlab" => send(tracker, "PUT", &format!("{api}?state_event=close"), None).map(drop),
        "youtrack" => {
            let state = setting("youtrackCloseState").unwrap_or_else(|| "Fixed".into());
            let commands = format!("{}/api/commands", origin(api));
            let command = json!({ "query": format!("state {state}"), "issues": [{ "idReadable": task.id }] });

            send(tracker, "POST", &commands, Some(command)).map(drop)
        }
        "jira" => {
            let transitions = send(tracker, "GET", &format!("{api}/transitions"), None)?;
            let done = items(&transitions["transitions"], |t| {
                (t["to"]["statusCategory"]["key"] == "done").then(|| t["id"].as_str().map(str::to_owned)).flatten()
            });
            let id = done.first().ok_or("None of this issue's transitions leads to a done status.")?;

            send(tracker, "POST", &format!("{api}/transitions"), Some(json!({ "transition": { "id": id } }))).map(drop)
        }
        _ => Err(format!("{} issues can't be closed from the IDE.", tracker.name).into()),
    }
}

/// YouTrackRepository.updateTimeSpent / JiraRestApi.updateTimeSpend.
pub fn post_time(task: &Task, seconds: u64) -> Result<(), Error> {
    let (tracker, api) = issue_api(task)?;

    match tracker.id {
        "youtrack" => {
            let item = json!({ "usesMarkdown": true, "text": "", "duration": { "minutes": seconds.div_ceil(60) } });
            send(tracker, "POST", &format!("{api}/timeTracking/workItems"), Some(item)).map(drop)
        }
        "jira" => send(tracker, "POST", &format!("{api}/worklog"), Some(json!({ "timeSpentSeconds": seconds.max(60) }))).map(drop),
        _ => Err(format!("Posting work time to {} isn't supported.", tracker.name).into()),
    }
}

fn issue_api(task: &Task) -> Result<(&'static Tracker, &str), Error> {
    let tracker = TRACKERS.iter().find(|t| Some(t.id) == task.tracker.as_deref()).ok_or("The task isn't a tracker issue.")?;
    let api = task.api.as_deref().ok_or("The issue has no API address. Update issues and try again.")?;

    Ok((tracker, api))
}

/// The repositories added in the Tasks view, else the setting, else the remote's.
fn repositories(scope: &Scope, tracker: &str, setting_key: &str, remote_host: &str) -> Vec<String> {
    let added = scope.repositories.get(tracker).cloned().unwrap_or_default();

    if !added.is_empty() {
        return added;
    }

    setting(setting_key).or_else(|| remote_path(scope.remotes, remote_host)).into_iter().collect()
}

fn github(tracker: &Tracker, scope: &Scope) -> Result<Vec<Task>, Error> {
    let repos = repositories(scope, "github", "githubRepository", "github.com");

    if repos.is_empty() {
        return Err("No github.com remote. Set the repository in Settings › Plugins › Tasks.".into());
    }

    let mut query = format!("{} is:issue is:open", repos.iter().map(|r| format!("repo:{r}")).collect::<Vec<_>>().join(" "));

    if host::setting_bool("githubAssignedOnly") == Some(true) {
        query.push_str(" assignee:@me");
    }

    if let Some(extra) = setting("githubQuery") {
        query = format!("{query} {extra}");
    }

    if let Some(text) = scope.query {
        query = format!("{query} {text}");
    }

    let url = format!("https://api.github.com/search/issues?q={}&sort=updated&per_page={LIMIT}&page={}", encode(&query), scope.page);
    let first_name = repos[0].rsplit('/').next().unwrap_or(&repos[0]).to_owned();

    Ok(items(&send(tracker, "GET", &url, None)?["items"], |issue| {
        let repo_name = issue["repository_url"].as_str().and_then(|u| u.rsplit('/').next()).unwrap_or(&first_name);

        Some(Task::issue(
            "github",
            &format!("{repo_name}-{}", issue["number"].as_u64()?),
            issue["title"].as_str()?,
            issue["html_url"].as_str()?,
            issue["url"].as_str()?,
        ))
    }))
}

fn gitlab(tracker: &Tracker, scope: &Scope) -> Result<Vec<Task>, Error> {
    let projects = repositories(scope, "gitlab", "gitlabProject", "gitlab.com");
    let search = scope.query.map(|q| format!("&search={}", encode(q))).unwrap_or_default();
    let bases = match projects.is_empty() {
        true => vec!["https://gitlab.com/api/v4/issues".to_owned()],
        false => projects.iter().map(|p| format!("https://gitlab.com/api/v4/projects/{}/issues", encode(p))).collect(),
    };
    let single = projects.len() == 1;
    let mut found = Vec::new();

    for base in bases {
        let url = format!("{base}?state=opened&order_by=updated_at&per_page={LIMIT}{search}&page={}", scope.page);

        found.extend(items(&send(tracker, "GET", &url, None)?, |issue| {
            let id = match single {
                true => format!("#{}", issue["iid"].as_u64()?),
                false => issue["references"]["full"].as_str()?.to_owned(),
            };

            let mut task=Task::issue("gitlab", &id, issue["title"].as_str()?, issue["web_url"].as_str()?, issue["_links"]["self"].as_str()?);
            if let (Some(full),Some(project_id),Some(iid))=(issue["references"]["full"].as_str(),issue["project_id"].as_u64(),issue["iid"].as_u64()) {
                if let Some((path,suffix))=full.rsplit_once('#') {
                    if path.len()<=100 && path.split('/').count()>=2 && path.split('/').all(|p|!p.is_empty()&&!matches!(p,"."|"..")&&p.bytes().all(|b|b.is_ascii_alphanumeric()||b"_-.".contains(&b))) && suffix==iid.to_string() && project_id>0 && iid>0
                        && task.api.as_deref()==Some(format!("https://gitlab.com/api/v4/projects/{project_id}/issues/{iid}").as_str())
                        && task.url.as_deref()==Some(format!("https://gitlab.com/{path}/-/issues/{iid}").as_str()) {
                        task.source_reference=Some(SourceReference::GitLab{project_path:path.into(),project_id,iid});
                    }
                }
            }
            Some(task)
        }));
    }

    Ok(found)
}

fn forgejo(tracker: &Tracker, scope: &Scope) -> Result<Vec<Task>, Error> {
    let repos = repositories(scope, "forgejo", "forgejoRepository", "codeberg.org");

    if repos.is_empty() {
        return Err("No codeberg.org remote. Set the repository in Settings › Plugins › Tasks.".into());
    }

    let search = scope.query.map(|q| format!("&q={}", encode(q))).unwrap_or_default();
    let mut found = Vec::new();

    for repo in &repos {
        let url = format!(
            "https://codeberg.org/api/v1/repos/{repo}/issues?type=issues&state=open&sort=updated&limit={LIMIT}{search}&page={}",
            scope.page
        );
        let prefix = if repos.len() > 1 { repo.as_str() } else { "" };

        found.extend(items(&send(tracker, "GET", &url, None)?, |issue| {
            Some(Task::issue(
                "forgejo",
                &format!("{prefix}#{}", issue["number"].as_u64()?),
                issue["title"].as_str()?,
                issue["html_url"].as_str()?,
                issue["url"].as_str()?,
            ))
        }));
    }

    Ok(found)
}

fn youtrack(tracker: &Tracker, scope: &Scope) -> Result<Vec<Task>, Error> {
    let site = site("youtrackHost", "YouTrack")?;
    let query = scope.query.map(str::to_owned).or_else(|| setting("youtrackQuery")).unwrap_or_default();
    let url = format!(
        "https://{site}/api/issues?query={}&fields=idReadable,summary&$top={LIMIT}&$skip={}",
        encode(&query),
        (scope.page.saturating_sub(1)) * LIMIT
    );

    Ok(items(&send(tracker, "GET", &url, None)?, |issue| {
        let id = issue["idReadable"].as_str()?;

        Some(Task::issue(
            "youtrack",
            id,
            issue["summary"].as_str()?,
            &format!("https://{site}/issue/{id}"),
            &format!("https://{site}/api/issues/{id}"),
        ))
    }))
}

fn jira(tracker: &Tracker, scope: &Scope) -> Result<Vec<Task>, Error> {
    Ok(jira_page(tracker, scope, None)?.issues)
}
fn jira_page(tracker: &Tracker, scope: &Scope, cursor: Option<&str>) -> Result<SearchPage, Error> {
    if cursor.is_some_and(|c| c.is_empty() || c.len() > 4096 || c.chars().any(char::is_control)) {
        return Err("Invalid Jira continuation token.".into());
    }
    let site = site("jiraHost", "Jira")?;
    let jql = match scope.query {
        Some(text) => format!(
            "text ~ \"{}\" order by updated DESC",
            text.replace('\\', "\\\\").replace('"', "\\\"")
        ),
        None => setting("jiraQuery").unwrap_or_default(),
    };
    let mut url = format!(
        "https://{site}/rest/api/3/search/jql?jql={}&maxResults={LIMIT}&fields=summary",
        encode(&jql)
    );

    if let Some(cursor) = cursor {
        url.push_str(&format!("&nextPageToken={}", encode(cursor)));
    }
    let response = send(tracker, "GET", &url, None).map_err(|error|match error {Error::Denied(_)=>Error::Denied("Jira search permission denied.".into()),_=>Error::Failed("Jira search failed. Retry the page or start a new query.".into())})?;
    let raw = response["issues"]
        .as_array()
        .ok_or("Jira search response lacks an issues array.")?;
    if raw.len() > LIMIT as usize {
        return Err("Jira returned more than the requested page limit.".into());
    }
    let next = match response.get("nextPageToken") {
        None | Some(Value::Null) => None,
        Some(Value::String(token))
            if !token.is_empty() && token.len() <= 4096 && !token.chars().any(char::is_control) =>
        {
            Some(token.clone())
        }
        _ => return Err("Invalid Jira continuation token.".into()),
    };
    if next.as_deref().is_some_and(|n| Some(n) == cursor) {
        return Err("Jira repeated its continuation token.".into());
    }
    if response.get("isLast").is_some_and(|v| !v.is_boolean())
        || (response["isLast"] == true && next.is_some())
        || (response["isLast"] == false && next.is_none())
    {
        return Err("Inconsistent Jira pagination metadata.".into());
    }
    let issues = items(&response["issues"], |issue| {
        let key = issue["key"].as_str()?;

        Some(Task::issue(
            "jira",
            key,
            issue["fields"]["summary"].as_str()?,
            &format!("https://{site}/browse/{key}"),
            &format!("https://{site}/rest/api/3/issue/{key}"),
        ))
    });
    if issues.len() != raw.len() {
        return Err("Jira returned an incomplete issue record.".into());
    }
    Ok(SearchPage { issues, next })
}

/// The cards the user is a member of (TrelloRepository without a board), or the cards matching a search.
fn trello(tracker: &Tracker, scope: &Scope) -> Result<Vec<Task>, Error> {
    let (url, cards_at) = match scope.query {
        Some(text) => (
            format!("https://api.trello.com/1/search?query={}&modelTypes=cards&cards_limit={LIMIT}&cards_page={}&card_fields=name,url,shortLink", encode(text),scope.page.saturating_sub(1)),
            "cards",
        ),
        None => ("https://api.trello.com/1/members/me/cards?filter=visible&fields=name,url,shortLink".to_owned(), ""),
    };
    let response = send(tracker, "GET", &url, None)?;
    let cards = if cards_at.is_empty() { &response } else { &response[cards_at] };

    let raw=cards.as_array().ok_or("Trello response lacks a cards array.")?;
    if scope.query.is_some() && raw.len()>LIMIT as usize {return Err("Trello returned more than the requested page limit.".into());}
    let parsed=items(cards, |card| {
        let id = card["id"].as_str()?;

        let mut task=Task::issue("trello",id,card["name"].as_str()?,card["url"].as_str()?,&format!("https://api.trello.com/1/cards/{id}"));
        if let Some(link)=card["shortLink"].as_str().filter(|s|s.len()==8&&s.bytes().all(|b|b.is_ascii_alphanumeric())) {
            let canonical=format!("https://trello.com/c/{link}");
            if id.len()==24 && id.bytes().all(|b|b.is_ascii_hexdigit()) && task.url.as_deref().is_some_and(|u|u==canonical||u.strip_prefix(&format!("{canonical}/")).is_some_and(|tail|!tail.is_empty()&&!tail.contains(['?','#','\\'])&&!tail.chars().any(char::is_control))) {
                task.url=Some(canonical);task.source_reference=Some(SourceReference::Trello{short_link:link.into()});
            }
        }
        Some(task)
    });
    if parsed.len()!=raw.len() {return Err("Trello returned an incomplete card record.".into());}
    Ok(parsed)
}

fn send(tracker: &Tracker, method: &str, url: &str, body: Option<Value>) -> Result<Value, Error> {
    let mut request = HttpRequest { method: method.into(), url: url.into(), ..HttpRequest::default() };

    for (name, value) in tracker.headers {
        request = request.header(name, value);
    }

    if let Some(body) = body {
        request = request.header("Content-Type", "application/json");
        request.body = Some(body.to_string().into_bytes());
    }

    let response = host::fetch(&request)?;

    if response.body.len() > 1 << 20 {
        return Err("Tracker response exceeds the 1 MiB limit.".into());
    }
    if response.status >= 400 {
        return Err(format!("HTTP {}: {}", response.status, response.text().chars().take(200).collect::<String>()).into());
    }

    if response.body.is_empty() {
        return Ok(Value::Null);
    }

    serde_json::from_slice(&response.body).map_err(|e| format!("Unexpected response: {e}.").into())
}

fn items<T>(list: &Value, item: impl Fn(&Value) -> Option<T>) -> Vec<T> {
    list.as_array().into_iter().flatten().filter_map(item).collect()
}

fn setting(key: &str) -> Option<String> {
    host::setting_string(key).map(|s| s.trim().to_owned()).filter(|s| !s.is_empty())
}

/// A host setting, without the `https://` or path users paste along.
fn site(key: &str, tracker: &str) -> Result<String, Error> {
    let value = setting(key).ok_or_else(|| format!("Set the {tracker} host in Settings › Plugins › Tasks."))?;
    let value = value.split_once("://").map_or(value.as_str(), |(_, rest)| rest);

    Ok(value.split('/').next().unwrap_or_default().to_ascii_lowercase())
}

/// `https://host` of an API address.
fn origin(url: &str) -> &str {
    let host_end = url.find("://").map_or(0, |i| i + 3);

    url[host_end..].find('/').map_or(url, |i| &url[..host_end + i])
}

/// `owner/name` of the first remote on `host`, `origin` first.
fn remote_path(remotes: &Remotes, host: &str) -> Option<String> {
    let mut found: Vec<_> = remotes.iter().filter_map(|(name, url)| Some((name != "origin", path_on(url, host)?))).collect();
    found.sort();

    found.into_iter().next().map(|(_, path)| path)
}

fn path_on(url: &str, host: &str) -> Option<String> {
    let rest = url.split_once("://").map_or(url, |(_, rest)| rest);
    let rest = rest.rsplit_once('@').map_or(rest, |(_, rest)| rest);
    let path = rest.strip_prefix(host)?.strip_prefix([':', '/'])?;
    let path = path.trim_end_matches('/').trim_end_matches(".git");

    path.contains('/').then(|| path.to_owned())
}

fn encode(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

/// A concrete server/repository selection. Search never fans a page out to
/// every repository or follows a server-supplied pagination URL.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SearchTarget {
    pub tracker: &'static str,
    pub server: String,
    pub repository: Option<String>,
    pub label: String,
}
impl SearchTarget {
    pub fn paginated(&self) -> bool {
        matches!(
            self.tracker,
            "github" | "gitlab" | "forgejo" | "youtrack" | "jira" | "trello"
        )
    }
}
pub fn search_targets(scope: &Scope) -> Result<Vec<SearchTarget>, Error> {
    let mut targets = Vec::new();
    for tracker in &TRACKERS {
        if !tracker.secrets.iter().all(|secret| host::secret_is_set(secret)) {
            continue;
        }
        let (server, repos) = match tracker.id {
            "github" => ("api.github.com".into(), repositories(scope, "github", "githubRepository", "github.com")),
            "gitlab" => ("gitlab.com".into(), repositories(scope, "gitlab", "gitlabProject", "gitlab.com")),
            "forgejo" => ("codeberg.org".into(), repositories(scope, "forgejo", "forgejoRepository", "codeberg.org")),
            "youtrack" => {
                let Ok(server) = site("youtrackHost", "YouTrack") else {
                    continue;
                };
                (server, Vec::new())
            }
            "jira" => {
                let Ok(server) = site("jiraHost", "Jira") else {
                    continue;
                };
                (server, Vec::new())
            }
            "trello" => ("api.trello.com".into(), Vec::new()),
            _ => continue,
        };
        let allowed = match tracker.id {
            "youtrack" => cloud_host(&server, ".youtrack.cloud"),
            "jira" => cloud_host(&server, ".atlassian.net"),
            _ => true,
        };
        if !allowed {
            continue;
        }
        let repos: Vec<Option<String>> = if repos.is_empty() { vec![None] } else { repos.into_iter().map(Some).collect() };
        for repository in repos {
            if let Some(repo) = &repository {
                if repo.len() > 256
                    || repo.split('/').any(|part| {
                        part.is_empty()
                            || part == "."
                            || part == ".."
                            || !part.bytes().all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
                    })
                {
                    return Err("Invalid tracker repository path.".into());
                }
                if matches!(tracker.id, "github" | "forgejo") && repo.split('/').count() != 2 {
                    return Err("GitHub/Codeberg repositories use owner/name.".into());
                }
            }
            let label =
                format!("{} · {}{}", tracker.name, server, repository.as_ref().map(|repo| format!(" / {repo}")).unwrap_or_default());
            targets.push(SearchTarget { tracker: tracker.id, server: server.clone(), repository, label });
            if targets.len() > 16 {
                return Err("Issue search supports at most 16 configured server/repository selections.".into());
            }
        }
    }
    Ok(targets)
}
fn cloud_host(host: &str, suffix: &str) -> bool {
    host.strip_suffix(suffix)
        .is_some_and(|tenant| !tenant.is_empty() && tenant.len() <= 128 && tenant.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-'))
}
/// Exact relevant configuration, without retaining authentication values or
/// credential-bearing remote URLs. Presence of a secret is not its identity.
pub fn search_signature(scope: &Scope) -> Result<String, Error> {
    let targets = search_targets(scope)?;
    let settings = ["githubQuery", "youtrackQuery", "jiraQuery"].map(|key| setting(key).unwrap_or_default());
    if settings.iter().any(|value| value.len() > 4096) {
        return Err("Tracker query setting exceeds 4096 bytes.".into());
    }
    let identities: Vec<_> = targets.iter().map(|target| (target.tracker, &target.server, &target.repository)).collect();
    Ok(json!([identities, settings, host::setting_bool("githubAssignedOnly")]).to_string())
}
#[derive(Clone)]
pub struct SearchPage {
    pub issues: Vec<Task>,
    pub next: Option<String>,
}
#[cfg(test)]
pub fn search_page(scope: &Scope, target: &SearchTarget, page: u32) -> Result<Vec<Task>, Error> {
    search_page_cursor(scope, target, page, None).map(|p| p.issues)
}
pub fn search_page_cursor(
    scope: &Scope,
    target: &SearchTarget,
    page: u32,
    cursor: Option<&str>,
) -> Result<SearchPage, Error> {
    if !(1..=5).contains(&page) || (!target.paginated() && page != 1) {
        return Err("Unsupported search page.".into());
    }
    if (target.tracker == "jira" && (page == 1) != cursor.is_none())
        || (target.tracker != "jira" && cursor.is_some())
    {
        return Err("Search continuation does not match its page.".into());
    }
    if target.tracker=="trello" && scope.query.is_none() {return Err("Trello paging requires an explicit search query.".into());}
    if !search_targets(scope)?.contains(target) {
        return Err("Tracker/server configuration changed. Search again.".into());
    }
    let tracker = TRACKERS
        .iter()
        .find(|tracker| tracker.id == target.tracker)
        .ok_or("Unknown tracker")?;
    let mut repos = scope.repositories.clone();
    if let Some(repo) = &target.repository {
        repos.insert(target.tracker.into(), vec![repo.clone()]);
    }
    let selected = Scope {
        remotes: scope.remotes,
        query: scope.query,
        repositories: &repos,
        page,
    };
    let fetched = if target.tracker == "jira" {
        jira_page(tracker, &selected, cursor)?
    } else {
        SearchPage {
            issues: (tracker.fetch)(tracker, &selected)?,
            next: None,
        }
    };
    let issues = fetched.issues;
    let mut seen = std::collections::BTreeSet::new();
    let mut safe = Vec::new();
    for issue in issues {
        let api = issue.api.as_deref().unwrap_or_default();
        let web = issue.url.as_deref().unwrap_or_default();
        let web_host = match target.tracker {
            "github" => "github.com",
            "trello" => "trello.com",
            _ => target.server.as_str(),
        };
        if let Some(SourceReference::GitLab{project_path,project_id,..})=&issue.source_reference {
            if target.repository.as_ref().is_some_and(|repo|repo!=project_path&&repo!=&project_id.to_string()) {return Err("GitLab reference is outside the selected repository.".into());}
        }
        if issue.id.is_empty()
            || issue.id.len() > 128
            || issue.summary.len() > 4096
            || api.len() > 2048
            || web.len() > 2048
            || !api.starts_with(&format!("https://{}/", target.server))
            || !web.starts_with(&format!("https://{web_host}/"))
            || [issue.id.as_str(), issue.summary.as_str(), api, web]
                .iter()
                .any(|value| value.chars().any(char::is_control))
        {
            return Err("Tracker returned an invalid or foreign-server issue. Search results were not cached.".into());
        }
        if target.tracker == "github" {
            if let Some(repo) = &target.repository {
                if !api.to_ascii_lowercase().starts_with(&format!(
                    "https://api.github.com/repos/{}/issues/",
                    repo.to_ascii_lowercase()
                )) {
                    return Err("GitHub returned an issue outside the selected repository.".into());
                }
            }
        }
        let path = api
            .strip_prefix(&format!("https://{}/", target.server))
            .unwrap_or_default();
        let numeric_tail = |prefix: &str| {
            path.strip_prefix(prefix)
                .is_some_and(|tail| !tail.is_empty() && tail.bytes().all(|b| b.is_ascii_digit()))
        };
        let endpoint = match target.tracker {
            "github" => path.rsplit_once("/issues/").is_some_and(|(base, tail)| {
                base.starts_with("repos/")
                    && !tail.is_empty()
                    && tail.bytes().all(|b| b.is_ascii_digit())
            }),
            "gitlab" => path.rsplit_once("/issues/").is_some_and(|(base, tail)| {
                base.starts_with("api/v4/projects/")
                    && !tail.is_empty()
                    && tail.bytes().all(|b| b.is_ascii_digit())
            }),
            "forgejo" => target
                .repository
                .as_ref()
                .is_some_and(|repo| numeric_tail(&format!("api/v1/repos/{repo}/issues/"))),
            "youtrack" => {
                path == format!("api/issues/{}", issue.id)
                    && issue
                        .id
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b"_-".contains(&b))
            }
            "jira" => {
                path == format!("rest/api/3/issue/{}", issue.id)
                    && issue
                        .id
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b"_-".contains(&b))
            }
            "trello" => {
                path == format!("1/cards/{}", issue.id)
                    && issue
                        .id
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b"_-".contains(&b))
            }
            _ => false,
        };
        if !endpoint {
            return Err("Tracker returned an unsupported issue API endpoint. Search results were not cached.".into());
        }
        if seen.insert(api.to_string()) {
            safe.push(issue);
        }
        if safe.len() == 50 {
            break;
        }
    }
    Ok(SearchPage {
        issues: safe,
        next: fetched.next,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::HttpResponse;
    use openidea_plugin::mock::MockHost;

    fn respond(host: &MockHost, body: &'static str) {
        host.on_fetch(move |_| Ok(HttpResponse { status: 200, headers: vec![], body: body.into() }));
    }

    #[test]
    fn finds_the_project_of_ssh_and_https_remotes() {
        let remotes = vec![
            ("fork".to_owned(), "https://github.com/me/tool.git".to_owned()),
            ("origin".to_owned(), "git@github.com:acme/tool.git".to_owned()),
            ("lab".to_owned(), "ssh://git@gitlab.com/group/sub/proj".to_owned()),
        ];

        assert_eq!(remote_path(&remotes, "github.com").as_deref(), Some("acme/tool"));
        assert_eq!(remote_path(&remotes, "gitlab.com").as_deref(), Some("group/sub/proj"));
        assert_eq!(remote_path(&remotes, "bitbucket.org"), None);
    }

    #[test]
    fn encodes_query_values() {
        assert_eq!(encode("repo:a/b is:open"), "repo%3Aa%2Fb%20is%3Aopen");
        assert_eq!(origin("https://acme.youtrack.cloud/api/issues/X-1"), "https://acme.youtrack.cloud");
    }

    #[test]
    fn closes_a_jira_issue_through_its_done_transition() {
        let host = MockHost::install().with_capabilities(&["secrets", "net:acme.atlassian.net"]);
        host.set_secret("jira-token", "Zm9v");
        respond(
            &host,
            r#"{"transitions":[{"id":"11","to":{"statusCategory":{"key":"indeterminate"}}},{"id":"31","to":{"statusCategory":{"key":"done"}}}]}"#,
        );
        let task = Task::issue(
            "jira",
            "PROJ-7",
            "Crash",
            "https://acme.atlassian.net/browse/PROJ-7",
            "https://acme.atlassian.net/rest/api/3/issue/PROJ-7",
        );

        close_issue(&task).unwrap();

        let requests = host.requests();
        assert_eq!(requests[1].method, "POST");
        assert_eq!(requests[1].url, "https://acme.atlassian.net/rest/api/3/issue/PROJ-7/transitions");
        assert_eq!(requests[1].body.as_deref(), Some(br#"{"transition":{"id":"31"}}"#.as_slice()));
        assert!(requests[1].headers.contains(&("Authorization".into(), "Basic Zm9v".into())));
    }

    #[test]
    fn posts_youtrack_work_items_in_minutes() {
        let host = MockHost::install().with_capabilities(&["secrets", "net:acme.youtrack.cloud"]);
        host.set_secret("youtrack-token", "perm:x");
        respond(&host, "");
        let task =
            Task::issue("youtrack", "X-1", "Crash", "https://acme.youtrack.cloud/issue/X-1", "https://acme.youtrack.cloud/api/issues/X-1");

        post_time(&task, 3601).unwrap();

        let request = &host.requests()[0];
        assert_eq!(request.url, "https://acme.youtrack.cloud/api/issues/X-1/timeTracking/workItems");
        let body: Value = serde_json::from_slice(request.body.as_deref().unwrap()).unwrap();
        assert_eq!(body["duration"]["minutes"], 61);
    }

    #[test]
    fn reads_trello_cards_with_both_secrets_in_one_header() {
        let host = MockHost::install().with_capabilities(&["secrets", "net:api.trello.com"]);
        host.set_secret("trello-key", "k");
        host.set_secret("trello-token", "t");
        respond(&host, r#"[{"id":"5f3a","name":"Ship it","url":"https://trello.com/c/abc"}]"#);

        let fetched = fetch_all(&Scope { remotes: &[], query: None, page: 1, repositories: &BTreeMap::new() });

        assert_eq!(fetched.trackers, 1);
        assert_eq!(fetched.issues[0].label(), "5f3a: Ship it");
        let auth = &host.requests()[0].headers[0].1;
        assert_eq!(auth, r#"OAuth oauth_consumer_key="k", oauth_token="t""#);
    }
    #[test]
    fn search_pagination_and_cloud_host_restrictions_are_explicit() {
        let host = MockHost::install().with_capabilities(&["secrets", "net:*"]);
        host.set_secret("youtrack-token", "token");
        host.set_setting("youtrackHost", "\"tenant.youtrack.cloud\"");
        respond(&host, "[]");
        let repositories = BTreeMap::new();
        let scope = Scope { remotes: &[], repositories: &repositories, query: Some("bug"), page: 2 };
        let target = search_targets(&scope).unwrap().remove(0);
        search_page(&scope, &target, 2).unwrap();
        assert!(host.requests()[0].url.contains("$skip=50"));
        assert!(search_page(&scope, &target, 6).is_err());
        host.set_setting("youtrackHost", "\"evil.youtrack.cloud.evil.example\"");
        assert!(search_targets(&scope).unwrap().is_empty());
        assert!(!cloud_host("user@tenant.youtrack.cloud", ".youtrack.cloud"));
    }
    #[test]
    fn page_parameters_are_tracker_specific_and_cursor_trackers_refuse_more() {
        let host = MockHost::install().with_capabilities(&["secrets", "net:*"]);
        for secret in ["github-token", "gitlab-token", "forgejo-token", "jira-token", "trello-key", "trello-token"] {
            host.set_secret(secret, "token");
        }
        host.set_setting("jiraHost", "\"tenant.atlassian.net\"");
        respond(&host, "[]");
        let repositories = BTreeMap::from([
            ("github".into(), vec!["acme/tool".into()]),
            ("gitlab".into(), vec!["group/sub/tool".into()]),
            ("forgejo".into(), vec!["acme/tool".into()]),
        ]);
        let scope = Scope { remotes: &[], repositories: &repositories, query: Some("bug & fix"), page: 2 };
        let targets = search_targets(&scope).unwrap();
        for target in &targets {
            if target.tracker=="trello" {continue;} // Dedicated search-page shape is exercised below.
            if target.paginated() && !matches!(target.tracker,"jira"|"trello") {
                search_page(&scope, target, 2).unwrap();
            } else {
                assert!(search_page(&scope, target, 2).is_err());
            }
        }
        let requests = host.requests();
        assert_eq!(requests.len(), 3);
        assert!(requests.iter().all(|request| request.url.contains("page=2")));
        assert!(requests.iter().any(|request| request.url.contains("projects/group%2Fsub%2Ftool/issues")));
        assert!(requests.iter().all(|request| request.url.contains("bug%20%26%20fix")));
        assert!(targets.iter().find(|target| target.tracker == "jira").is_some_and(|target| target.paginated()));
        assert!(targets.iter().find(|target| target.tracker == "trello").is_some_and(|target| target.paginated()));
    }
}

#[cfg(test)]
mod jira_cursor_contract_tests {
    use super::*;
    use openidea_plugin::{HttpResponse,mock::MockHost};
    #[test]
    fn jira_requires_valid_bounded_nonrepeated_metadata_and_explicit_page_cursor() {
        let h=MockHost::install().with_capabilities(&["secrets","net:acme.atlassian.net"]);h.set_secret("jira-token","fixture");h.set_setting("jiraHost",r#""acme.atlassian.net""#);let repos=BTreeMap::new();let scope=Scope{remotes:&[],query:Some("bug"),page:1,repositories:&repos};let t=search_targets(&scope).unwrap().remove(0);
        assert!(search_page_cursor(&scope,&t,2,None).is_err());assert!(h.requests().is_empty());
        for value in [json!({"issues":[],"nextPageToken":17}),json!({"issues":[],"nextPageToken":""}),json!({"issues":[],"nextPageToken":"a\nb"}),json!({"issues":[],"isLast":false}),json!({"issues":[],"isLast":true,"nextPageToken":"A"}),json!({"issues":[],"nextPageToken":"A"}),json!({"issues":[{"key":"APP-1"}]})] {
            h.on_fetch(move |_|Ok(HttpResponse{status:200,headers:vec![],body:value.to_string().into_bytes()}));assert!(search_page_cursor(&scope,&t,2,Some("A")).is_err());
        }
        let before=h.requests().len();assert!(search_page_cursor(&scope,&t,2,Some(&"x".repeat(4097))).is_err());assert_eq!(h.requests().len(),before);
    }
}

#[cfg(test)]
mod trello_page_contract_tests {
    use super::*;
    use openidea_plugin::mock::MockHost;
    #[test]
    fn trello_member_listing_cannot_be_mistaken_for_a_search_page() {
        let h=MockHost::install().with_capabilities(&["secrets","net:api.trello.com"]);h.set_secret("trello-key","key");h.set_secret("trello-token","token");let repositories=BTreeMap::new();let scope=Scope{remotes:&[],repositories:&repositories,query:None,page:1};let target=search_targets(&scope).unwrap().remove(0);
        assert!(search_page_cursor(&scope,&target,1,None).is_err());assert!(search_page_cursor(&scope,&target,6,None).is_err());assert!(h.requests().is_empty());
        let query=Scope{query:Some("bug"),..scope};
        for body in [serde_json::json!({}),serde_json::json!({"cards":[{"id":"broken"}]})] {
            h.on_fetch(move |_|Ok(openidea_plugin::HttpResponse{status:200,headers:vec![],body:body.to_string().into_bytes()}));assert!(search_page_cursor(&query,&target,1,None).is_err());
        }
    }
}
