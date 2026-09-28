//! Pure conversational turns ("hi", "thanks", "what can you do?") are
//! answered without tools.
//!
//! Evidence (live harness, qa-greeting, 0.9.3–0.9.5): "hi" made the model
//! call `directory_tree` / `shell_exec` in most runs (4 of 5 on 0.9.5, and
//! up to 6 calls in one run), then describe the empty workspace. The
//! planning request carried every tool, a system prompt demanding that the
//! first response be a tool call, and the workspace-grounding context.
//!
//! [`is_pure_chat`] classifies the task before planning. It is a whole-message
//! parse, not a keyword search: the message is split into clauses, and EVERY
//! clause must be one of a closed set of conversational forms — greeting,
//! wellbeing question, thanks/acknowledgement, farewell, or a question about
//! the agent itself — with nothing else left over. "hi, can you fix the
//! failing test in src/x.rs" has a clause that is none of those, so it is a
//! task. Anything code-, path- or URL-shaped, or longer than a short line,
//! is a task. A bare "ok" / "yes" is a confirmation, never chat.
//!
//! A chat turn is sent with no tool schemas, a short system prompt that says
//! no tools are available, the planning quota's output cap with thinking
//! off ([`crate::api::ThinkingMode::Chat`]), and none of the per-turn
//! workspace context (learning hint, work ledger, tool manifest). A reply
//! that still contains a tool call means the classification was wrong: the
//! task falls back to normal planning.

/// Longest message (chars) that can be pure chat.
const MAX_CHAT_CHARS: usize = 80;
/// Most words a pure-chat message may have.
const MAX_CHAT_WORDS: usize = 12;

/// Complete conversational clauses (after normalization, see [`clauses`]).
const CHAT_CLAUSES: &[&str] = &[
    // greetings
    "hi",
    "hello",
    "hey",
    "heya",
    "hiya",
    "hey hey",
    "yo",
    "howdy",
    "greetings",
    "hola",
    "sup",
    "what is up",
    "wassup",
    "good morning",
    "good afternoon",
    "good evening",
    "good day",
    "morning",
    "evening",
    "gm",
    "hello world",
    "nice to meet you",
    // wellbeing
    "how are you",
    "how are you doing",
    "how are you today",
    "how is it going",
    "how are things",
    "how do you do",
    "how have you been",
    "you there",
    "are you there",
    // thanks / acknowledgement
    "thanks",
    "thank you",
    "thx",
    "ty",
    "thanks a lot",
    "thanks so much",
    "thanks very much",
    "thank you so much",
    "thank you very much",
    "thanks a bunch",
    "many thanks",
    "cheers",
    "much appreciated",
    "appreciate it",
    "i appreciate it",
    "great",
    "awesome",
    "perfect",
    "nice",
    "cool",
    "great job",
    "nice work",
    "good job",
    "well done",
    "that is great",
    "that helps",
    "that helped",
    "that is helpful",
    "very helpful",
    // farewells
    "bye",
    "goodbye",
    "bye bye",
    "see you",
    "see you later",
    "see ya",
    "good night",
    "take care",
    "have a good day",
    "have a nice day",
    // about the agent itself
    "who are you",
    "what are you",
    "what is your name",
    "who made you",
    "who built you",
    "who created you",
    "what can you do",
    "what else can you do",
    "what can you help with",
    "what can you help me with",
    "what do you do",
    "how can you help",
    "how can you help me",
    "what are your capabilities",
    "what are you capable of",
    "what model are you",
    "which model are you",
    "what model is this",
    "are you an ai",
    "are you a bot",
    "are you a robot",
    "are you human",
    "introduce yourself",
    "tell me about yourself",
    "what is selfware",
    "help",
];

/// Words dropped at the start of a clause ("ok thanks", "oh hi").
const LEADING_FILLERS: &[&str] = &["oh", "ok", "okay", "well", "so", "um", "hmm", "and", "also"];

/// Words dropped at the end of a clause ("hi there", "thanks selfware").
const TRAILING_ADDRESSEES: &[&str] = &[
    "there", "selfware", "buddy", "friend", "mate", "pal", "man", "dude", "all", "everyone",
    "folks", "guys", "again", "bot", "agent", "please",
];

/// Whether `task` is purely conversational (see the module docs).
pub(crate) fn is_pure_chat(task: &str) -> bool {
    let task = task.trim();
    if task.is_empty() || task.chars().count() > MAX_CHAT_CHARS {
        return false;
    }
    // Code, paths, URLs, commands, mentions: never chat.
    if task.contains([
        '/', '\\', '`', '=', '{', '}', '(', ')', '[', ']', '<', '>', '#', '@', '$', '|', '*', '_',
        '~', '"',
    ]) || task.lines().count() > 1
    {
        return false;
    }
    let Some(clauses) = clauses(task) else {
        return false;
    };
    let mut matched = 0;
    for forms in &clauses {
        if forms.iter().all(String::is_empty) {
            continue;
        }
        if !forms.iter().any(|f| CHAT_CLAUSES.contains(&f.as_str())) {
            return false;
        }
        matched += 1;
    }
    matched > 0
}

