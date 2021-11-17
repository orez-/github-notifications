use std::env::temp_dir;
use std::fs;
use std::path::PathBuf;
use std::time::{Duration, SystemTime};

use serde::Deserialize;
use serde::de::DeserializeOwned;

const API_PREFIX: &str = "https://api.github.com/";
const NOTIFICATIONS_URL: &str = "https://api.github.com/notifications";
const SELF_URL: &str = "https://api.github.com/user";

type DateTime = String;  // (∙⥚∙)
type DeserializeError = serde_path_to_error::Error<serde_json::Error>;

fn to_filename(url: &str) -> PathBuf {
    let url = url.strip_prefix(API_PREFIX).unwrap_or(url);
    let url = url.replace('/', "__");
    let url = url.replace(|c| !matches!(c, 'a'..='z' | '0'..='9' | '.' | '_'), "");
    let mut path = temp_dir();
    path.push("github-notifications");
    fs::create_dir_all(&path).unwrap();
    path.push(url);
    path
}

#[derive(Debug)]
pub enum Error {
    Reqwest(reqwest::Error),
    Deserialize(DeserializeError),
    Io(std::io::Error),
}

impl From<reqwest::Error> for Error {
    fn from(item: reqwest::Error) -> Self {
        Error::Reqwest(item)
    }
}

impl From<DeserializeError> for Error {
    fn from(item: DeserializeError) -> Self {
        Error::Deserialize(item)
    }
}

impl From<std::io::Error> for Error {
    fn from(item: std::io::Error) -> Self {
        Error::Io(item)
    }
}

#[derive(Deserialize, Debug)]
pub struct User {
    pub id: u64,
    pub login: String,
}

#[derive(Deserialize, Debug)]
pub struct PullRequest {
    pub url: String,
    pub id: u64,
    pub number: u64,
    pub state: PullRequestState,
    pub locked: bool,
    pub title: String,
    pub html_url: String,
    pub requested_reviewers: Vec<User>,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "snake_case")]
pub enum PullRequestState {
    Open,
    Closed,
}

// https://docs.github.com/en/rest/reference/activity#notifications
#[derive(Deserialize, Debug)]
#[serde(rename_all = "snake_case")]
pub enum NotificationReason {
    Assign,          // You were assigned to the issue.
    Author,          // You created the thread.
    Comment,         // You commented on the thread.
    CiActivity,      // A GitHub Actions workflow run that you triggered was completed.
    Invitation,      // You accepted an invitation to contribute to the repository.
    Manual,          // You subscribed to the thread (via an issue or pull request).
    Mention,         // You were specifically @mentioned in the content.
    ReviewRequested, // You, or a team you're a member of, were requested to review a pull request.
    SecurityAlert,   // GitHub discovered a security vulnerability in your repository.
    StateChange,     // You changed the thread state (for example, closing an issue or merging a pull request).
    Subscribed,      // You're watching the repository.
    TeamMention,     // You were on a team that was mentioned.
    #[serde(other)]
    Other,
}

#[derive(Deserialize, Debug)]
pub enum SubjectType {
    PullRequest,
    Issue,
}

#[derive(Deserialize, Debug)]
pub struct NotificationSubject {
    pub title: String,
    pub url: String,
    pub latest_comment_url: Option<String>,
    pub r#type: SubjectType,
}

impl NotificationSubject {
    pub fn pull_request(&self, client: &Client) -> Result<Option<PullRequest>, Error> {
        if !matches!(self.r#type, SubjectType::PullRequest) {
            return Ok(None);
        }
        Ok(Some(client.get(&self.url, Some(Duration::from_secs(60)))?))
    }
}

#[derive(Deserialize, Debug)]
pub struct Notification {
    pub id: String,
    pub reason: NotificationReason,
    pub subject: NotificationSubject,
    pub unread: bool,
    pub updated_at: DateTime,
    pub last_read_at: Option<DateTime>,
    pub url: String,
}

// https://rust-lang.github.io/rfcs/3137-let-else.html
// https://twitter.com/ekuber/status/1456686889755242499
// 😫
fn cache_is_fresh(path: &PathBuf, ttl: Option<Duration>) -> bool {
    if !path.exists() {
        return false;
    }
    let ttl = match ttl {
        Some(e) => e,
        None => { return true; }
    };
    let metadata = match fs::metadata(path) {
        Ok(e) => e,
        Err(_) => { return false; }
    };
    let modified = match metadata.modified() {
        Ok(e) => e,
        Err(_) => { return false; }
    };
    let since = SystemTime::now().duration_since(modified).unwrap_or(Duration::ZERO);
    since < ttl
}

pub struct Client {
    token: String,
}

impl Client {
    pub fn new(token: String) -> Self {
        Self { token }
    }

    // No ttl means cache never expires
    fn get<T: DeserializeOwned>(&self, url: &str, ttl: Option<Duration>) -> Result<T, Error> {
        let path = to_filename(url);
        let body: String;
        if cache_is_fresh(&path, ttl) {
            body = fs::read_to_string(path)?;
        }
        else {
            let client = reqwest::blocking::Client::new();
            let request = client.get(url)
                .bearer_auth(&self.token)
                .header("user-agent", "reqwest")
                .build()?;
            body = client.execute(request)?.text()?;
            if let Err(error) = fs::write(path, &body) {
                eprintln!("Couldn't save! {:?}", error);
            }
        }
        let body_de = &mut serde_json::Deserializer::from_str(&body);
        Ok(serde_path_to_error::deserialize(body_de)?)
    }

    pub fn notifications(&self) -> Result<Vec<Notification>, Error> {
        self.get(NOTIFICATIONS_URL, Some(Duration::ZERO))
    }

    pub fn current_user(&self) -> Result<User, Error> {
        self.get(SELF_URL, Some(Duration::from_secs(60 * 60 * 24)))
    }
}
