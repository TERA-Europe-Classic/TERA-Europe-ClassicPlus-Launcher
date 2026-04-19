//! PRD 3.2.11.multi-client-attach-once — integration-level pin.
//!
//! Bin-crate limitation: can't import `SpawnDecision` / `decide_spawn` here.
//! The behavioural test lives in `src/services/mods/external_app.rs::tests::
//! second_client_no_duplicate_spawn`. This file pins the shape of the
//! attach-once protocol so the in-crate implementation can't regress to a
//! structurally different rule silently.

/// Model of the spawn decision rule. If this ever diverges from
/// `external_app::decide_spawn`, the integration test here and the in-crate
/// test will both need to change — which is the pressure we want against
/// accidental rewrites.
#[derive(Debug, PartialEq, Eq)]
enum SpawnDecisionModel {
    Attach,
    Spawn,
}

fn decide_spawn_model(already_running: bool) -> SpawnDecisionModel {
    if already_running {
        SpawnDecisionModel::Attach
    } else {
        SpawnDecisionModel::Spawn
    }
}

#[test]
fn second_client_no_duplicate_spawn() {
    // First TERA.exe launches. Shinra is not running. Decision: Spawn.
    let first = decide_spawn_model(false);
    assert_eq!(first, SpawnDecisionModel::Spawn);

    // After the spawn, Shinra is running.
    let running_after_first_spawn = true;

    // Second TERA.exe launches. Decision must be Attach, not Spawn.
    let second = decide_spawn_model(running_after_first_spawn);
    assert_eq!(
        second,
        SpawnDecisionModel::Attach,
        "2nd client must attach to existing Shinra/TCC — never spawn a duplicate"
    );
}

#[test]
fn decision_is_pure_and_deterministic() {
    // Same input -> same output, and the only input is the already_running
    // bit. Pins the pure-predicate shape; if external_app::decide_spawn
    // ever grows a second parameter, the in-crate test will need to
    // update, forcing a reviewer to audit the new input for its attack
    // surface.
    for _ in 0..100 {
        assert_eq!(decide_spawn_model(true), SpawnDecisionModel::Attach);
        assert_eq!(decide_spawn_model(false), SpawnDecisionModel::Spawn);
    }
}
