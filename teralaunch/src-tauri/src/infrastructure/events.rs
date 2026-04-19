//! Event emitter abstraction for testability.
//!
//! This module provides a trait for emitting events, allowing the application
//! to use mock implementations in tests while using Tauri's event system in production.

use serde::Serialize;
use std::sync::{Arc, Mutex};

/// Trait for emitting events, allowing mocking in tests.
///
/// This abstracts over Tauri's Window::emit and AppHandle::emit_all methods,
/// enabling unit tests without a full Tauri runtime.
pub trait EventEmitter: Send + Sync {
    /// Emit an event with a serializable payload.
    fn emit<S: Serialize + Clone>(&self, event: &str, payload: S) -> Result<(), String>;

    /// Emit an event to all windows.
    fn emit_all<S: Serialize + Clone>(&self, event: &str, payload: S) -> Result<(), String>;
}

/// Recorded event for testing.
#[derive(Debug, Clone)]
pub struct RecordedEvent {
    pub event: String,
    pub payload: String,
}

/// Mock event emitter that records all emitted events.
///
/// This is available in both test and non-test builds to support
/// testing in other modules.
pub struct MockEventEmitter {
    events: Arc<Mutex<Vec<RecordedEvent>>>,
    should_fail: bool,
}

impl MockEventEmitter {
    /// Create a new MockEventEmitter that records events.
    pub fn new() -> Self {
        Self {
            events: Arc::new(Mutex::new(Vec::new())),
            should_fail: false,
        }
    }

    /// Create a MockEventEmitter that always fails on emit.
    pub fn failing() -> Self {
        Self {
            events: Arc::new(Mutex::new(Vec::new())),
            should_fail: true,
        }
    }

    /// Get all recorded events.
    pub fn events(&self) -> Vec<RecordedEvent> {
        self.events.lock().unwrap().clone()
    }

    /// Get the count of recorded events.
    pub fn event_count(&self) -> usize {
        self.events.lock().unwrap().len()
    }

    /// Clear all recorded events.
    pub fn clear(&self) {
        self.events.lock().unwrap().clear();
    }

    /// Check if any event with the given name was emitted.
    pub fn has_event(&self, event_name: &str) -> bool {
        self.events
            .lock()
            .unwrap()
            .iter()
            .any(|e| e.event == event_name)
    }

    /// Get all events with the given name.
    pub fn get_events(&self, event_name: &str) -> Vec<RecordedEvent> {
        self.events
            .lock()
            .unwrap()
            .iter()
            .filter(|e| e.event == event_name)
            .cloned()
            .collect()
    }
}

impl Default for MockEventEmitter {
    fn default() -> Self {
        Self::new()
    }
}

impl EventEmitter for MockEventEmitter {
    fn emit<S: Serialize + Clone>(&self, event: &str, payload: S) -> Result<(), String> {
        if self.should_fail {
            return Err("Mock emit failure".to_string());
        }
        let payload_json =
            serde_json::to_string(&payload).unwrap_or_else(|_| "<serialization error>".to_string());
        self.events.lock().unwrap().push(RecordedEvent {
            event: event.to_string(),
            payload: payload_json,
        });
        Ok(())
    }

    fn emit_all<S: Serialize + Clone>(&self, event: &str, payload: S) -> Result<(), String> {
        self.emit(event, payload)
    }
}

/// Tauri Window-based event emitter.
///
/// Wraps a Tauri Window to implement the EventEmitter trait.
pub struct TauriWindowEmitter {
    window: tauri::Window,
}

impl TauriWindowEmitter {
    /// Create a new TauriWindowEmitter wrapping the given window.
    #[cfg(not(tarpaulin_include))]
    pub fn new(window: tauri::Window) -> Self {
        Self { window }
    }
}

impl EventEmitter for TauriWindowEmitter {
    #[cfg(not(tarpaulin_include))]
    fn emit<S: Serialize + Clone>(&self, event: &str, payload: S) -> Result<(), String> {
        use tauri::Emitter;
        self.window.emit(event, payload).map_err(|e| e.to_string())
    }

