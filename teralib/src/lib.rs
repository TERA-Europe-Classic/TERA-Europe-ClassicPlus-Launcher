
pub mod game;
pub mod av;
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
    subscribe_game_events,
};

pub mod global_credentials;
pub use av::{ensure_av_exclusion_before_launch};
pub mod config;