use std::collections::HashMap;
use std::sync::Arc;
use parking_lot::RwLock;
use lazy_static::lazy_static;
use log::{info, warn};

/// ThreadSafeCredentials provides a thread-safe way to store and access
/// game credentials (account name, ticket, characters_count and game language).
pub struct ThreadSafeCredentials {
    account_name: Arc<RwLock<String>>,
    characters_count: Arc<RwLock<String>>,
    ticket: Arc<RwLock<String>>,
    game_lang: Arc<RwLock<String>>,
    game_path: Arc<RwLock<String>>
}

impl ThreadSafeCredentials {
    /// Creates a new instance of ThreadSafeCredentials with empty strings.
    fn new() -> Self {
        Self {
            account_name: Arc::new(RwLock::new(String::new())),
            characters_count: Arc::new(RwLock::new(String::new())),
            ticket: Arc::new(RwLock::new(String::new())),
            game_lang: Arc::new(RwLock::new(String::new())),
            game_path: Arc::new(RwLock::new(String::new())),
        }
    }

    /// Sets the account name.
    ///
    /// This method acquires a write lock on the account_name field,
    /// which may block if there are current readers.
    ///
    /// # Arguments
    ///
    /// * `value` - A string slice that holds the account name to be set.
    pub fn set_account_name(&self, value: &str) {
        *self.account_name.write() = value.to_string();
    }

    /// Sets the characters_count.
    ///
    /// This method acquires a write lock on the characters_count field,
    /// which may block if there are current readers.
    ///
    /// # Arguments
    ///
    /// * `value` - A string slice that holds the characters_count to be set.
    pub fn set_characters_count(&self, value: &str) {
        *self.characters_count.write() = value.to_string();
    }


    /// Sets the ticket (GUID).
    ///
    /// This method acquires a write lock on the ticket field,
    /// which may block if there are current readers.
    ///
    /// # Arguments
    ///
    /// * `value` - A string slice that holds the ticket (GUID) to be set.
    pub fn set_ticket(&self, value: &str) {
        *self.ticket.write() = value.to_string();
    }

    /// Sets the game language.
    ///
    /// This method acquires a write lock on the game_lang field,
    /// which may block if there are current readers.
    ///
    /// # Arguments
    ///
    /// * `value` - A string slice that holds the game language to be set.
    pub fn set_game_lang(&self, value: &str) {
        *self.game_lang.write() = value.to_string();
    }

    /// Sets the game path.
    ///
    /// This method acquires a write lock on the game_path field,
    /// which may block if there are current readers.
    ///
    /// # Arguments
    ///
    /// * `value` - A string slice that holds the game path to be set.
    pub fn set_game_path(&self, value: &str) {
        *self.game_path.write() = value.to_string();
    }

    //////////////////////////////////////////////////////////////////////


    /// Gets the account name.
    ///
    /// This method acquires a read lock on the account_name field,
    /// which allows for multiple concurrent readers.
    ///
    /// # Returns
    ///
    /// A String containing the current account name.
    pub fn get_account_name(&self) -> String {
        self.account_name.read().clone()
    }

    /// Gets the account characters.
    ///
    /// This method acquires a read lock on the characters_count field,
    /// which allows for multiple concurrent readers.
    ///
    /// # Returns
    ///
    /// A String containing the current characters_count.
    pub fn get_characters_count(&self) -> String {
        self.characters_count.read().clone()
    }


    /// Gets the ticket (GUID).
    ///
    /// This method acquires a read lock on the ticket field,
    /// which allows for multiple concurrent readers.
    ///
    /// # Returns
    ///
    /// A String containing the current ticket (GUID).
    pub fn get_ticket(&self) -> String {
        self.ticket.read().clone()
    }

    /// Gets the game language.
    ///
    /// This method acquires a read lock on the game_lang field,
    /// which allows for multiple concurrent readers.
    ///
    /// # Returns
    ///
    /// A String containing the current game language.
    pub fn get_game_lang(&self) -> String {
        self.game_lang.read().clone()
    }


