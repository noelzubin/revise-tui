use std::borrow::Cow;

use color_eyre::Result;
use layout::Flex;
use ratatui::{prelude::*, widgets::*};
use tokio::sync::mpsc::UnboundedSender;
use tui_input::Input;

use super::Component;
use crate::{
    action::Action,
    app::{AppState, CardInfo, Focused},
    config::Config,
    usecase::CardSummary,
    utils::date_to_relative_string,
};

pub struct Home {
    command_tx: Option<UnboundedSender<Action>>,
    config: Config,
    components: Vec<Box<dyn Component>>,
}

impl Home {
    pub fn new() -> Self {
        Self {
            command_tx: None,
            config: Config::default(),
            components: vec![Box::new(ReviseTable {})],
        }
    }
}

const OFF_WHITE: Color = Color::Rgb(100, 100, 100);

impl Component for Home {
    fn register_action_handler(&mut self, tx: UnboundedSender<Action>) -> Result<()> {
        self.command_tx = Some(tx);
        Ok(())
    }

    fn register_config_handler(&mut self, config: Config) -> Result<()> {
        self.config = config;
        Ok(())
    }

    fn update(&mut self, action: Action) -> Result<Option<Action>> {
        match action {
            Action::Tick => {
                // add any logic here that should run on every tick
            }
            Action::Render => {
                // add any logic here that should run on every render
            }
            _ => {}
        }
        Ok(None)
    }

    fn draw(&mut self, app_state: &mut AppState, frame: &mut Frame, area: Rect) -> Result<()> {
        let a1: [Rect; 3] = Layout::vertical(vec![
            Constraint::Length(1),
            Constraint::Min(0),
            Constraint::Length(1),
        ])
        .areas(area);

        let a2: [Rect; 2] =
            Layout::horizontal(vec![Constraint::Length(30), Constraint::Min(0)]).areas(a1[1]);

        frame.render_widget(
            Paragraph::new(" REVISE 0.0.1 ".fg(Color::Black).bg(Color::Yellow).bold())
                .alignment(Alignment::Center),
            a1[0],
        );

        self.components[0].draw(app_state, frame, a2[1])?;

        let mut sidebar = DeckSidebar::default();
        sidebar.draw(app_state, frame, a2[0])?;

        let mut kb = Keybindings::default();
        kb.draw(app_state, frame, a1[2])?;

        Ok(())
    }
}

struct ReviseTable {}

impl Component for ReviseTable {
    fn draw(&mut self, app_state: &mut AppState, frame: &mut Frame, area: Rect) -> Result<()> {
        let has_card_info = app_state.card_info.is_some();

        let a1: [Rect; 2] =
            Layout::vertical(vec![Constraint::Fill(2), Constraint::Fill(1)]).areas(area);
        let a2: [Rect; 2] =
            Layout::horizontal(vec![Constraint::Fill(1), Constraint::Fill(1)]).areas(a1[1]);

        let cards_table_area = if has_card_info { a1[0] } else { area };

        render_card_table(app_state, frame, cards_table_area)?;

        if !has_card_info {
            return Ok(());
        }

        let CardInfo { card, reviews } = app_state.card_info.as_ref().unwrap();

        let block = Block::bordered()
            .title("|Card Info|")
            .padding(Padding::uniform(1))
            .border_style(Style::default().fg(OFF_WHITE));
        let card_info_area = block.inner(a2[0]);
        frame.render_widget(block, a2[0]);

        let [upper_area, lower_area] =
            Layout::vertical(vec![Constraint::Length(4), Constraint::Fill(1)])
                .areas(card_info_area);

        let info_table = Table::new(
            vec![
                Row::new(vec![
                    Cell::from("Name").style(Style::default().fg(Color::Cyan)),
                    Cell::from(card.title.clone()),
                ]),
                Row::new(vec![
                    Cell::from("Due Date").style(Style::default().fg(Color::Cyan)),
                    Cell::from(date_to_relative_string(card.next_show_date)),
                ]),
                Row::new(vec![
                    Cell::from("Created At").style(Style::default().fg(Color::Cyan)),
                    Cell::from(date_to_relative_string(card.created_at)),
                ]),
            ],
            vec![Constraint::Length(15), Constraint::Length(30)],
        );

        frame.render_widget(info_table, upper_area);

        let rows = reviews.iter().enumerate().map(|(ind, r)| {
            Row::new(vec![
                Cell::from((ind + 1).to_string()),
                Cell::from(date_to_relative_string(r.review_time)),
                Cell::from(r.interval.to_string()),
                Cell::from(format!("{:.2}", r.stability)),
                Cell::from(format!("{:.2}", r.difficulty)),
            ])
        });

        let revlog_table = Table::new(
            rows,
            vec![
                Constraint::Length(3),
                Constraint::Length(25),
                Constraint::Length(8),
                Constraint::Length(10),
                Constraint::Length(10),
            ],
        )
        .header(
            Row::new(vec![
                Cell::from("No."),
                Cell::from("Date"),
                Cell::from("Interval"),
                Cell::from("Stability"),
                Cell::from("Difficulty"),
            ])
            .style(Style::new().fg(Color::Cyan)),
        )
        .block(
            Block::new()
                .title("Previous Revisions:")
                .style(Style::new().fg(Color::White)),
        );

        frame.render_widget(revlog_table, lower_area);

        // Render revise card prompt
        if let Some(revise_card) = app_state.revise_card.as_ref() {
            let mut text = Text::from("");
            revise_card
                .next_dates
                .iter()
                .enumerate()
                .for_each(|(ind, interval)| {
                    text.push_line(format!(
                        "[{}] {}: in {} days",
                        ind + 1,
                        interval.0,
                        interval.1
                    ));
                });

            text.push_line(format!("[q] skip"));

            let revise_text = Paragraph::new(text)
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title(format!("|Revise card|"))
                        .padding(Padding::horizontal(2))
                        .style(Style::default().fg(Color::Yellow)),
                )
                .alignment(Alignment::Left);

            let area = center(area, Constraint::Length(50), Constraint::Length(9));

            frame.render_widget(Clear, area);
            frame.render_widget(revise_text, area);
        }

