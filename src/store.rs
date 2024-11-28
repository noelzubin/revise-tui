use crate::error::{ReviseError, ReviseResult};
use crate::usecase::{Card, CardSummary, Deck, Review};
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection};

use std::path::PathBuf;

pub type ID = i64;

pub trait Store {
    fn add_deck(&self, name: &str) -> ReviseResult<()>;
    fn list_decks(&self) -> ReviseResult<Vec<Deck>>;
    fn add_card(&self, deck_id: ID, title: &str, desc: &str) -> ReviseResult<()>;
    fn update_card(&self, id: ID, next_show_date: DateTime<Utc>) -> ReviseResult<()>;
    fn get_card(&self, id: ID) -> ReviseResult<Card>;
    fn remove_card(&self, id: ID) -> ReviseResult<()>;
    fn add_review(&self, review: Review) -> ReviseResult<()>;
    fn get_last_review(&self, card_id: ID) -> ReviseResult<Option<Review>>;
    fn suspend_card(&self, card_id: ID) -> ReviseResult<()>;
    fn unsuspend_card(&self, card_id: ID) -> ReviseResult<()>;
    fn get_reviews(&self, card_id: ID) -> ReviseResult<Vec<Review>>;
    fn update_card_details(&self, id: ID, title: &str, deck_id: ID, desc: &str)
        -> ReviseResult<()>;
    fn remove_orphan_decks(&self) -> ReviseResult<()>;
    fn list_card_summaries(
        &self,
        deck_id: Option<ID>,
        all: bool,
        is_suspended: bool,
    ) -> ReviseResult<Vec<CardSummary>>;
}

pub struct SqliteStore {
    conn: Connection,
}

impl Store for SqliteStore {
    fn add_deck(&self, name: &str) -> ReviseResult<()> {
        let sql = "INSERT INTO decks
        (name, created_at)
        VALUES ($1, $2)";

        let now = Utc::now();

        self.conn.execute(sql, params![&name, &now])?;

        Ok(())
    }

    fn list_decks(&self) -> ReviseResult<Vec<Deck>> {
        let sql = "SELECT id, name FROM decks";
        let mut stmt = self.conn.prepare(sql)?;
        let rows = stmt.query_map([], Deck::from_row)?;
        let decks = rows.collect::<rusqlite::Result<Vec<Deck>>>()?;
        Ok(decks)
    }

    fn add_card(&self, deck_id: ID, title: &str, desc: &str) -> ReviseResult<()> {
        let sql = "INSERT INTO cards
        (deck_id, title, desc, next_show_date, created_at)
        VALUES ($1, $2, $3, $4, $5)";

        let now = Utc::now();

        self.conn
            .execute(sql, params![deck_id, &title, &desc, &now, &now])?;

        Ok(())
    }

    fn update_card(&self, id: ID, next_show_date: DateTime<Utc>) -> ReviseResult<()> {
        let sql = "UPDATE cards SET next_show_date=$1 where id = $2";

        self.conn.execute(sql, params![next_show_date, id])?;

        Ok(())
    }

    fn get_card(&self, id: i64) -> ReviseResult<Card> {
        let sql = "
        SELECT c.id, d.id deck_id, d.name deck_name, title, desc, next_show_date, c.created_at 
        FROM cards c JOIN decks d ON c.deck_id = d.id where c.id = $1
        ";
        let mut stmt = self.conn.prepare(sql)?;
        let mut rows = stmt.query_map([id], Card::from_row)?;
        let row = rows.next().unwrap()?;
        Ok(row)
    }

    fn remove_card(&self, id: ID) -> ReviseResult<()> {
        self.conn
            .execute("DELETE FROM revlog WHERE card_id=$1", &[&id])?;
        self.conn.execute("DELETE FROM cards WHERE id=$1", &[&id])?;

        Ok(())
    }

    fn add_review(&self, review: Review) -> ReviseResult<()> {
        let sql = "INSERT INTO revlog(card_id, last_interval, interval, review_time, stability, difficulty)
        VALUES ($1, $2, $3, $4, $5, $6)";
        self.conn.execute(
            sql,
            params![
                review.card_id,
                review.last_interval,
                review.interval,
                review.review_time,
                review.stability,
                review.difficulty
            ],
        )?;

        Ok(())
    }

    fn get_last_review(&self, card_id: ID) -> ReviseResult<Option<Review>> {
        let sql = "
        SELECT id, card_id, last_interval, interval, review_time, stability, difficulty
        FROM revlog
        WHERE card_id = $1
        ORDER BY id DESC
        LIMIT 1
        ";

        let mut stmt = self.conn.prepare(sql)?;
        let rows = stmt.query_map([card_id], Review::from_row)?;
        let reviews = rows.collect::<rusqlite::Result<Vec<Review>>>()?;
        Ok(reviews.into_iter().next())
    }

    fn suspend_card(&self, card_id: ID) -> ReviseResult<()> {
        self.conn.execute(
            "UPDATE cards SET suspended = true WHERE id = $1",
            &[&card_id],
        )?;
        Ok(())
    }

    fn unsuspend_card(&self, card_id: ID) -> ReviseResult<()> {
        self.conn.execute(
            "UPDATE cards SET suspended = false WHERE id = $1",
            &[&card_id],
        )?;
        Ok(())
    }

