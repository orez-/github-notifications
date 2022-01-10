use std::env;
use std::process;
mod github;

#[derive(Debug)]
enum NotificationLevel {
    Owned,
    Subscribed,
    Tagged,
    TeamTagged,
    Other,
}

impl NotificationLevel {
    fn of(notif: &github::Notification, client: &github::Client) -> Result<Self, github::Error> {
        use github::NotificationReason::*;
        let level = match notif.reason {
            Author | CiActivity | SecurityAlert => NotificationLevel::Owned,
            Comment | Manual | Subscribed => NotificationLevel::Subscribed,
            Assign | Mention => NotificationLevel::Tagged,
            ReviewRequested => {
                match notif.subject.details(&client)? {
                    github::PrOrIssue::PullRequest(pr) => {
                        let me = client.current_user()?;
                        let tagged = pr.requested_reviewers.iter().any(|user| user.id == me.id);
                        if tagged { NotificationLevel::Tagged }
                        else { NotificationLevel::TeamTagged }
                    }
                    github::PrOrIssue::Issue(_) => NotificationLevel::Tagged,
                }
            },
            TeamMention => NotificationLevel::TeamTagged,
            Invitation | StateChange | Other => NotificationLevel::Other,
        };
        Ok(level)
    }
}

#[derive(Clone, Copy)]
enum TerminalColor {
    Green = 2,
    Yellow = 3,
    Cyan = 14,
    Gray = 8,
    DarkYellow = 58,
    FadedPurple = 103,
}

impl TerminalColor {
    fn to_code(&self) -> String {
        (*self as u32).to_string()
    }
}

fn to_color(text: &str, color: TerminalColor) -> String {
    format!("\x1b[38;5;{}m{}\x1b[0m", color.to_code(), text)
}

fn format_level(level: &NotificationLevel) -> String {
    use NotificationLevel::*;
    use TerminalColor::*;
    let level_str = format!("{:?}", level);  // xxx don't use debug for real human stuff
    match level {
        Owned => to_color(&level_str, Cyan),
        Subscribed => to_color(&level_str, Green),
        Tagged => to_color(&level_str, Yellow),
        TeamTagged => to_color(&level_str, DarkYellow),
        Other => level_str,
    }.to_string()
}

fn main() {
    let token = match env::var("GITHUB_TOKEN") {
        Ok(t) => t,
        Err(_) => {
            eprintln!("Could not find GITHUB_TOKEN environment variable.");
            eprintln!("  Please generate a token with the `repo` scope and assign it to the GITHUB_TOKEN environment variable.");
            eprintln!("  https://github.com/settings/tokens");
            process::exit(1);
        }
    };
    let client = github::Client::new(token);
    let notifications = client.notifications();
    for notif in notifications.unwrap() {
        let level = NotificationLevel::of(&notif, &client).unwrap();
        let pr = notif.subject.details(&client).unwrap();
        let title = notif.subject.title;
        let title = match pr.state() {
            github::PullRequestState::Open => title,
            github::PullRequestState::Closed =>
                to_color(&title, TerminalColor::FadedPurple),
        };
        println!("[{}] {}", format_level(&level), title);
        println!("  {}", to_color(&pr.html_url(), TerminalColor::Gray));
    }
}