        // Render delete confirmation prompt
        if let Some(_deck_id) = app_state.confirm_delete_deck {
            let text = Text::from(vec![
                Line::from("Are you sure you want to delete this deck?"),
                Line::from("This will delete all cards in the deck."),
                Line::from(""),
                Line::from("[y] Yes  [n] No".yellow()),
            ]);

            let confirm_text = Paragraph::new(text)
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title("|Confirm Delete|")
                        .padding(Padding::horizontal(2))
                        .border_style(Style::default().fg(Color::Red)),
                )
                .alignment(Alignment::Center);

            let area = center(area, Constraint::Length(50), Constraint::Length(7));

            frame.render_widget(Clear, area);
            frame.render_widget(confirm_text, area);
        }

        let card_desc = Paragraph::new(card.desc.to_string()).style(Style::new().fg(Color::White));
        frame.render_widget(
            card_desc.block(
                Block::bordered()
                    .title("|Description|")
                    .padding(Padding::uniform(1))
                    .style(Style::default().fg(OFF_WHITE)),
            ),
            a2[1],
        );

        Ok(())
    }
}

fn render_card_table(app_state: &mut AppState, frame: &mut Frame, area: Rect) -> Result<()> {
    if app_state.cards.is_empty() {
        let empty_message = " No cards available. Press 'a' to add a new card. ";
        frame.render_widget(
            Paragraph::new(empty_message)
                .alignment(Alignment::Center)
                .block(Block::default().borders(Borders::ALL)),
            center(
                area,
                Constraint::Length((empty_message.len() + 2) as u16),
                Constraint::Length(3),
            ),
        );
        return Ok(());
    }

    let header = ["Title", "Due Date", "Deck", "Id"]
        .into_iter()
        .map(Cell::from)
        .collect::<Row>()
        .style(Style::default().fg(Color::Cyan))
        .height(1);

    let mut block = Block::bordered()
        .title("|Cards|")
        .padding(Padding::horizontal(1))
        .border_style(Style::default().fg(OFF_WHITE));

    if app_state.focused == Focused::Cards {
        block = block.border_style(Style::default().yellow().bold())
    }

    if app_state.cards_table_searching || !app_state.cards_table_input.value().is_empty() {
        block = block.title_bottom(get_input_line(&app_state.cards_table_input));
    }

    let cards_iter: Box<dyn Iterator<Item = &CardSummary>> =
        if app_state.cards_table_input.value().is_empty() {
            Box::new(app_state.cards.iter())
        } else {
            Box::new(
                app_state
                    .cards
                    .iter()
                    .filter(|c| c.title.contains(app_state.cards_table_input.value())),
            )
        };

    frame.render_stateful_widget(
        Table::new(
            cards_iter.map(|item| {
                Row::new(vec![
                    Cell::from(item.title.clone()),
                    Cell::from(date_to_relative_string(item.next_show_date)),
                    Cell::from(item.deck.clone()),
                    Cell::from(item.id.to_string()),
                ])
            }),
            vec![
                Constraint::Length(35),
                Constraint::Length(25),
                Constraint::Length(20),
                Constraint::Length(10),
            ],
        )
        .style(Style::new().fg(OFF_WHITE))
        .header(header)
        .row_highlight_style(Style::default().white())
        .block(block),
        area,
        &mut app_state.cards_table_state,
    );

    if app_state.cards_table_searching {
        let (x, y) = (
            area.x + app_state.cards_table_input.visual_cursor() as u16 + 10,
            area.bottom().saturating_sub(1),
        );

        frame.render_widget(
            Clear,
            Rect {
                x,
                y,
                width: 1,
                height: 1,
            },
        );

        frame.set_cursor_position(Position::new(x, y));
    }

    Ok(())
}