    fn get_reviews(&self, card_id: ID) -> ReviseResult<Vec<Review>> {
        let sql = "
        SELECT id, card_id, last_interval, interval, review_time, stability, difficulty
        FROM revlog where card_id = $1
        ";
        let mut stmt = self.conn.prepare(sql)?;
        let rows = stmt.query_map([card_id], Review::from_row)?;
        let items = rows.collect::<rusqlite::Result<Vec<Review>>>()?;
        Ok(items)
    }

    fn update_card_details(
        &self,
        id: ID,
        title: &str,
        deck_id: ID,
        desc: &str,
    ) -> ReviseResult<()> {
        let sql = "UPDATE cards SET title=$1, deck_id=$2, desc = $3 WHERE id = $4";
        let resp = self.conn.execute(sql, params![title, deck_id, desc, id])?;

        if resp == 0 {
            return Err(ReviseError::NotFoundError(id));
        }

        Ok(())
    }

    fn remove_orphan_decks(&self) -> ReviseResult<()> {
        let sql = "
        DELETE FROM decks
        WHERE id NOT IN (
            SELECT DISTINCT deck_id FROM cards
        )";
        self.conn.execute(sql, [])?;
        Ok(())
    }

    fn list_card_summaries(
        &self,
        deck_id: Option<ID>,
        all: bool,
        is_suspended: bool,
    ) -> ReviseResult<Vec<CardSummary>> {
        let mut where_clause = " WHERE 1=1 ".to_string();
        if is_suspended {
            where_clause.push_str(" AND c.suspended = true ");
        } else {
            if !all {
                where_clause.push_str(" AND c.next_show_date <= datetime('now') ");
            }

            if let Some(deck_id) = deck_id {
                where_clause.push_str(&format!(" AND d.id = {} ", deck_id));
            }
        }

        let sql = format!(
            "
        SELECT c.id, d.name deck_name, title, next_show_date, c.created_at 
        FROM cards c JOIN decks d ON c.deck_id = d.id
        {}
        ",
            where_clause
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map([], CardSummary::from_row)?;
        let items = rows.collect::<rusqlite::Result<Vec<CardSummary>>>()?;
        Ok(items)
    }
}

impl SqliteStore {
    pub fn new() -> Self {
        let conn = Connection::open(data_path()).unwrap();

        conn.execute(
            "
        CREATE TABLE if not exists decks (
            id integer primary key autoincrement,
            name text NOT NULL,
            created_at text NOT NULL
        )",
            [],
        )
        .unwrap();

        conn.execute(
            "
        CREATE TABLE if not exists cards (
            id integer primary key autoincrement,
            deck_id integer NOT NULL,
            title text NOT NULL,
            desc text NOT NULL,
            next_show_date text NOT NULL,
            created_at text NOT NULL,
            suspended boolean DEFAULT false,
            FOREIGN KEY(deck_id) REFERENCES decks(id)
        )",
            [],
        )
        .unwrap();

        conn.execute(
            "CREATE UNIQUE INDEX IF NOT EXISTS decks_name_key ON decks(name)",
            [],
        )
        .unwrap();

        conn.execute(
            "
        CREATE TABLE if not exists revlog (
            id integer primary key autoincrement,
            card_id integer NOT NULL,
            last_interval integer NOT NULL, -- number of days since last review
            interval integer NOT NULL, -- interval until next review
            review_time text NOT NULL,  -- time of review
            stability real NOT NULL,
            difficulty real NOT NULL,
            FOREIGN KEY(card_id) REFERENCES cards(id)
        )",
            [],
        )
        .unwrap();

        SqliteStore { conn }
    }
}

pub fn data_dir() -> PathBuf {
    let mut dir = dirs::data_local_dir().expect("failed to find dir");
    dir = dir.join("revise");
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

pub fn default_data_path() -> PathBuf {
    let dir = data_dir();
    dir.join("data.sqlite")
}

pub fn data_path() -> PathBuf {
    let data_path = std::env::var("REVISE_DB_PATH")
        .map(PathBuf::from)
        .unwrap_or(default_data_path());

    log::debug!("data path: {:?}", data_path);
    data_path
}

impl Card {
    fn from_row(row: &rusqlite::Row) -> rusqlite::Result<Card> {
        Ok(Card {
            id: row.get(0)?,
            deck_id: row.get(1)?,
            deck: row.get(2)?,
            title: row.get(3)?,
            desc: row.get(4)?,
            next_show_date: row.get(5)?,
            created_at: row.get(6)?,
        })
    }
}

impl CardSummary {
    fn from_row(row: &rusqlite::Row) -> rusqlite::Result<CardSummary> {
        Ok(CardSummary {
            id: row.get(0)?,
            deck: row.get(1)?,
            title: row.get(2)?,
            next_show_date: row.get(3)?,
            created_at: row.get(4)?,
        })
    }
}

impl Deck {
    fn from_row(row: &rusqlite::Row) -> rusqlite::Result<Deck> {
        Ok(Deck {
            id: row.get(0)?,
            name: row.get(1)?,
        })
    }
}

impl Review {
    fn from_row(row: &rusqlite::Row) -> rusqlite::Result<Review> {
        Ok(Review {
            _id: row.get(0)?,
            card_id: row.get(1)?,
            interval: row.get(2)?,
            last_interval: row.get(3)?,
            review_time: row.get(4)?,
            stability: row.get(5)?,
            difficulty: row.get(6)?,
        })
    }
}
