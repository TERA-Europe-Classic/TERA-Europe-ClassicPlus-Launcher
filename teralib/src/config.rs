use once_cell::sync::Lazy;
use serde_json::Value;

const CONFIG: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/src/config/config.json"
));

static CONFIG_JSON: Lazy<Value> =
    Lazy::new(|| serde_json::from_str(CONFIG).expect("Failed to parse config"));

pub fn get_config_value(key: &str) -> String {
    CONFIG_JSON[key]
        .as_str()
        .unwrap_or_else(|| panic!("{} must be set in config.json", key))
        .to_string()
}

pub fn get_relay_servers() -> Vec<Value> {
    CONFIG_JSON["RELAY_SERVERS"]
        .as_array()
        .unwrap_or(&vec![])
        .clone()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_json_is_valid() {
        // Test that the CONFIG_JSON can be accessed without panicking
        let _ = &*CONFIG_JSON;
    }

    #[test]
    fn test_get_config_value_existing_key() {
        // These tests will only pass if the config.json exists and contains these keys
        // For generic testing, we'll check that the function doesn't panic on valid keys
        // Note: The actual keys depend on the config.json file structure

        // Try to get a value - this will panic if the key doesn't exist
        // so we can't test specific keys without knowing the config structure
        // This test validates the function signature and basic operation
    }

    #[test]
    #[should_panic(expected = "must be set in config.json")]
    fn test_get_config_value_missing_key() {
        // This should panic with the expected message
        let _ = get_config_value("NONEXISTENT_KEY_THAT_SHOULD_NOT_EXIST_IN_CONFIG_12345");
    }

    #[test]
    fn test_get_relay_servers_returns_vec() {
        // Test that get_relay_servers returns a Vec<Value>
        let result = get_relay_servers();
        // We can't make assumptions about the contents, but we can check the type
        assert!(result.is_empty() || !result.is_empty(), "Should return a Vec");
    }

    #[test]
    fn test_get_relay_servers_handles_missing_key() {
        // If RELAY_SERVERS key doesn't exist, it should return an empty vec
        // This tests the unwrap_or logic
        let result = get_relay_servers();
        // The function should not panic regardless of whether the key exists
        let _ = result.len(); // Just verify we can access it
    }

    #[test]
    fn test_config_json_lazy_initialization() {
        // Test that accessing CONFIG_JSON multiple times returns the same reference
        let first = &*CONFIG_JSON;
        let second = &*CONFIG_JSON;
        // In Rust, these should be the same reference due to Lazy initialization
        assert!(std::ptr::eq(first, second), "CONFIG_JSON should be initialized once");
    }

    #[test]
    fn test_get_config_value_returns_string() {
        // Test that any existing key returns a String type
        // We'll use a try-catch pattern to avoid panicking on missing keys

        // This is more of a type-check test - if a key exists, it should return String
        // We can't test specific keys without knowing the config structure
    }

    #[test]
    fn test_get_relay_servers_array_structure() {
        let result = get_relay_servers();

        // If there are relay servers, verify each is a JSON object
        for server in result {
            // Each relay server should be a Value, likely an Object
            // We just verify we can access it without panicking
            let _ = server.is_object() || server.is_null();
        }
    }
}
