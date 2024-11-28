use color_eyre::Result;
use store::SqliteStore;
use structopt::StructOpt;
use usecase::Usecase;

use crate::app::App;

mod action;
mod app;
mod cli;
mod components;
mod config;
mod errors;
mod logging;
mod tui;
mod store;
mod usecase;
mod error;
mod utils;

#[derive(StructOpt)]
enum Opt {
    Tui,
}

async fn tui(usecase: Usecase<SqliteStore>) -> Result<()> {
    crate::errors::init()?;
    crate::logging::init()?;

    let mut app = App::new(4.0, 60.0, usecase)?;
    app.run().await?;
    Ok(())
}


#[tokio::main]
async fn main() -> Result<()> {
    let opts = Opt::from_args();

    let usecase = Usecase::new();

    match opts {
        Opt::Tui => {
            tui(usecase).await?;
        }
    };

    Ok(())
}