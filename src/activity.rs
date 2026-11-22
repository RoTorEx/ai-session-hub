//! Observed terminal activity and opening of newly completed responses.
//! Opening a pane is not proof that its contents were read.
use crate::{
    content::{self, Role},
    iterm,
    model::{self, Session},
    process,
};
use anyhow::Result;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::time::SystemTime;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum State {
    Working,
    Idle,
    Approval,
    #[default]
    Unknown,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Attention {
    Question,
    Blocked,
    #[default]
    None,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Observation {
    pub state: State,
    pub answer: Option<u64>,
    pub attention: Attention,
    pub request: Option<String>,
    pub model: Option<String>,
    pub reasoning: Option<String>,
    pub last_message: Option<String>,
}
impl Observation {
    pub fn parse(text: &str, provider: process::Provider) -> Self {
        let c = content::parse(text, provider);
        let state = match c.state {
            "Working" => State::Working,
            "Waiting for input" => State::Idle,
            "Needs approval" => State::Approval,
            _ => State::Unknown,
        };
        let message = c
            .messages
            .iter()
            .rev()
            .take_while(|m| m.role != Role::You)
            .find(|m| m.role == Role::Agent);
        let answer = message.map(|m| answer_hash(&m.text));
        let attention = if state == State::Idle {
            message.map_or(Attention::None, |m| attention(&m.text))
        } else {
            Attention::None
        };
        Self {
            state,
            answer,
            attention,
            request: c
                .messages
                .iter()
                .rev()
                .find(|m| m.role == Role::You)
                .map(|m| m.text.clone()),
            model: c.model.clone(),
            reasoning: c.reasoning.clone(),
            last_message: message.map(|m| m.text.clone()),
        }
    }
    /// The bound transcript's latest agent message replaces the visible one. The screen sample
    /// is 24 rows, so the marker of any longer reply is above it: such a reply was never seen
    /// as new, and a question closing it was missed.
    pub fn with_transcript(mut self, message: Option<&str>) -> Self {
        if let Some(message) = message {
            self.answer = Some(answer_hash(message));
            if self.state == State::Idle {
                self.attention = attention(message);
            }
        }
        self
    }
    /// Codex writes its own status into the terminal title: a braille spinner while a turn
    /// runs, "Action Required" while it waits for the user. Screen markers misread both: the
    /// status line is hidden while a message streams mid-turn, and a queued question leaves
    /// "Working" on screen. A recognized approval screen keeps priority.
    pub fn with_title(mut self, title: Option<&str>, provider: process::Provider) -> Self {
        let Some(title) = title.map(str::trim_start) else {
            return self;
        };
        if provider != process::Provider::Codex || self.state == State::Approval {
            return self;
        }
        if title.starts_with(|c| ('\u{2800}'..='\u{28ff}').contains(&c)) {
            self.state = State::Working;
            self.attention = Attention::None;
        } else if title.starts_with('[') && title.contains("Action Required") {
            self.state = State::Idle;
            if self.attention == Attention::None {
                self.attention = Attention::Question;
            }
        }
        self
    }
}
fn answer_hash(text: &str) -> u64 {
    let mut h = DefaultHasher::new();
    text.split_whitespace().collect::<Vec<_>>().hash(&mut h);
    h.finish()
}
fn attention(text: &str) -> Attention {
    let text = text.to_lowercase();
    let tail = text.lines().rev().take(8).collect::<Vec<_>>().join("\n");
    if [
        "i am blocked",
        "i'm blocked",
        "blocked by",
        "blocker:",
        "cannot proceed",
        "can't proceed",
        "unable to proceed",
        "заблокирован",
        "блокер:",
        "не могу продолжить",
    ]
    .iter()
    .any(|p| tail.contains(p))
    {
        Attention::Blocked
    } else if [
        "please provide",
        "please choose",
        "please confirm",
        "please tell me",
        "waiting for your",
        "i need your",
        "уточни, пожалуйста",
        "подтверди, пожалуйста",
        "нужен твой",
        "нужно твоё",
        "жду от тебя",
        "пришли, пожалуйста",
    ]
    .iter()
    .any(|p| tail.contains(p))
        || text
            .lines()
            .rev()
            .find(|line| !line.trim().is_empty())
            .is_some_and(|line| {
                let line = line.trim();
                line.ends_with('?') && !line.starts_with(['>', '`'])
            })
    {
        Attention::Question
    } else {
        Attention::None
    }
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Progress {
    pub state: State,
    working: bool,
    baseline: Option<u64>,
    last_answer: Option<u64>,
    ready: bool,
    opened: bool,
    attention: Attention,
    pub updated_at: Option<SystemTime>,
    pub requested_at: Option<SystemTime>,
    pub request: Option<String>,
    pub model: Option<String>,
    pub reasoning: Option<String>,
    observed: bool,
    last_state: Option<State>,
    pub last_message: Option<String>,
    pub native_metadata: bool,
}
impl Progress {
    pub fn symbol(&self) -> &'static str {
        match self.label() {
            "Working" => "●",
            "Untracked" => "○",
            "Approval" => "!",
            "New reply" => "◆",
            "Opened" => "✓",
            "Question" => "?",
            "Blocked" => "×",
            _ => "·",
        }
    }
    pub fn observe(&mut self, observation: Option<&Observation>, focused: bool) {
        let now = SystemTime::now();
        if let Some(o) = observation {
            if !self.native_metadata
                && self.observed
                && (self
                    .last_state
                    .is_some_and(|s| s != o.state && o.state != State::Unknown)
                    || (o.answer.is_some() && self.last_answer != o.answer)
                    || o.request
                        .as_ref()
                        .is_some_and(|r| self.request.as_ref() != Some(r)))
            {
                self.updated_at = Some(now);
            }
            if let Some(request) = &o.request
                && !self.native_metadata
            {
                if self.observed && self.request.as_ref().is_some_and(|r| r != request) {
                    self.requested_at = Some(now);
                }
                self.request = Some(request.clone());
            }
            if o.model.is_some() {
                self.model.clone_from(&o.model);
            }
            if o.reasoning.is_some() {
                self.reasoning.clone_from(&o.reasoning);
            }
            if !self.native_metadata {
                self.last_message.clone_from(&o.last_message);
            }
            self.last_state = Some(o.state);
            self.observed = true;
        }
        self.state = observation.map_or(State::Unknown, |o| o.state);
        self.attention = observation.map_or(Attention::None, |o| o.attention);
        if let Some(o) = observation {
            if o.state == State::Working && !self.working {
                self.working = true;
                self.baseline = self.last_answer;
                self.ready = false;
                self.opened = false;
            } else if o.state == State::Idle && self.working {
                self.ready = o.answer.is_some() && o.answer != self.baseline;
                self.working = false;
            }
            if self.opened && o.answer.is_some() && o.answer != self.last_answer {
                self.opened = false;
            }
            if o.answer.is_some() {
                self.last_answer = o.answer;
            }
        }
        if focused && self.state == State::Idle {
            self.acknowledge();
        }
    }
    pub fn acknowledge(&mut self) {
        if self.state == State::Idle {
            self.ready = false;
            self.opened = true;
        }
    }
    pub fn label(&self) -> &'static str {
        match self.state {
            State::Working => "Working",
            State::Approval => "Approval",
            State::Unknown => "Unknown",
            State::Idle if self.attention == Attention::Blocked => "Blocked",
            State::Idle if self.attention == Attention::Question => "Question",
            State::Idle if self.ready => "New reply",
            State::Idle if self.opened => "Opened",
            State::Idle => "Untracked",
        }
    }
    pub fn explanation(&self) -> &'static str {
        match self.label() {
            "New reply" => "New response · not opened since it arrived",
            "Opened" => "Idle · pane opened · reading not confirmed",
            "Untracked" => "Idle · opening unknown",
            "Working" => "Working",
            "Approval" => "Needs approval",
            "Question" => "Idle · question or action requested from you",
            "Blocked" => "Idle · agent reported a blocker",
            _ => "Live · activity unknown",
        }
    }
    pub fn opening(&self) -> &'static str {
        if self.ready {
            "New response · pane not observed open"
        } else if self.opened {
            "Pane opened · reading not confirmed"
        } else {
            "Earlier opening history unknown"
        }
    }
}
pub struct Snapshot {
    pub sessions: Vec<(String, Option<Observation>)>,
    pub focused: Option<String>,
    pub metadata: std::collections::HashMap<String, crate::session_data::Metadata>,
}
pub fn read(sessions: &[Session], terminal: &mut iterm::Client) -> Result<Snapshot> {
    let table = process::scan()?;
    let valid: Vec<_> = sessions
        .iter()
        .filter(|s| {
            s.location
                .as_ref()
                .is_some_and(|p| model::valid_identity(s, p, &table))
        })
        .collect();
    let panes: Vec<_> = valid.iter().filter_map(|s| s.location.clone()).collect();
    let observed = terminal.observe(&panes)?;
    let mut metadata = crate::session_data::read(&valid);
    // Reject identity reuse during the file read.
    let after = process::scan()?;
    metadata.retain(|id, _| {
        valid.iter().any(|s| {
            &s.id == id
                && after
                    .iter()
                    .any(|p| p.pid == s.pid && p.started == s.started)
        })
    });
    let sessions = sessions
        .iter()
        .map(|s| {
            let index = valid.iter().position(|v| v.id == s.id);
            let text = index
                .and_then(|i| observed.texts.get(i))
                .and_then(|s| s.as_deref());
            let title = index
                .and_then(|i| observed.titles.get(i))
                .and_then(|s| s.as_deref());
            (
                s.id.clone(),
                text.map(|text| {
                    let message = metadata.get(&s.id).and_then(|m| m.last_message.as_deref());
                    Observation::parse(text, s.provider)
                        .with_transcript(message)
                        .with_title(title, s.provider)
                }),
            )
        })
        .collect();
    Ok(Snapshot {
        sessions,
        focused: observed.focused,
        metadata,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn timing_records_changes_without_treating_initial_or_failed_samples_as_events() {
        let mut p = Progress::default();
        let idle = o(State::Idle, Some(1));
        p.observe(Some(&idle), false);
        assert!(p.updated_at.is_none());
        p.observe(None, false);
        p.observe(Some(&idle), false);
        assert!(p.updated_at.is_none());
        p.observe(Some(&o(State::Working, Some(1))), false);
        let changed = p.updated_at;
        assert!(changed.is_some());
        p.observe(None, false);
        p.observe(Some(&o(State::Working, Some(1))), false);
        assert_eq!(p.updated_at, changed);
        let mut request = o(State::Working, Some(1));
        request.request = Some("First visible request".into());
        p.observe(Some(&request), false);
        assert!(p.requested_at.is_none());
        request.request = Some("A different submitted request".into());
        p.observe(Some(&request), false);
        assert!(p.requested_at.is_some());
    }
    fn o(state: State, answer: Option<u64>) -> Observation {
        Observation {
            state,
            answer,
            attention: Attention::None,
            request: None,
            model: None,
            reasoning: None,
            last_message: None,
        }
    }
    #[test]
    fn only_new_observed_completion_can_be_ready_and_opening_acknowledges_it() {
        let mut p = Progress::default();
        p.observe(Some(&o(State::Idle, Some(1))), false);
        assert_eq!(p.label(), "Untracked");
        p.observe(Some(&o(State::Working, Some(1))), false);
        p.observe(Some(&o(State::Idle, Some(2))), false);
        assert_eq!(p.label(), "New reply");
        p.observe(None, true);
        assert_eq!(p.label(), "Unknown");
        p.observe(Some(&o(State::Idle, Some(2))), false);
        assert_eq!(p.label(), "New reply");
        p.observe(Some(&o(State::Idle, Some(2))), true);
        assert_eq!(p.label(), "Opened");
        p.observe(Some(&o(State::Idle, Some(3))), false);
        assert_eq!(p.label(), "Untracked");
        p.observe(Some(&o(State::Idle, Some(2))), false);
        p.observe(Some(&o(State::Working, Some(2))), true);
        assert_eq!(p.label(), "Working");
        p.observe(Some(&o(State::Approval, Some(2))), false);
        assert_eq!(p.label(), "Approval");
        p.observe(Some(&o(State::Idle, Some(2))), false);
        assert_eq!(p.label(), "Untracked");
    }
    #[test]
    fn opening_initial_idle_is_recorded_and_missing_response_markers_do_not_erase_it() {
        let mut p = Progress::default();
        p.observe(Some(&o(State::Idle, Some(1))), false);
        assert_eq!(p.label(), "Untracked");
        p.acknowledge();
        assert_eq!(p.label(), "Opened");
        p.observe(Some(&o(State::Idle, None)), false);
        assert_eq!(p.label(), "Opened");
        p.observe(Some(&o(State::Idle, Some(1))), false);
        assert_eq!(p.label(), "Opened");
        let mut first_visit = Progress::default();
        first_visit.observe(Some(&o(State::Idle, Some(1))), true);
        assert_eq!(first_visit.label(), "Opened");
    }

    #[test]
    fn parser_requires_provider_state_and_an_agent_response() {
        let o = Observation::parse(
            "› task\n• New answer\n› draft\n70% context left",
            process::Provider::Codex,
        );
        assert_eq!(o.state, State::Idle);
        assert!(o.answer.is_some());
        let o = Observation::parse(
            "› task\n• Ran tests\n› draft\n70% context left",
            process::Provider::Codex,
        );
        assert!(o.answer.is_none());
        let o = Observation::parse(
            "› old task\n• Which branch?\n› new task\n• Ran tests\n› draft\n70% context left",
            process::Provider::Codex,
        );
        assert_eq!(o.attention, Attention::None);
        assert!(o.answer.is_none());
        assert_eq!(
            Observation::parse("plain output", process::Provider::Claude).state,
            State::Unknown
        );
    }
    #[test]
    fn transcript_reply_counts_when_its_marker_is_above_the_sampled_rows() {
        let claude = process::Provider::Claude;
        // The tail of a long reply: no reply marker within the sampled rows.
        let working = "3. third point\n\n✻ Cooking… (esc to interrupt)\n❯\n⏵⏵ auto mode on (shift+tab to cycle)";
        let idle =
            "3. third point\n\nShould I continue?\n\n❯\n⏵⏵ auto mode on (shift+tab to cycle)";
        let mut p = Progress::default();
        p.observe(Some(&Observation::parse(working, claude)), false);
        p.observe(Some(&Observation::parse(idle, claude)), false);
        assert_eq!(p.label(), "Untracked");

        let read = |text, message| Observation::parse(text, claude).with_transcript(message);
        let mut p = Progress::default();
        p.observe(Some(&read(idle, Some("Old reply."))), false);
        p.observe(Some(&read(working, None)), false);
        p.observe(
            Some(&read(idle, Some("1. first\n2. second\n3. third point"))),
            false,
        );
        assert_eq!(p.label(), "New reply");
        p.observe(Some(&read(working, None)), false);
        p.observe(
            Some(&read(idle, Some("3. third point\n\nShould I continue?"))),
            false,
        );
        assert_eq!(p.label(), "Question");
        // An interrupted turn adds no reply.
        p.observe(Some(&read(working, None)), false);
        p.observe(Some(&read(idle, None)), false);
        assert_eq!(p.label(), "Untracked");
    }
    #[test]
    fn codex_title_outranks_screen_markers_that_misread_a_running_turn() {
        let codex = process::Provider::Codex;
        // The status line is hidden while a message streams mid-turn.
        let streaming = "› Task\n• Part of the answer\n\n› Ask Codex to do anything\n\n  ← for agents · ? for shortcuts";
        // A queued question leaves the running status on screen.
        let queued = "› Task\n• Now I need the drive.\n\n? Connect the drive.\n\n  • Connected\n  • Later\n\n• Working (20m 42s • esc to interrupt)\n\n• Queued follow-up inputs\n  ? 1 question\n    ⇧← to answer\n\n› Ask Codex to do anything\n\n  ← for agents · ? for shortcuts";
        let read = |text, title| Observation::parse(text, codex).with_title(Some(title), codex);
        assert_eq!(Observation::parse(streaming, codex).state, State::Idle);
        assert_eq!(
            read(streaming, "⠹ Task | project (codex)").state,
            State::Working
        );
        assert_eq!(read(streaming, "Task | project (codex)").state, State::Idle);

        let mut p = Progress::default();
        p.observe(Some(&read(queued, "⠹ Task | project (codex)")), false);
        p.observe(Some(&read(streaming, "⠹ Task | project (codex)")), false);
        assert_eq!(p.label(), "Working");
        p.observe(
            Some(&read(
                queued,
                "[ . ] Action Required | Task | project (codex)",
            )),
            false,
        );
        assert_eq!(p.label(), "Question");
        p.observe(Some(&read(streaming, "Task | project (codex)")), false);
        assert_eq!(p.label(), "New reply");

        let approval = "Would you like to run this command?\n  Yes, proceed";
        assert_eq!(
            read(approval, "[ ! ] Action Required | Task | project (codex)").state,
            State::Approval
        );
        let claude = process::Provider::Claude;
        let idle = "❯ Task\n⏺ Done.\n❯ draft\n? for shortcuts";
        assert_eq!(
            Observation::parse(idle, claude)
                .with_title(Some("⠹ Task (claude)"), claude)
                .state,
            State::Idle
        );
    }
    #[test]
    fn distinguishes_questions_blockers_approval_and_working_from_idle() {
        assert_eq!(
            Observation::parse(
                "❯ Task\n⏺ Please confirm the target branch.\n❯ draft\n? for shortcuts",
                process::Provider::Claude
            )
            .attention,
            Attention::Question
        );
        assert_eq!(
            Observation::parse(
                "❯ Task\n⏺ I cannot proceed without access.\n❯ draft\n? for shortcuts",
                process::Provider::Claude
            )
            .attention,
            Attention::Blocked
        );
        for (response, expected) in [
            ("Which branch should I use?", Attention::Question),
            (
                "Please provide the missing credentials.",
                Attention::Question,
            ),
            ("I am blocked by missing access.", Attention::Blocked),
            ("Не могу продолжить без доступа.", Attention::Blocked),
            ("Everything is done.", Attention::None),
            ("What caused it?\nThe bug is fixed now.", Attention::None),
        ] {
            let text = format!("› Task\n• {response}\n› draft\n70% context left");
            assert_eq!(
                Observation::parse(&text, process::Provider::Codex).attention,
                expected
            );
            let text = format!("{text}\nWorking (esc to interrupt)");
            assert_eq!(
                Observation::parse(&text, process::Provider::Codex).attention,
                Attention::None
            );
        }
        let c = Observation::parse(
            "Do you want to proceed?\n70% context left",
            process::Provider::Codex,
        );
        assert_eq!(c.state, State::Approval);
        let mut p = Progress::default();
        p.observe(Some(&o(State::Working, None)), false);
        let question = Observation {
            state: State::Idle,
            answer: Some(1),
            attention: Attention::Question,
            request: None,
            model: None,
            reasoning: None,
            last_message: None,
        };
        p.observe(Some(&question), false);
        assert_eq!(p.symbol(), "?");
        assert!(p.opening().contains("not observed"));
        p.observe(Some(&question), true);
        assert_eq!(p.symbol(), "?");
        assert!(p.opening().contains("reading not confirmed"));
    }
}