    #[cfg(not(tarpaulin_include))]
    fn emit_all<S: Serialize + Clone>(&self, event: &str, payload: S) -> Result<(), String> {
        use tauri::Emitter;
        // In v2, Emitter::emit broadcasts to all listeners — no separate emit_all.
        self.window.emit(event, payload).map_err(|e| e.to_string())
    }
}

/// Tauri AppHandle-based event emitter.
///
/// Wraps a Tauri AppHandle to implement the EventEmitter trait.
pub struct TauriAppEmitter {
    app_handle: tauri::AppHandle,
}

impl TauriAppEmitter {
    /// Create a new TauriAppEmitter wrapping the given app handle.
    #[cfg(not(tarpaulin_include))]
    pub fn new(app_handle: tauri::AppHandle) -> Self {
        Self { app_handle }
    }
}

impl EventEmitter for TauriAppEmitter {
    #[cfg(not(tarpaulin_include))]
    fn emit<S: Serialize + Clone>(&self, event: &str, payload: S) -> Result<(), String> {
        use tauri::Emitter;
        self.app_handle
            .emit(event, payload)
            .map_err(|e| e.to_string())
    }

    #[cfg(not(tarpaulin_include))]
    fn emit_all<S: Serialize + Clone>(&self, event: &str, payload: S) -> Result<(), String> {
        use tauri::Emitter;
        self.app_handle
            .emit(event, payload)
            .map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mock_emitter_records_events() {
        let emitter = MockEventEmitter::new();

        emitter.emit("test_event", "payload").unwrap();
        emitter.emit("another_event", 42).unwrap();

        let events = emitter.events();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].event, "test_event");
        assert_eq!(events[1].event, "another_event");
    }

    #[test]
    fn mock_emitter_serializes_payload() {
        let emitter = MockEventEmitter::new();

        #[derive(Serialize, Clone)]
        struct TestPayload {
            value: i32,
        }

        emitter.emit("event", TestPayload { value: 123 }).unwrap();

        let events = emitter.events();
        assert!(events[0].payload.contains("123"));
    }

    #[test]
    fn mock_emitter_can_fail() {
        let emitter = MockEventEmitter::failing();

        let result = emitter.emit("event", "payload");
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "Mock emit failure");
    }

    #[test]
    fn mock_emitter_clear() {
        let emitter = MockEventEmitter::new();

        emitter.emit("event", "payload").unwrap();
        assert_eq!(emitter.event_count(), 1);

        emitter.clear();
        assert_eq!(emitter.event_count(), 0);
    }

    #[test]
    fn mock_emitter_emit_all_delegates_to_emit() {
        let emitter = MockEventEmitter::new();

        emitter.emit_all("broadcast", "data").unwrap();

        let events = emitter.events();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event, "broadcast");
    }

    #[test]
    fn mock_emitter_has_event() {
        let emitter = MockEventEmitter::new();

        emitter.emit("test_event", "payload").unwrap();

        assert!(emitter.has_event("test_event"));
        assert!(!emitter.has_event("other_event"));
    }

    #[test]
    fn mock_emitter_get_events() {
        let emitter = MockEventEmitter::new();

        emitter.emit("progress", "data1").unwrap();
        emitter.emit("complete", "done").unwrap();
        emitter.emit("progress", "data2").unwrap();

        let progress_events = emitter.get_events("progress");
        assert_eq!(progress_events.len(), 2);
        assert_eq!(progress_events[0].event, "progress");
        assert_eq!(progress_events[1].event, "progress");
    }

    #[test]
    fn mock_emitter_default() {
        let emitter = MockEventEmitter::default();
        assert_eq!(emitter.event_count(), 0);
    }

    #[test]
    fn recorded_event_creation_and_clone() {
        let event = RecordedEvent {
            event: "test_event".to_string(),
            payload: "test_payload".to_string(),
        };

        let cloned = event.clone();
        assert_eq!(event.event, cloned.event);
        assert_eq!(event.payload, cloned.payload);
    }

    #[test]
    fn mock_emitter_complex_nested_payload() {
        #[derive(Serialize, Clone)]
        struct Inner {
            value: String,
            number: i32,
        }

        #[derive(Serialize, Clone)]
        struct ComplexPayload {
            items: Vec<Inner>,
            metadata: Inner,
            tags: Vec<String>,
        }

        let emitter = MockEventEmitter::new();
        let payload = ComplexPayload {
            items: vec![
                Inner {
                    value: "item1".to_string(),
                    number: 1,
                },
                Inner {
                    value: "item2".to_string(),
                    number: 2,
                },
            ],
            metadata: Inner {
                value: "meta".to_string(),
                number: 99,
            },
            tags: vec!["tag1".to_string(), "tag2".to_string()],
        };

        emitter.emit("complex", payload).unwrap();

        let events = emitter.events();
        assert_eq!(events.len(), 1);
        assert!(events[0].payload.contains("item1"));
        assert!(events[0].payload.contains("item2"));
        assert!(events[0].payload.contains("meta"));
        assert!(events[0].payload.contains("tag1"));
        assert!(events[0].payload.contains("tag2"));
    }

    #[test]
    fn mock_emitter_thread_safety() {
        use std::thread;

        let emitter = Arc::new(MockEventEmitter::new());
        let mut handles = vec![];

        // Spawn 10 threads, each emitting 10 events
        for i in 0..10 {
            let emitter_clone = Arc::clone(&emitter);
            let handle = thread::spawn(move || {
                for j in 0..10 {
                    emitter_clone
                        .emit("thread_event", format!("thread-{}-event-{}", i, j))
                        .unwrap();
                }
            });
            handles.push(handle);
        }

        // Wait for all threads to complete
        for handle in handles {
            handle.join().unwrap();
        }

        // Verify we have all 100 events
        assert_eq!(emitter.event_count(), 100);
        assert_eq!(emitter.get_events("thread_event").len(), 100);
    }

    #[test]
    fn get_events_returns_empty_for_unknown_event() {
        let emitter = MockEventEmitter::new();

        emitter.emit("known_event", "data").unwrap();

        let unknown_events = emitter.get_events("unknown_event");
        assert_eq!(unknown_events.len(), 0);
    }

    #[test]
    fn has_event_returns_false_for_empty_emitter() {
        let emitter = MockEventEmitter::new();

        assert!(!emitter.has_event("any_event"));
    }

    #[test]
    fn event_count_increments_correctly() {
        let emitter = MockEventEmitter::new();

        assert_eq!(emitter.event_count(), 0);

        emitter.emit("event1", "data").unwrap();
        assert_eq!(emitter.event_count(), 1);

        emitter.emit("event2", "data").unwrap();
        assert_eq!(emitter.event_count(), 2);

        emitter.emit("event3", "data").unwrap();
        assert_eq!(emitter.event_count(), 3);

        emitter.emit_all("event4", "data").unwrap();
        assert_eq!(emitter.event_count(), 4);
    }

    #[test]
    fn mock_emitter_multiple_emissions_same_event() {
        let emitter = MockEventEmitter::new();

        for i in 0..5 {
            emitter.emit("repeated", i).unwrap();
        }

        assert_eq!(emitter.event_count(), 5);
        assert_eq!(emitter.get_events("repeated").len(), 5);
        assert!(emitter.has_event("repeated"));

        let events = emitter.get_events("repeated");
        for (i, event) in events.iter().enumerate() {
            assert_eq!(event.event, "repeated");
            assert!(event.payload.contains(&i.to_string()));
        }
    }

    #[test]
    fn mock_emitter_failing_does_not_record() {
        let emitter = MockEventEmitter::failing();

        let result = emitter.emit("event", "payload");
        assert!(result.is_err());

        // Should not record when failing
        assert_eq!(emitter.event_count(), 0);
        assert!(!emitter.has_event("event"));
    }

    #[test]
    fn mock_emitter_clear_resets_state() {
        let emitter = MockEventEmitter::new();

        emitter.emit("event1", "data").unwrap();
        emitter.emit("event2", "data").unwrap();
        emitter.emit("event3", "data").unwrap();

        assert_eq!(emitter.event_count(), 3);
        assert!(emitter.has_event("event1"));

        emitter.clear();

        assert_eq!(emitter.event_count(), 0);
        assert!(!emitter.has_event("event1"));
        assert_eq!(emitter.get_events("event1").len(), 0);
    }
}
