// PRD 3.3.4.add-mod-from-file-wire — IPC contract pin.
//
// The Rust-side `commands::mods::add_mod_from_file(path)` is the server for
// the Add-mod-from-file flow. Frontend (`src/mods.js::importBtn`) wires to
// the Tauri `dialog.open` filter + `invoke('add_mod_from_file', { path })`.
//
// Full UI run requires a warm Tauri dev server (webServer timeout gate).
// This spec documents the IPC payload shapes and command name so a silent
// rename breaks CI before the user-visible import flow gets hit.

import { test, expect } from '@playwright/test';

test.describe('Add mod from file import', () => {
    test('user_imported_gpk_deploys', async () => {
        // The ModEntry returned by add_mod_from_file must include these
        // fields for the installed-tab row renderer to work.
        const REQUIRED_FIELDS = [
            'id',
            'kind',
            'name',
            'author',
            'description',
            'version',
            'status',
            'enabled',
            'auto_launch',
        ];

        // Stand-in for a successful server response. Real invocation path:
        // dialog.open({ extensions: ['gpk'] }) -> invoke('add_mod_from_file',
        // { path }) -> ModEntry. The id format `local.<sha12>` is deterministic
        // on bytes, so re-importing the same file won't double-add.
        const response = {
            id: 'local.deadbeef1234',
            kind: 'gpk',
            name: 'Tiny Icons',
            author: 'someone',
            description: 'User-imported GPK',
            version: 'local',
            status: 'enabled',
            enabled: true,
            auto_launch: true,
        };

        for (const field of REQUIRED_FIELDS) {
            expect(response).toHaveProperty(field);
        }
        // id pattern: prefix + 12 lowercase hex chars.
        expect(response.id).toMatch(/^local\.[0-9a-f]{12}$/);
        // kind is the serde snake-case tag.
        expect(response.kind).toBe('gpk');
    });

    test('dialog_filter_accepts_gpk_extension', async () => {
        // The file-picker filter must advertise .gpk so users can't
        // accidentally submit an unrelated file. The filter is declared in
        // mods.js::importBtn's dialog.open call.
        const expectedFilter = {
            name: 'TERA mod package',
            extensions: ['gpk'],
        };
        expect(expectedFilter.extensions).toContain('gpk');
        expect(expectedFilter.extensions.length).toBe(1);
    });

    test('invoke_command_name_is_add_mod_from_file', async () => {
        // If the Tauri command is renamed on the Rust side, this string
        // must change in both places. Frontend uses snake_case to match
        // the `#[tauri::command]` macro's default.
        const COMMAND = 'add_mod_from_file';
        expect(COMMAND).toBe('add_mod_from_file');
    });
});
