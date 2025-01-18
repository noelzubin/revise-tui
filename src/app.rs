use color_eyre::Result;
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    prelude::Rect,
    widgets::{ListState, TableState},
};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use tracing::{debug, info};
use tui_input::{backend::crossterm::EventHandler, Input};

use crate::{
    action::Action,
    components::{home::Home, Component},
    config::Config,
    store::{SqliteStore, ID},
    tui::{Event, Tui},
    usecase::{Card, CardSummary, Deck, Review, Usecase},
};

pub struct ReviseCardDetails {
    pub id: ID,
    pub next_dates: Vec<(&'static str, f32)>,
}

pub struct AppState {
    pub decks: Vec<Deck>,
    pub cards: Vec<CardSummary>,
    pub focused: Focused,
    pub card_info: Option<CardInfo>,
    pub cards_table_state: TableState,
    pub cards_table_searching: bool,
    pub cards_table_input: Input,
    pub decks_list_state: ListState,
    pub revise_card: Option<ReviseCardDetails>,
}

#[derive(PartialEq, Eq)]
pub enum Focused {
    Sidebar,
    Cards,
}

pub struct CardInfo {
    pub card: Card,
    pub reviews: Vec<Review>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            decks: Vec::new(),
            cards: Vec::new(),
            focused: Focused::Cards,
            card_info: None,
            cards_table_state: TableState::default().with_selected(Some(0)),
            cards_table_input: Input::default(),
            cards_table_searching: false,
            decks_list_state: ListState::default().with_selected(Some(0)),
            revise_card: None,
        }
    }
}

pub struct App {
    config: Config,
    tick_rate: f64,
    frame_rate: f64,
    components: Vec<Box<dyn Component>>,
    should_quit: bool,
    should_suspend: bool,
    mode: Mode,
    last_tick_key_events: Vec<KeyEvent>,
    action_tx: mpsc::UnboundedSender<Action>,
    action_rx: mpsc::UnboundedReceiver<Action>,
    usecase: Usecase<SqliteStore>,
    state: AppState,
}

#[derive(Default, Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Mode {
    #[default]
    Home,
}

impl App {
    pub fn new(tick_rate: f64, frame_rate: f64, usecase: Usecase<SqliteStore>) -> Result<Self> {
        let (action_tx, action_rx) = mpsc::unbounded_channel();
        Ok(Self {
            tick_rate,
            frame_rate,
            components: vec![
                Box::new(Home::new()),
                // Box::new(FpsCounter::default())
            ],
            should_quit: false,
            should_suspend: false,
            config: Config::new()?,
            mode: Mode::Home,
            last_tick_key_events: Vec::new(),
            action_tx,
            action_rx,
            usecase,
            state: AppState::default(),
        })
    }

    pub fn get_cards_in_deck(&self, ind: usize, decks: &Vec<Deck>) -> Vec<CardSummary> {
        if ind == 0 {
            return self.usecase.list_card_summaries(None, false, false);
        }

        if ind == 1 {
            return self.usecase.list_card_summaries(None, true, true);
        }

        if ind == 2 {
            return self.usecase.list_card_summaries(None, true, false);
        }

        let deck = decks.get(ind - 3).expect("There is atleast one deck");
        return self
            .usecase
            .list_card_summaries(Some(deck.id), false, false);
    }

