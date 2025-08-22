
pub mod game;
pub mod injection;
#[macro_use]
extern crate litcrypt;
use_litcrypt!();

pub use game::{
    run_game,
    get_game_status_receiver,
    is_game_running,
    reset_global_state,
    setup_logging,
    enable_file_logging,
    TeraLogger,
};
pub mod global_credentials;
pub use injection::{inject_agnitor};
pub mod config;