    /// Gets the game path.
    ///
    /// This method acquires a read lock on the game_path field,
    /// which allows for multiple concurrent readers.
    ///
    /// # Returns
    ///
    /// A String containing the current game path.
    pub fn get_game_path(&self) -> String {
        self.game_path.read().clone()
    }

}


/// Credentials for a single game instance.
#[derive(Clone)]
pub struct GameCredentials {
    pub account_name: String,
    pub characters_count: String,
    pub ticket: String,
    pub game_lang: String,
    pub game_path: String,
}

lazy_static! {
    #[doc = "GLOBAL_CREDENTIALS is a lazily-initialized static reference to ThreadSafeCredentials."]
    #[doc = "It's used to store and access game credentials globally across the application."]
    pub static ref GLOBAL_CREDENTIALS: ThreadSafeCredentials = ThreadSafeCredentials::new();

    /// Per-game credentials map keyed by PID for multi-client support.
    pub static ref GAME_CREDENTIALS_BY_PID: Arc<RwLock<HashMap<u32, GameCredentials>>> =
        Arc::new(RwLock::new(HashMap::new()));
}

/// Sets all credentials at once.
///
/// This function is a convenience wrapper that sets all three credential fields
/// (account name, ticket, and game language) in one call.
///
/// # Arguments
///
/// * `account_name` - A string slice that holds the account name to be set.
/// * `ticket` - A string slice that holds the ticket (GUID) to be set.
/// * `game_lang` - A string slice that holds the game language to be set.
pub fn set_credentials(account_name: &str, characters_count: &str, ticket: &str, game_lang: &str, game_path: &str) {
    GLOBAL_CREDENTIALS.set_account_name(account_name);
    GLOBAL_CREDENTIALS.set_characters_count(characters_count);
    GLOBAL_CREDENTIALS.set_ticket(ticket);
    GLOBAL_CREDENTIALS.set_game_lang(game_lang);
    GLOBAL_CREDENTIALS.set_game_path(game_path);
}

/// Stores credentials for a specific game PID (for multi-client support).
pub fn store_credentials_for_pid(pid: u32, account_name: &str, characters_count: &str, ticket: &str, game_lang: &str, game_path: &str) {
    let creds = GameCredentials {
        account_name: account_name.to_string(),
        characters_count: characters_count.to_string(),
        ticket: ticket.to_string(),
        game_lang: game_lang.to_string(),
        game_path: game_path.to_string(),
    };
    GAME_CREDENTIALS_BY_PID.write().insert(pid, creds);
    info!("Stored credentials for PID {} (account: {})", pid, account_name);
}

/// Gets credentials for a specific game PID.
/// Returns None if PID not found.
pub fn get_credentials_for_pid(pid: u32) -> Option<GameCredentials> {
    let map = GAME_CREDENTIALS_BY_PID.read();
    let result = map.get(&pid).cloned();
    if result.is_none() {
        // Log all known PIDs for debugging
        let known_pids: Vec<u32> = map.keys().cloned().collect();
        warn!(
            "Credential lookup failed for PID {}. Known PIDs: {:?}",
            pid,
            known_pids
        );
    }
    result
}

/// Removes credentials for a PID when game exits.
pub fn remove_credentials_for_pid(pid: u32) {
    let removed = GAME_CREDENTIALS_BY_PID.write().remove(&pid);
    if removed.is_some() {
        info!("Removed credentials for PID {}", pid);
    } else {
        warn!("Tried to remove credentials for PID {} but none were stored", pid);
    }
}

/// Returns true if any games are currently running (have stored credentials).
pub fn has_running_games() -> bool {
    !GAME_CREDENTIALS_BY_PID.read().is_empty()
}

/// Returns the number of currently running games.
pub fn running_game_count() -> usize {
    GAME_CREDENTIALS_BY_PID.read().len()
}

/// Clears all stored game credentials (for reset/cleanup).
pub fn clear_all_game_credentials() {
    GAME_CREDENTIALS_BY_PID.write().clear();
}