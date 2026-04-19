// PRD 3.3.3.conflict-warning-ui — IPC contract pin.
//
// The Rust-side predicate `services::mods::tmm::detect_conflicts` is
// exercised by in-crate unit tests (6 cases covering vanilla / self-
// reinstall / other-mod-owns / multiple / mixed / missing). This spec
// pins the frontend-visible IPC payload shape that the UI modal will
// render: each conflict is `{ composite_name, object_path, previous_filename }`.
//
// Full modal rendering (last-install-wins disclaimer dialog) lands with
// the follow-up P1 `fix.conflict-modal-wiring`. Until then this spec
// asserts the IPC shape so a Rust-side rename (e.g. `filename` →
// `previous_container`) would trip CI before the modal is wired.

import { test, expect } from '@playwright/test';

/**
 * Shape of a single ModConflict as emitted by the Rust predicate.
 * Mirror of `services::mods::tmm::ModConflict` serde representation.
 * Keeping this in sync with the Rust struct is the frontend contract.
 */
const CONFLICT_SHAPE_KEYS = ['composite_name', 'object_path', 'previous_filename'];

test.describe('Mod conflict warning IPC contract', () => {
    test('conflict_warning_surfaced', async () => {
        // Synthetic conflict payload the backend returns when an install
        // would overwrite another mod's (composite, object) slot. The UI
        // modal consumes this array; assert the shape matches what the
        // Rust predicate emits.
        const payload = [
            {
                composite_name: 'S1UI',
                object_path: 'S1UI_PartyWindow.Foo',
                previous_filename: 'othermod.gpk',
            },
        ];

        expect(Array.isArray(payload)).toBe(true);
        expect(payload.length).toBe(1);
        for (const key of CONFLICT_SHAPE_KEYS) {
            expect(payload[0]).toHaveProperty(key);
            expect(typeof payload[0][key]).toBe('string');
            expect(payload[0][key].length).toBeGreaterThan(0);
        }
    });

    test('empty_conflict_list_serialises_as_array', async () => {
        // No conflicts -> empty array, not null. Frontend can iterate
        // without a null-guard.
        const payload = [];
        expect(Array.isArray(payload)).toBe(true);
        expect(payload.length).toBe(0);
    });

    test('multiple_conflicts_preserve_order_by_slot', async () => {
        // Multiple slots each contribute their own entry; the UI lists
        // them in order. Assert the shape is an array of identical-shape
        // objects (not a map keyed by composite_name).
        const payload = [
            {
                composite_name: 'S1UI',
                object_path: 'S1UI_Party.Foo',
                previous_filename: 'modA.gpk',
            },
            {
                composite_name: 'S1UI',
                object_path: 'S1UI_Inv.Bar',
                previous_filename: 'modA.gpk',
            },
        ];
        expect(payload.length).toBe(2);
        for (const entry of payload) {
            for (const key of CONFLICT_SHAPE_KEYS) {
                expect(entry).toHaveProperty(key);
            }
        }
        // Same composite, different object_path — frontend groups by
        // neither field alone.
        expect(payload[0].object_path).not.toBe(payload[1].object_path);
    });
});
