//! Read-only, bounded terminal history for the selected, revalidated pane.
//! It never supplies process identity or authorizes navigation.
use crate::{
    iterm,
    model::{self, Session},
    process::Provider,
};
use anyhow::Result;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Role {
    You,
    Agent,
    Action,
}
impl Role {
    pub fn label(&self) -> &'static str {
        match self {
            Self::You => "You",
            Self::Agent => "Agent",
            Self::Action => "Action",
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Message {
    pub role: Role,
    pub text: String,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Content {
    pub state: &'static str,
    pub messages: Vec<Message>,
    pub context: String,
}
pub fn read(session: &Session, terminal: &mut iterm::Client) -> Result<Content> {
    let pane = model::validate(session)?;
    let text = terminal.content(pane)?;
    Ok(parse(&text, session.provider))
}
fn clean(line: &str) -> String {
    model::safe_text(line).trim_end().to_owned()
}

pub fn parse(text: &str, provider: Provider) -> Content {
    let lines: Vec<_> = text.lines().map(clean).collect();
    let tail = lines
        .iter()
        .rev()
        .take(24)
        .cloned()
        .collect::<Vec<_>>()
        .join("\n");
    let state = if tail.contains("Would you like to run")
        || tail.contains("Do you trust")
        || tail.contains("Do you want to proceed")
        || tail.contains("Yes, proceed")
    {
        "Needs approval"
    } else if tail.contains("esc to interrupt") || tail.contains("ctrl+c to interrupt") {
        "Working"
    } else if tail.contains("context left")
        || tail.contains("? for shortcuts")
        || tail.contains("bypass permissions")
    {
        "Waiting for input"
    } else {
        "Live · activity unknown"
    };
    let mut messages = Vec::new();
    let mut current: Option<Message> = None;
    for line in &lines {
        let trimmed = line.trim_start();
        // Status/footer and the current input/draft are not conversation turns.
        if trimmed.contains("esc to interrupt")
            || trimmed.contains("context left")
            || trimmed.starts_with("tab to queue")
            || trimmed.contains("? for shortcuts")
        {
            continue;
        }
        let marker = match provider {
            Provider::Codex if trimmed.starts_with('›') => {
                Some((Role::You, trimmed.trim_start_matches('›').trim()))
            }
            Provider::Claude if trimmed.starts_with('❯') || trimmed.starts_with('>') => {
                Some((Role::You, trimmed.trim_start_matches(['❯', '>']).trim()))
            }
            Provider::Codex if trimmed.starts_with('•') => {
                let text = trimmed.trim_start_matches('•').trim();
                let action = [
                    "Ran ",
                    "Running ",
                    "Read ",
                    "Edited ",
                    "Updated ",
                    "Searched ",
                    "Explored ",
                ]
                .iter()
                .any(|p| text.starts_with(p));
                Some((if action { Role::Action } else { Role::Agent }, text))
            }
            Provider::Claude if trimmed.starts_with('⏺') => {
                let body = trimmed.trim_start_matches('⏺').trim();
                let action = [
                    "Read(", "Bash(", "Write(", "Edit(", "Update(", "Grep(", "Glob(",
                ]
                .iter()
                .any(|prefix| body.starts_with(prefix));
                Some((if action { Role::Action } else { Role::Agent }, body))
            }
            _ => None,
        };
        if let Some((role, body)) = marker {
            if let Some(previous) = current.take() {
                push(&mut messages, previous);
            }
            current = Some(Message {
                role,
                text: body.to_owned(),
            });
        } else if let Some(message) = current.as_mut()
            && message.text.len() < 8192
        {
            message.text.push('\n');
            message.text.push_str(trimmed);
        }
    }
    // An unanswered bottom prompt can be an unsubmitted draft/queued message.
    // Do not label it as a request that the agent has already received.
    if let Some(last) = current
        && last.role != Role::You
    {
        push(&mut messages, last);
    }
    if messages.len() > 32 {
        messages.drain(..messages.len() - 32);
    }
    let context = if messages.is_empty() {
        lines
            .iter()
            .rev()
            .take(32)
            .rev()
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    } else {
        String::new()
    };
    Content {
        state,
        messages,
        context,
    }
}
fn push(messages: &mut Vec<Message>, mut message: Message) {
    message.text = message.text.trim().chars().take(8192).collect();
    if !message.text.is_empty() {
        messages.push(message);
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reads_visible_turns_actions_and_status_without_treating_draft_as_request() {
        let c = parse(
            "› Fix this\n  please\n• Ran cargo test\n  all passed\n• Fixed it\n• Working (1s • esc to interrupt)\n› unsent draft\n69% context left",
            Provider::Codex,
        );
        assert_eq!(c.state, "Working");
        assert_eq!(c.messages.len(), 3);
        assert_eq!(c.messages[0].text, "Fix this\nplease");
        assert_eq!(c.messages[1].role, Role::Action);
        assert_eq!(c.messages[2].text, "Fixed it");
    }
    #[test]
    fn approval_has_priority_and_unrecognized_screens_remain_unknown() {
        assert_eq!(
            parse(
                "Do you trust this folder?\n? for shortcuts",
                Provider::Claude
            )
            .state,
            "Needs approval"
        );
        let c = parse("unrecognized output", Provider::Claude);
        assert_eq!(c.state, "Live · activity unknown");
        assert_eq!(c.context, "unrecognized output");
    }
}
