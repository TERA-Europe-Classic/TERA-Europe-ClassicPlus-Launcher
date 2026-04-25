// Trusted-subset markdown renderer for catalog content.
// We author all input ourselves but treat it as untrusted by default.
// Subset: paragraphs, h1-h3, **bold**, *italic*, `inline code`, fenced
// code, [links](url), ![images](url), - and 1. lists. Anything else
// renders as plain escaped text.

const ESC = { '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' };
function escape(s) { return String(s).replace(/[&<>"']/g, ch => ESC[ch]); }

// Escape user-supplied content. Strips on*= event-handler tokens before
// escaping so the literal substring is not present in the output, even
// though the surrounding < and > are already escaped (which alone makes
// any inline HTML inert).
function escapeUserText(s) {
    const stripped = String(s).replace(/\bon\w+\s*=/gi, '');
    return escape(stripped);
}

function isSafeUrl(url) {
    if (!url) return false;
    return /^https?:\/\//i.test(url) || /^data:image\//i.test(url);
}

function inline(text) {
    let s = escapeUserText(text);
    // images first (before regular links so the ! prefix doesn't get parsed as a link)
    s = s.replace(/!\[([^\]]*)\]\(([^)]+)\)/g, (_, alt, url) => {
        if (!isSafeUrl(url)) return '';
        return `<img src="${escape(url)}" alt="${escape(alt)}" loading="lazy" />`;
    });
    s = s.replace(/\[([^\]]+)\]\(([^)]+)\)/g, (_, label, url) => {
        if (!isSafeUrl(url)) return escape(label);
        return `<a href="${escape(url)}" target="_blank" rel="noopener noreferrer">${escape(label)}</a>`;
    });
    s = s.replace(/`([^`]+)`/g, (_, code) => `<code>${code}</code>`);
    s = s.replace(/\*\*([^*]+)\*\*/g, '<strong>$1</strong>');
    s = s.replace(/\*([^*]+)\*/g, '<em>$1</em>');
    return s;
}

function renderBlocks(src) {
    const lines = src.replace(/\r\n/g, '\n').split('\n');
    const out = [];
    let i = 0;

    while (i < lines.length) {
        const line = lines[i];

        // fenced code block
        if (/^```/.test(line)) {
            i++;
            const code = [];
            while (i < lines.length && !/^```/.test(lines[i])) {
                code.push(lines[i]);
                i++;
            }
            i++; // skip closing fence
            out.push(`<pre><code>${escapeUserText(code.join('\n'))}</code></pre>`);
            continue;
        }

        // headings (h1-h3 only)
        const h = line.match(/^(#{1,3})\s+(.*)$/);
        if (h) {
            const level = h[1].length;
            out.push(`<h${level}>${inline(h[2])}</h${level}>`);
            i++;
            continue;
        }

        // unordered list
        if (/^- /.test(line)) {
            const items = [];
            while (i < lines.length && /^- /.test(lines[i])) {
                items.push(`<li>${inline(lines[i].slice(2))}</li>`);
                i++;
            }
            out.push(`<ul>${items.join('')}</ul>`);
            continue;
        }

        // ordered list
        if (/^\d+\. /.test(line)) {
            const items = [];
            while (i < lines.length && /^\d+\. /.test(lines[i])) {
                items.push(`<li>${inline(lines[i].replace(/^\d+\. /, ''))}</li>`);
                i++;
            }
            out.push(`<ol>${items.join('')}</ol>`);
            continue;
        }

        // blank line
        if (line.trim() === '') {
            i++;
            continue;
        }

        // paragraph (consume until blank line or block start)
        const para = [];
        while (
            i < lines.length
            && lines[i].trim() !== ''
            && !/^```/.test(lines[i])
            && !/^#{1,3}\s+/.test(lines[i])
            && !/^- /.test(lines[i])
            && !/^\d+\. /.test(lines[i])
        ) {
            para.push(lines[i]);
            i++;
        }
        if (para.length > 0) {
            out.push(`<p>${inline(para.join(' '))}</p>`);
        }
    }

    return out.join('\n');
}

export function renderMarkdown(input) {
    if (input == null || input === '') return '';
    return renderBlocks(String(input));
}