fn center(area: Rect, horizontal: Constraint, vertical: Constraint) -> Rect {
    let [area] = Layout::horizontal([horizontal])
        .flex(Flex::Center)
        .areas(area);
    let [area] = Layout::vertical([vertical]).flex(Flex::Center).areas(area);
    area
}

#[derive(Default)]
struct DeckSidebar {}

impl Component for DeckSidebar {
    fn draw(&mut self, app_state: &mut AppState, frame: &mut Frame, area: Rect) -> Result<()> {
        let mut items = vec![
            ListItem::new("Review"),
            ListItem::new("Suspended"),
            ListItem::new("All Collection"),
        ];

        items.extend(
            app_state.decks.iter().enumerate().map(|(ind, deck)| {
                ListItem::new(format!("[{}] {}", ind + 1, deck.name.to_string()))
            }),
        );

        let mut block = Block::bordered()
            .title("|Decks|")
            .border_style(Style::new().fg(OFF_WHITE));

        if app_state.focused == Focused::Sidebar {
            block = block.border_style(Style::new().yellow().bold())
        }

        let sidebar = List::new(items)
            .highlight_symbol("• ")
            .highlight_style(Style::default().bold())
            .block(block);

        frame.render_stateful_widget(sidebar, area, &mut app_state.decks_list_state);
        Ok(())
    }
}

#[derive(Default)]
struct Keybindings {}

impl Component for Keybindings {
    fn draw(&mut self, app_state: &mut AppState, frame: &mut Frame, area: Rect) -> Result<()> {
        let key_bindings = if app_state.focused == Focused::Sidebar {
            vec![
                ("Tab/l", "Focus cards"),
                ("k/j", "Previous/Next Collection"),
                ("d", "Delete deck"),
                ("q", "Quit"),
            ]
        } else {
            if app_state.revise_card.is_some() {
                vec![("1-4", "Revise card with <ease>"), ("q", "Quit")]
            } else {
                vec![
                    ("<n>", "Quick deck filter"),
                    ("Tab/h", "Focus decks"),
                    ("j/k", "Move down/up"),
                    ("a", "Add card"),
                    ("e", "Edit card"),
                    ("d", "Delete card"),
                    ("r", "Review card"),
                    ("s", "Suspend card"),
                    ("q", "Quit"),
                ]
            }
        };

        let line = Line::from(
            key_bindings
                .iter()
                .enumerate()
                .flat_map(|(i, (keys, desc))| {
                    vec![
                        "[".fg(OFF_WHITE),
                        Span::from((*keys).yellow()),
                        "→ ".fg(OFF_WHITE),
                        Span::from(*desc),
                        "]".fg(OFF_WHITE),
                        if i != key_bindings.len() - 1 { " " } else { "" }.into(),
                    ]
                })
                .collect::<Vec<Span>>(),
        );
        frame.render_widget(Paragraph::new(line.alignment(Alignment::Right)), area);
        Ok(())
    }
}

/// Returns the input line.
fn get_input_line(input: &Input) -> Line<'static> {
    Line::from(vec![
        "|".fg(OFF_WHITE),
        "search: ".yellow(),
        Span::from(Cow::Owned(input.value().to_string())),
        " ".into(),
        "|".fg(OFF_WHITE),
    ])
}
