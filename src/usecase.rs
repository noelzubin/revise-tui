use crate::store::{SqliteStore, Store, ID};
use chrono::{DateTime, Duration, Utc};
use colored::*;
use fsrs::{MemoryState, FSRS};
use std::process::Command;
use std::{fmt, fs};

pub struct Usecase<S: Store> {
    store: S,
    editor: Option<String>,
}

impl Usecase<SqliteStore> {
    pub fn new() -> Self {
        let store = SqliteStore::new();
        Usecase { 
            store,
            editor: None,
        }
    }

    pub fn new_with_editor(editor: Option<String>) -> Self {
        let store = SqliteStore::new();
        Usecase { 
            store,
            editor,
        }
    }

    fn get_editor(&self) -> String {
        match &self.editor {
            Some(cmd) => cmd.clone(),
            None => std::env::var("EDITOR").unwrap_or_else(|_| "vi".to_string())
        }
    }

    fn spawn_editor(&self, path: &str) {
        let editor_cmd = self.get_editor();
        let mut chars = editor_cmd.chars().peekable();
        let mut args = Vec::new();
        let mut current = String::new();
        let mut in_quotes = false;

        while let Some(c) = chars.next() {
            match c {
                '\'' => in_quotes = !in_quotes,
                ' ' if !in_quotes => {
                    if !current.is_empty() {
                        args.push(current.clone());
                        current.clear();
                    }
                },
                _ => current.push(c),
            }
        }
        if !current.is_empty() {
            args.push(current);
        }

        let program = args.first().expect("Editor command cannot be empty");
        let mut command = Command::new(program);
        command.args(&args[1..]);
        command.arg(path);
        
        command.status().expect("Failed to launch editor");
    }

    pub fn add_deck(&self, name: &str) {
        self.store.add_deck(name).unwrap();
    }

    pub fn list_decks(&self) -> Vec<Deck> {
        let decks = self.store.list_decks().unwrap();
        return decks;
        // decks.iter().for_each(|deck| {
        //     println!("{}.\t{}", deck.id, deck.name);
        // });
    }

    // If no desc get the desc from neovim file
    pub fn add_card(&self, current_deck: Option<&str>) {
        const TMP_FILE_PATH: &str = "/tmp/revise_card.md";
        let content = format!("---\ntitle:\ndeck: {}\n---\n", current_deck.unwrap_or_default());
        fs::write(TMP_FILE_PATH, content).unwrap();

        // retry until you get desc from frontmatter
        let (title, deck_name, desc) = loop {
            self.spawn_editor(TMP_FILE_PATH);
            let desc = std::fs::read_to_string(TMP_FILE_PATH).unwrap();
            let fm = parse_yaml_frontmatter(&desc);

            if let Some(title) = fm.get("title") {
                if !title.trim().is_empty() {
                    if let Some(deck) = fm.get("deck") {
                        if !deck.trim().is_empty() {
                            break (
                                title.trim().to_string(),
                                deck.trim().to_string(),
                                desc.to_string(),
                            );
                        }
                    }
                }
            }
        };

        std::fs::remove_file(TMP_FILE_PATH).unwrap();

        // Find or create the deck
        let deck_id = match self.list_decks().into_iter().find(|d| d.name == deck_name) {
            Some(deck) => deck.id,
            None => {
                self.add_deck(&deck_name);
                self.list_decks()
                    .into_iter()
                    .find(|d| d.name == deck_name)
                    .unwrap()
                    .id
            }
        };

        self.store.add_card(deck_id, &title, &desc).unwrap();
    }

    pub fn list_card_summaries(&self, deck_id: Option<ID>, all: bool, is_suspended: bool) -> Vec<CardSummary> {
        return self.store.list_card_summaries(deck_id, all, is_suspended).unwrap();
    }

    pub fn edit_card(&self, id: ID) {
        const TMP_FILE_PATH: &str = "/tmp/revise_card.md";
        let card = self.store.get_card(id).unwrap();
        fs::write(TMP_FILE_PATH, card.desc).unwrap();

        let (title, deck_name, desc) = loop {
            self.spawn_editor(TMP_FILE_PATH);
            let desc = std::fs::read_to_string(TMP_FILE_PATH).unwrap();
            let fm = parse_yaml_frontmatter(&desc);
            if let Some(title) = fm.get("title") {
                if !title.trim().is_empty() {
                    if let Some(deck) = fm.get("deck") {
                        if !deck.trim().is_empty() {
                            break (
                                title.trim().to_string(),
                                deck.trim().to_string(),
                                desc.to_string(),
                            );
                        }
                    }
                }
            }
        };

        std::fs::remove_file(TMP_FILE_PATH).unwrap();

        let deck_id = if card.deck != deck_name {
            match self.list_decks().into_iter().find(|d| d.name == deck_name) {
                Some(deck) => deck.id,
                None => {
                    self.add_deck(&deck_name);
                    self.list_decks()
                        .into_iter()
                        .find(|d| d.name == deck_name)
                        .unwrap()
                        .id
                }
            }
        } else {
            card.deck_id
        };

        self.store.remove_orphan_decks().unwrap();

        self.store
            .update_card_details(id, &title, deck_id, &desc)
            .unwrap();
    }