/// The message as normalized clauses: lower case, split at sentence
/// punctuation and " and ", contractions expanded, elongated greetings
/// collapsed ("hiii"). Each clause comes in its written form and with
/// leading fillers and/or trailing addressees dropped ("ok thanks" ->
/// "thanks", "hi there" -> "hi"; "well done" and "are you there" keep
/// their own form). `None` when the message has too many words or
/// characters that are not text.
fn clauses(task: &str) -> Option<Vec<Vec<String>>> {
    let lower = task.to_lowercase().replace(['’', '‘'], "'");
    let mut words_total = 0;
    let mut out = Vec::new();
    for raw in lower.split(['.', ',', '!', '?', ';', ':']) {
        // Emoji and other symbols carry no request; letters, digits,
        // apostrophes, hyphens and spaces are all a clause may hold.
        let mut text = String::new();
        for ch in raw.chars() {
            if ch.is_alphanumeric() && !ch.is_ascii() {
                // Non-ASCII letters: another language, not in the closed set.
                return None;
            }
            if ch.is_ascii_alphanumeric() || ch == '\'' || ch == ' ' || ch == '-' {
                text.push(ch);
            } else if ch.is_ascii() && !ch.is_whitespace() {
                return None;
            } else {
                text.push(' ');
            }
        }
        for part in text.split(" and ") {
            let mut words: Vec<String> = part
                .split_whitespace()
                .flat_map(expand_contraction)
                .collect();
            words_total += words.len();
            if words_total > MAX_CHAT_WORDS {
                return None;
            }
            if let Some(first) = words.first_mut() {
                *first = collapse_elongation(first);
            }
            // "ok thanks" -> "thanks"; a bare "ok" keeps its own (non-chat)
            // form, so a confirmation is never chat.
            let mut lead = words.clone();
            while lead.len() > 1 && LEADING_FILLERS.contains(&lead[0].as_str()) {
                lead.remove(0);
            }
            let strip_trailing = |w: &[String]| -> String {
                let mut w = w.to_vec();
                while w.len() > 1 && TRAILING_ADDRESSEES.contains(&w[w.len() - 1].as_str()) {
                    w.pop();
                }
                w.join(" ")
            };
            out.push(vec![
                words.join(" "),
                lead.join(" "),
                strip_trailing(&words),
                strip_trailing(&lead),
            ]);
        }
    }
    Some(out)
}

/// "what's" -> "what is", "u" -> "you", ... (one word to one or two).
fn expand_contraction(word: &str) -> Vec<String> {
    let word = word.trim_matches(['\'', '-']);
    let expanded: &[&str] = match word {
        "what's" | "whats" => &["what", "is"],
        "how's" | "hows" => &["how", "is"],
        "that's" | "thats" => &["that", "is"],
        "who're" => &["who", "are"],
        "u" => &["you"],
        "r" => &["are"],
        "ur" => &["your"],
        "thank-you" => &["thank", "you"],
        "" => &[],
        other => return vec![other.to_string()],
    };
    expanded.iter().map(|w| w.to_string()).collect()
}

/// "hiii" -> "hi", "heyyy" -> "hey", "hellooo" -> "hello": a repeated last
/// letter is dropped only when the result is a greeting word.
fn collapse_elongation(word: &str) -> String {
    const GREETINGS: &[&str] = &["hi", "hey", "hello", "yo", "hiya", "heya", "thanks", "bye"];
    let mut w = word.to_string();
    while !GREETINGS.contains(&w.as_str()) {
        let mut chars = w.chars().rev();
        match (chars.next(), chars.next()) {
            (Some(a), Some(b)) if a == b && w.len() > 2 => {
                w.pop();
            }
            _ => return word.to_string(),
        }
    }
    w
}

/// System prompt of a chat turn: who the agent is, and that this reply has
/// no tools. Replaces the tool-protocol prompt for this one request.
pub(crate) const CHAT_SYSTEM_PROMPT: &str = "You are Selfware, an AI coding agent that works in \
the user's terminal on their project: it reads and edits files, runs shell commands, builds and \
tests, searches and reviews code, and verifies its changes. The user's message is \
conversational (a greeting, thanks, or a question about you), not a task. Reply directly and \
briefly in plain text. No tools are available for this reply: do not call tools, do not \
inspect or describe the workspace. If they want something done, invite them to say what.";

impl super::Agent {
    /// Whether this planning turn is answered as chat: the task was
    /// classified pure chat, nothing has run yet, and plan mode is off.
    pub(super) fn chat_turn_applies(&self) -> bool {
        self.task_is_chat && !self.plan_mode && self.total_tool_call_count() == 0
    }

    /// The request for a chat turn: the chat system prompt and the user's
    /// message — no tool manifest, learning hint or work ledger.
    pub(super) fn chat_request_messages(&self) -> Vec<crate::api::types::Message> {
        vec![
            crate::api::types::Message::system(CHAT_SYSTEM_PROMPT),
            crate::api::types::Message::user(
                self.task_context_for_classification().trim().to_string(),
            ),
        ]
    }
}

#[cfg(test)]
#[path = "../../tests/unit/agent/chat_turn/chat_turn_test.rs"]
mod tests;
