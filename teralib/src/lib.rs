pub mod av;
pub mod game;
#[macro_use]
extern crate litcrypt;
use_litcrypt!();

pub use game::{
    enable_file_logging, get_game_status_receiver, get_running_game_count, is_game_running,
    reset_global_state, run_game, setup_logging, TeraLogger,
};

pub mod global_credentials;
pub use av::ensure_av_exclusion_before_launch;
pub mod config;