    pub async fn run(&mut self) -> Result<()> {
        let mut tui = Tui::new()?
            // .mouse(true) // uncomment this line to enable mouse support
            .tick_rate(self.tick_rate)
            .frame_rate(self.frame_rate);

        self.state.decks = self.usecase.list_decks();
        self.state.cards = self.get_cards_in_deck(0, &self.state.decks);

        if let Some(selected_row) = self.state.cards_table_state.selected() {
            if let Some(card) = self.state.cards.get(selected_row) {
                let card = self.usecase.get_card(card.id);
                let reviews = self.usecase.get_reviews(card.id);
                self.state.card_info = Some(CardInfo { card, reviews });
            }
        }

        tui.enter()?;

        for component in self.components.iter_mut() {
            component.register_action_handler(self.action_tx.clone())?;
        }
        for component in self.components.iter_mut() {
            component.register_config_handler(self.config.clone())?;
        }
        for component in self.components.iter_mut() {
            component.init(tui.size()?)?;
        }

        let action_tx = self.action_tx.clone();
        loop {
            self.handle_events(&mut tui).await?;
            self.handle_actions(&mut tui)?;
            if self.should_suspend {
                tui.suspend()?;
                action_tx.send(Action::Resume)?;
                action_tx.send(Action::ClearScreen)?;
                // tui.mouse(true);
                tui.enter()?;
            } else if self.should_quit {
                tui.stop()?;
                break;
            }
        }
        tui.exit()?;
        Ok(())
    }

    async fn handle_events(&mut self, tui: &mut Tui) -> Result<()> {
        let Some(event) = tui.next_event().await else {
            return Ok(());
        };
        let action_tx = self.action_tx.clone();
        match event {
            Event::Quit => action_tx.send(Action::Quit)?,
            Event::Tick => action_tx.send(Action::Tick)?,
            Event::Render => action_tx.send(Action::Render)?,
            Event::Resize(x, y) => action_tx.send(Action::Resize(x, y))?,
            Event::Key(key) => self.handle_key_event(key, tui)?,
            _ => {}
        }
        for component in self.components.iter_mut() {
            if let Some(action) = component.handle_events(Some(event.clone()))? {
                action_tx.send(action)?;
            }
        }
        Ok(())
    }