    pub fn get_card(&self, id: ID) -> Card {
        let card = self.store.get_card(id).unwrap();
        return card;
    }

    pub fn remove_card(&self, id: ID) {
        self.store.remove_card(id).unwrap();
        self.store.remove_orphan_decks().unwrap();
    }

    pub fn get_reviews(&self, id: ID) -> Vec<Review> {
        self.store.get_reviews(id).unwrap()
    }

    pub fn suspend_card(&self, id: ID) {
        self.store.suspend_card(id).unwrap();
    }

    pub fn unsuspend_card(&self, id: ID) {
        self.store.unsuspend_card(id).unwrap();
    }

    pub fn get_next_dates(&self, card: &CardSummary) -> Vec<(&'static str, f32)> {
        // Find next states
        let last_review = self.store.get_last_review(card.id).unwrap();
        let fsrs = FSRS::new(Some(&[])).unwrap();

        let last_date = last_review
            .as_ref()
            .map(|lr| lr.review_time)
            .unwrap_or(card.created_at);

        let now = Utc::now();
        let days_elapsed = (now - last_date).num_days() as u32;

        let next_states = fsrs
            .next_states(
                last_review.as_ref().map(|r| MemoryState {
                    difficulty: r.difficulty,
                    stability: r.stability,
                }),
                0.9,
                days_elapsed,
            )
            .unwrap();

        let dates = vec![
            ("again", next_states.again.interval.round()),
            ("hard", next_states.hard.interval.round()),
            ("good", next_states.good.interval.round()),
            ("easy", next_states.easy.interval.round()),
        ];
        return dates;
    }

    pub fn revise_card(&self, card_id: ID, n: usize) {
        let card = self.store.get_card(card_id).unwrap();

        let last_review = self.store.get_last_review(card.id).unwrap();
        let fsrs = FSRS::new(Some(&[])).unwrap();

        let last_date = last_review
            .as_ref()
            .map(|lr| lr.review_time)
            .unwrap_or(card.created_at);

        let now = Utc::now();
        let days_elapsed = (now - last_date).num_days() as u32;

        let next_states = fsrs
            .next_states(
                last_review.as_ref().map(|r| MemoryState {
                    difficulty: r.difficulty,
                    stability: r.stability,
                }),
                0.9,
                days_elapsed,
            )
            .unwrap();

        if n > 4 {
            panic!("invalid input {}", n); 
        }

        let next_state = match n {
            1 => next_states.again,
            2 => next_states.hard,
            3 => next_states.good,
            4 => next_states.easy,
            _ => panic!("invalid input"),
        };

        let revision = Review {
            _id: 0,
            card_id: card.id,
            difficulty: next_state.memory.difficulty,
            stability: next_state.memory.stability,
            interval: next_state.interval as u32,
            last_interval: days_elapsed,
            review_time: now,
        };

        self.store.add_review(revision).unwrap();
        let next_show_date = now + Duration::days(next_state.interval as i64);
        self.store.update_card(card.id, next_show_date).unwrap();
    }
}


#[derive(Debug)] 
pub struct CardSummary {
    pub id: ID,
    pub deck: String,
    pub title: String,
    pub next_show_date: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug)]
pub struct Card {
    pub id: ID,
    pub deck_id: ID,
    pub deck: String,
    pub title: String,
    pub desc: String,
    pub next_show_date: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug)]
pub struct Deck {
    pub id: ID,
    pub name: String,
}

pub struct Review {
    pub _id: ID,
    pub card_id: ID,
    pub interval: u32,
    pub last_interval: u32,
    pub review_time: DateTime<Utc>,
    pub stability: f32,
    pub difficulty: f32,
}

impl fmt::Display for Card {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}. \t[{}] {}\n\tnext show date: {}",
            self.id,
            self.deck.white().dimmed(),
            self.desc.bright_green().bold(),
            self.next_show_date
                .with_timezone(&chrono::Local)
                .format("%Y-%m-%d %H:%M")
                .to_string()
                .yellow()
        )
    }
}

fn parse_yaml_frontmatter(s: &str) -> std::collections::HashMap<String, String> {
    let mut frontmatter = std::collections::HashMap::new();
    let mut in_frontmatter = false;

    for line in s.lines() {
        if line.trim() == "---" {
            in_frontmatter = !in_frontmatter;
            continue;
        }

        if in_frontmatter {
            if let Some((key, value)) = line.split_once(':') {
                frontmatter.insert(key.trim().to_string(), value.trim().to_string());
            }
        }
    }

    frontmatter
}
