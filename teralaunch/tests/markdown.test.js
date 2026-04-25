import { describe, it, expect } from 'vitest';
import { renderMarkdown } from '../src/markdown.js';

describe('renderMarkdown', () => {
    it('escapes raw HTML so script tags are inert', () => {
        const out = renderMarkdown('<script>alert(1)</script>');
        expect(out).not.toContain('<script>');
        expect(out).toContain('&lt;script&gt;');
    });

    it('renders **bold** and *italic*', () => {
        const out = renderMarkdown('**a** and *b*');
        expect(out).toContain('<strong>a</strong>');
        expect(out).toContain('<em>b</em>');
    });

    it('renders [text](url) links with rel=noopener', () => {
        const out = renderMarkdown('[click](https://example.com)');
        expect(out).toMatch(/<a [^>]*href="https:\/\/example\.com"/);
        expect(out).toMatch(/rel="noopener noreferrer"/);
        expect(out).toMatch(/target="_blank"/);
    });

    it('rejects javascript: URLs in links', () => {
        const out = renderMarkdown('[evil](javascript:alert(1))');
        expect(out).not.toMatch(/href="javascript:/);
    });

    it('renders unordered lists', () => {
        const out = renderMarkdown('- one\n- two\n- three');
        expect(out).toContain('<ul>');
        expect(out).toContain('<li>one</li>');
        expect(out).toContain('<li>three</li>');
    });

    it('renders ordered lists', () => {
        const out = renderMarkdown('1. one\n2. two');
        expect(out).toContain('<ol>');
        expect(out).toContain('<li>one</li>');
    });

    it('renders headings up to h3 only', () => {
        const out = renderMarkdown('# h1\n## h2\n### h3\n#### h4');
        expect(out).toContain('<h1>h1</h1>');
        expect(out).toContain('<h2>h2</h2>');
        expect(out).toContain('<h3>h3</h3>');
        expect(out).not.toContain('<h4>');
        expect(out).toContain('#### h4');
    });

    it('renders paragraphs separated by blank lines', () => {
        const out = renderMarkdown('one\n\ntwo');
        expect(out).toMatch(/<p>one<\/p>\s*<p>two<\/p>/);
    });

    it('renders inline `code`', () => {
        const out = renderMarkdown('use `S1UI_Chat2.gpk`');
        expect(out).toContain('<code>S1UI_Chat2.gpk</code>');
    });

    it('renders fenced code blocks', () => {
        const out = renderMarkdown('```\nplain text\n```');
        expect(out).toContain('<pre><code>plain text');
    });

    it('renders images only when URL is http(s) or data:image/', () => {
        const ok = renderMarkdown('![alt](https://example.com/x.png)');
        expect(ok).toMatch(/<img [^>]*src="https:\/\/example\.com\/x\.png"/);
        expect(ok).toMatch(/loading="lazy"/);

        const evil = renderMarkdown('![alt](javascript:alert(1))');
        expect(evil).not.toMatch(/<img /);
    });

    it('strips on*= attributes from any inline HTML attempt', () => {
        const out = renderMarkdown('text with <img src="x" onerror="alert(1)">');
        expect(out).not.toContain('onerror');
    });

    it('returns empty string for null/undefined input', () => {
        expect(renderMarkdown(null)).toBe('');
        expect(renderMarkdown(undefined)).toBe('');
        expect(renderMarkdown('')).toBe('');
    });

    it('preserves plain text with no markdown', () => {
        const out = renderMarkdown('just plain text');
        expect(out).toContain('just plain text');
    });
});