    fn handle_key_event(&mut self, key: KeyEvent, tui: &mut Tui) -> Result<()> {
        let action_tx = self.action_tx.clone();
        let Some(keymap) = self.config.keybindings.get(&self.mode) else {
            return Ok(());
        };
        match keymap.get(&vec![key]) {
            Some(action) => {
                info!("Got action: {action:?}");
                action_tx.send(action.clone())?;
            }
            _ => {
                // If the key was not handled as a single key action,
                // then consider it for multi-key combinations.
                self.last_tick_key_events.push(key);

                // Check for multi-key combinations
                if let Some(action) = keymap.get(&self.last_tick_key_events) {
                    info!("Got action: {action:?}");
                    action_tx.send(action.clone())?;
                }
            }
        }

        if self.state.focused == Focused::Cards {
            if self.state.cards_table_searching {
                match key.code {
                    KeyCode::Enter | KeyCode::Esc => {
                        self.state.cards_table_searching = false;
                    }
                    _ => {
                        self.state
                            .cards_table_input
                            .handle_event(&crossterm::event::Event::Key(key));
                    }
                }
            } else if self.state.revise_card.is_some() {
                let card_id = self.state.revise_card.as_ref().unwrap().id;
                match key.code {
                    KeyCode::Char('1') => {
                        self.usecase.revise_card(card_id, 1);
                        self.state.revise_card = None;

                        self.state.cards = self.get_cards_in_deck(
                            self.state.decks_list_state.selected().unwrap(),
                            &self.state.decks,
                        );
                    }
                    KeyCode::Char('2') => {
                        self.usecase.revise_card(card_id, 2);
                        self.state.revise_card = None;

                        self.state.cards = self.get_cards_in_deck(
                            self.state.decks_list_state.selected().unwrap(),
                            &self.state.decks,
                        );
                    }
                    KeyCode::Char('3') => {
                        self.usecase.revise_card(card_id, 3);
                        self.state.revise_card = None;

                        self.state.cards = self.get_cards_in_deck(
                            self.state.decks_list_state.selected().unwrap(),
                            &self.state.decks,
                        );
                    }
                    KeyCode::Char('4') => {
                        self.usecase.revise_card(card_id, 4);
                        self.state.revise_card = None;

                        self.state.cards = self.get_cards_in_deck(
                            self.state.decks_list_state.selected().unwrap(),
                            &self.state.decks,
                        );
                    }
                    KeyCode::Esc => {
                        self.state.revise_card = None;
                    }
                    _ => {}
                }
            } else {
                match key.code {
                    KeyCode::Char(n @ '1'..='9') => {
                        n.to_digit(10).map(|n| {
                            if (n as usize) > self.state.decks.len() {
                                return;
                            }
                            self.state.decks_list_state.select(Some(n as usize + 2));
                            self.state.cards = self.get_cards_in_deck(
                                self.state.decks_list_state.selected().unwrap(),
                                &self.state.decks,
                            );
                        });
                    }
                    KeyCode::Tab => {
                        self.state.focused = Focused::Sidebar;
                    }
                    KeyCode::Char('k') => {
                        self.state.cards_table_state.select_previous();
                    }
                    KeyCode::Char('j') => {
                        self.state.cards_table_state.select_next();
                    }
                    KeyCode::Char('/') => {
                        self.state.cards_table_searching = true;
                    }
                    KeyCode::Char('d') => {
                        if let Some(selected_row) = self.state.cards_table_state.selected() {
                            if let Some(card) = self.state.cards.get(selected_row) {
                                self.usecase.remove_card(card.id);
                            }
                        }
                        self.state.decks = self.usecase.list_decks();
                        self.state.cards = self.get_cards_in_deck(
                            self.state.decks_list_state.selected().unwrap(),
                            &self.state.decks,
                        );
                    }

                    KeyCode::Char('e') => {
                        if let Some(selected_row) = self.state.cards_table_state.selected() {
                            if let Some(card) = self.state.cards.get(selected_row) {
                                tui.exit().unwrap();
                                self.usecase.edit_card(card.id);
                                tui.enter()?;
                                tui.terminal.clear().unwrap();
                                self.state.decks = self.usecase.list_decks();
                                self.state.cards = self.get_cards_in_deck(
                                    self.state.decks_list_state.selected().unwrap(),
                                    &self.state.decks,
                                );
                            }
                        }
                    }

                    _ => {}
                }

                let is_suspended = self.state.decks_list_state.selected().unwrap() == 1;

                if is_suspended {
                    match key.code {
                        KeyCode::Char('s') => {
                            // If in supended list, unsuspend the cards.
                            if let Some(selected_row) = self.state.cards_table_state.selected() {
                                if let Some(card) = self.state.cards.get(selected_row) {
                                    self.usecase.unsuspend_card(card.id);
                                    self.state.cards = self.get_cards_in_deck(
                                        self.state.decks_list_state.selected().unwrap(),
                                        &self.state.decks,
                                    );
                                }
                            }
                        }
                        _ => {}
                    }
                } else {
                    match key.code {
                        KeyCode::Char('a') => {
                            tui.exit().unwrap();
                            let current_deck = if self.state.decks_list_state.selected().unwrap() >= 3 {
                                let deck_index = self.state.decks_list_state.selected().unwrap() - 3;
                                Some(self.state.decks[deck_index].name.as_str())
                            } else {
                                None
                            };
                            self.usecase.add_card(current_deck);
                            tui.enter()?;
                            tui.terminal.clear().unwrap();
                            self.state.decks = self.usecase.list_decks();
                            self.state.cards = self.get_cards_in_deck(
                                self.state.decks_list_state.selected().unwrap(),
                                &self.state.decks,
                            );
                        }

                        KeyCode::Char('e') => {
                            if let Some(selected_row) = self.state.cards_table_state.selected() {
                                if let Some(card) = self.state.cards.get(selected_row) {
                                    tui.exit().unwrap();
                                    self.usecase.edit_card(card.id);
                                    tui.enter()?;
                                    tui.terminal.clear().unwrap();
                                    self.state.decks = self.usecase.list_decks();
                                    self.state.cards = self.get_cards_in_deck(
                                        self.state.decks_list_state.selected().unwrap(),
                                        &self.state.decks,
                                    );
                                }
                            }
                        }

                        KeyCode::Char('r') => {
                            if let Some(selected_row) = self.state.cards_table_state.selected() {
                                if let Some(card) = self.state.cards.get(selected_row) {
                                    self.state.revise_card = Some(ReviseCardDetails {
                                        id: card.id,
                                        next_dates: self.usecase.get_next_dates(card),
                                    });
                                }
                            }
                        }

                        KeyCode::Char('s') => {
                            // If in supended list, unsuspend the cards.
                            if let Some(selected_row) = self.state.cards_table_state.selected() {
                                if let Some(card) = self.state.cards.get(selected_row) {
                                    self.usecase.suspend_card(card.id);
                                    self.state.cards = self.get_cards_in_deck(
                                        self.state.decks_list_state.selected().unwrap(),
                                        &self.state.decks,
                                    );
                                }
                            }
                        }

                        _ => {}
                    }
                }
            }
        } else if self.state.focused == Focused::Sidebar {
            match key.code {
                KeyCode::Tab => {
                    self.state.focused = Focused::Cards;
                }
                KeyCode::Char('k') => {
                    self.state.decks_list_state.select_previous();
                    self.state.cards = self.get_cards_in_deck(
                        self.state.decks_list_state.selected().unwrap(),
                        &self.state.decks,
                    );
                }
                KeyCode::Char('j') => {
                    self.state.decks_list_state.select_next();
                    let selected = self.state.decks_list_state.selected().unwrap().clamp(0, &self.state.decks.len() + 2);
                    self.state.cards = self.get_cards_in_deck(
                        selected,
                        &self.state.decks,
                    );
                }
                KeyCode::Char('d') => {
                    if self.state.decks_list_state.selected().unwrap() >= 3 {
                        let deck_index = self.state.decks_list_state.selected().unwrap() - 3;
                        if let Some(deck) = self.state.decks.get(deck_index) {
                            self.usecase.delete_deck(deck.id);
                            self.state.decks = self.usecase.list_decks();
                            self.state.decks_list_state.select(Some(0));
                            self.state.cards = self.get_cards_in_deck(0, &self.state.decks);
                        }
                    }
                }
                _ => {}
            }
        }

        if let Some(selected_row) = self.state.cards_table_state.selected() {
            if let Some(card) = self.state.cards.get(selected_row) {
                let card = self.usecase.get_card(card.id);
                let reviews = self.usecase.get_reviews(card.id);
                self.state.card_info = Some(CardInfo { card, reviews });
            }
        }

        Ok(())
    }

