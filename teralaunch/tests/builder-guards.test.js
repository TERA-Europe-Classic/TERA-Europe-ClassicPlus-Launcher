import { describe, expect, it } from 'vitest';
import { readFileSync } from 'node:fs';

const builderSource = readFileSync(new URL('../../builder.ps1', import.meta.url), 'utf8');

describe('builder NSIS detection', () => {
  it('checks Chocolatey and NSIS Bin install locations used by GitHub runners', () => {
    expect(builderSource).toContain('NSIS\\Bin\\makensis.exe');
    expect(builderSource).toContain('C:\\ProgramData\\chocolatey\\lib\\nsis');
    expect(builderSource).toContain('Get-ChildItem');
  });
});