    fn handle_actions(&mut self, tui: &mut Tui) -> Result<()> {
        while let Ok(action) = self.action_rx.try_recv() {
            if action != Action::Tick && action != Action::Render {
                debug!("{action:?}");
            }
            match action {
                Action::Tick => {
                    self.last_tick_key_events.drain(..);
                }
                Action::Quit => self.should_quit = true,
                Action::Suspend => self.should_suspend = true,
                Action::Resume => self.should_suspend = false,
                Action::ClearScreen => tui.terminal.clear()?,
                Action::Resize(w, h) => self.handle_resize(tui, w, h)?,
                Action::Render => self.render(tui)?,
                _ => {}
            }
            for component in self.components.iter_mut() {
                if let Some(action) = component.update(action.clone())? {
                    self.action_tx.send(action)?
                };
            }
        }
        Ok(())
    }

    fn handle_resize(&mut self, tui: &mut Tui, w: u16, h: u16) -> Result<()> {
        tui.resize(Rect::new(0, 0, w, h))?;
        self.render(tui)?;
        Ok(())
    }

    fn render(&mut self, tui: &mut Tui) -> Result<()> {
        tui.draw(|frame| {
            for component in self.components.iter_mut() {
                if let Err(err) = component.draw(&mut self.state, frame, frame.area()) {
                    let _ = self
                        .action_tx
                        .send(Action::Error(format!("Failed to draw: {:?}", err)));
                }
            }
        })?;
        Ok(())
    }
}
