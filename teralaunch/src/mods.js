/**
 * Mods view controller.
 *
 * Owns: the Installed / Browse tab UI, row rendering, primary-action state
 * machine, download tray, search/filter. Talks to the Rust backend via the
 * nine `commands::mods::*` Tauri commands.
 *
 * No framework — the rest of the launcher is plain DOM manipulation and
 * direct `invoke()` calls, and this view matches that style.
 */

const { invoke: modsInvoke } = window.__TAURI__.core || window.__TAURI__.tauri;
const { listen: modsListen } = window.__TAURI__.event;

const ModsView = {
    state: {
        tab: 'installed',              // 'installed' | 'browse'
        filter: 'all',                 // 'all' | 'external' | 'gpk'
        category: 'all',               // 'all' | <catalog category string>
        query: '',
        installed: [],                 // ModEntry[]
        catalog: [],                   // CatalogEntry[]
        downloads: new Map(),          // id -> { progress, state }
    },
    _eventUnlisten: null,

    _mounted: false,
    _modalBound: false,

    /**
     * Opens the Mods modal. On first call, fetches mods.html, injects it
     * into #mods-modal-content, and wires up the internal mount. Subsequent
     * calls just re-show the modal and refresh state.
     */
    async open() {
        const backdrop = document.getElementById('mods-modal');
        if (!backdrop) {
            console.warn('ModsView.open: #mods-modal not in DOM');
            return;
        }

        if (!this._mounted) {
            const container = document.getElementById('mods-modal-content');
            try {
                const response = await fetch('./mods.html');
                const html = await response.text();
                container.innerHTML = html;
            } catch (e) {
                console.error('Failed to load mods.html:', e);
                container.innerHTML = '<div style="padding:24px;color:#f88">Failed to load mods UI.</div>';
                return;
            }
            this.cacheDom();
            this.bindEvents();
            this._mounted = true;
            // Force-refresh the catalog on first open so the user always
            // sees the latest entries after upgrading. Cache still serves
            // later opens within the same session.
            await Promise.all([this.loadInstalled(), this.loadCatalog(true)]);
            this.render();
            this.subscribeToProgress();
            if (window.App?.updateAllTranslations) {
                await window.App.updateAllTranslations();
            }
        } else {
            // Refresh installed list on re-open so catalog changes land.
            await Promise.all([this.loadInstalled(), this.loadCatalog(false)]);
            this.render();
        }

        this._bindModalDismissOnce(backdrop);
        backdrop.hidden = false;
        backdrop.setAttribute('aria-hidden', 'false');
    },

    close() {
        const backdrop = document.getElementById('mods-modal');
        if (!backdrop) return;
        backdrop.hidden = true;
        backdrop.setAttribute('aria-hidden', 'true');
    },

    _bindModalDismissOnce(backdrop) {
        if (this._modalBound) return;
        this._modalBound = true;
        backdrop.addEventListener('click', (e) => {
            if (e.target === backdrop) this.close();
        });
        // Titlebar X lives inside mods.html (injected on first open).
        const titlebarClose = document.getElementById('mods-titlebar-close');
        if (titlebarClose) titlebarClose.addEventListener('click', () => this.close());
        document.addEventListener('keydown', (e) => {
            if (e.key === 'Escape' && !backdrop.hidden) {
                // Detail panel has its own Escape handler; only close the modal
                // if the detail backdrop is already hidden.
                const detail = document.getElementById('mods-detail-backdrop');
                if (!detail || detail.hidden) this.close();
            }
        });
    },

    /** Legacy alias — the router no longer calls this, but keep for safety. */
    async mount() { return this.open(); },

    async unmount() {
        if (typeof this._eventUnlisten === 'function') {
            this._eventUnlisten();
            this._eventUnlisten = null;
        }
    },

    cacheDom() {
        this.$page = document.getElementById('mods-page');
        this.$search = document.getElementById('mods-search');
        this.$installedExt = document.getElementById('mods-installed-external');
        this.$installedGpk = document.getElementById('mods-installed-gpk');
        this.$installedEmpty = document.getElementById('mods-installed-empty');
        this.$browseRows = document.getElementById('mods-browse-rows');
        this.$browseEmpty = document.getElementById('mods-browse-empty');
        this.$tray = document.getElementById('mods-download-tray');
        this.$trayItems = document.getElementById('mods-download-tray-items');
        this.$trayCount = document.getElementById('mods-download-tray-count');
        this.$countInstalled = document.getElementById('mods-count-installed');
        this.$countBrowse = document.getElementById('mods-count-browse');
        this.$categoryRow = document.getElementById('mods-category-row');
        this.$detailBackdrop = document.getElementById('mods-detail-backdrop');
        this.$detailIcon = document.getElementById('mods-detail-icon');
        this.$detailName = document.getElementById('mods-detail-name');
        this.$detailAuthor = document.getElementById('mods-detail-author');
        this.$detailVersion = document.getElementById('mods-detail-version');
        this.$detailDescription = document.getElementById('mods-detail-description');
        this.$detailScreenshotsSection = document.getElementById('mods-detail-screenshots-section');
        this.$detailScreenshots = document.getElementById('mods-detail-screenshots');
        this.$detailFactAuthor = document.getElementById('mods-detail-fact-author');
        this.$detailFactLicense = document.getElementById('mods-detail-fact-license');
        this.$detailFactLicenseRow = document.getElementById('mods-detail-fact-license-row');
        this.$detailFactCredits = document.getElementById('mods-detail-fact-credits');
        this.$detailFactCreditsRow = document.getElementById('mods-detail-fact-credits-row');
        this.$detailLinkRow = document.getElementById('mods-detail-link-row');
        this.$detailSourceLink = document.getElementById('mods-detail-source-link');
    },

    bindEvents() {
        if (!this.$page) return;

        this.$page.querySelectorAll('.mods-tab').forEach(btn => {
            btn.addEventListener('click', () => this.setTab(btn.dataset.tab));
        });
        // fix.mods-categories-ui (iter 85): scope the kind-filter binding
        // to the .mods-filter-group container. After the filter-strip merge,
        // .mods-filter-chip is also used by the category row — a global
        // query would double-bind every category chip to setFilter(undefined).
        this.$page.querySelectorAll('.mods-filter-group .mods-filter-chip').forEach(btn => {
            btn.addEventListener('click', () => this.setFilter(btn.dataset.filter));
        });

        if (this.$search) {
            const wrap = document.getElementById('mods-search-wrap');
            const clearBtn = document.getElementById('mods-search-clear');
            const updateClear = () => {
                if (!wrap) return;
                wrap.classList.toggle('has-query', this.$search.value.length > 0);
            };
            this.$search.addEventListener('input', (e) => {
                this.state.query = e.target.value.trim().toLowerCase();
                updateClear();
                this.render();
            });
            if (clearBtn) {
                clearBtn.addEventListener('click', () => {
                    this.$search.value = '';
                    this.state.query = '';
                    updateClear();
                    this.$search.focus();
                    this.render();
                });
            }
            updateClear();
        }

        const folderBtn = document.getElementById('mods-folder-btn');
        if (folderBtn) {
            folderBtn.addEventListener('click', async () => {
                try { await modsInvoke('open_mods_folder'); }
                catch (e) { showModsError('Could not open folder', e); }
            });
        }

        const importBtn = document.getElementById('mods-import-btn');
        if (importBtn) {
            // PRD 3.3.4.add-mod-from-file-wire: pick a .gpk, hand the path
            // to the Rust command, refresh the installed list on success.
            importBtn.addEventListener('click', async () => {
                try {
                    const { open } = window.__TAURI__?.dialog || {};
                    const { invoke } = window.__TAURI__?.tauri || window.__TAURI__?.core || {};
                    if (!open || !invoke) {
                        showModsError('Add mod from file', 'Tauri dialog API unavailable.');
                        return;
                    }
                    const selected = await open({
                        multiple: false,
                        filters: [{ name: 'TERA mod package', extensions: ['gpk'] }],
                    });
                    if (!selected || Array.isArray(selected)) return;
                    const entry = await invoke('add_mod_from_file', { path: selected });
                    if (typeof this.loadInstalled === 'function') {
                        await this.loadInstalled();
                    }
                    if (typeof this.render === 'function') {
                        this.render();
                    }
                    return entry;
                } catch (e) {
                    showModsError('Add mod from file', String(e?.message || e));
                }
            });
        }

        // Delegated row-action clicks — one listener per pane.
        const rowListener = (e) => this.handleRowClick(e);
        if (this.$installedExt) this.$installedExt.addEventListener('click', rowListener);
        if (this.$installedGpk) this.$installedGpk.addEventListener('click', rowListener);
        if (this.$browseRows) this.$browseRows.addEventListener('click', rowListener);

        // Detail panel: close on backdrop click, close button, or Escape.
        if (this.$detailBackdrop) {
            this.$detailBackdrop.addEventListener('click', (e) => {
                if (e.target === this.$detailBackdrop) this.closeDetail();
            });
        }
        const closeBtn = document.getElementById('mods-detail-close');
        if (closeBtn) closeBtn.addEventListener('click', () => this.closeDetail());
        document.addEventListener('keydown', (e) => {
            if (e.key === 'Escape' && this.$detailBackdrop && !this.$detailBackdrop.hidden) {
                this.closeDetail();
            }
        });
    },

    setTab(tab) {
        if (tab !== 'installed' && tab !== 'browse') return;
        this.state.tab = tab;
        this.$page.querySelectorAll('.mods-tab').forEach(btn => {
            const active = btn.dataset.tab === tab;
            btn.classList.toggle('active', active);
            btn.setAttribute('aria-selected', active ? 'true' : 'false');
        });
        this.$page.querySelectorAll('.mods-pane').forEach(pane => {
            pane.classList.toggle('active', pane.dataset.pane === tab);
        });
        this.render();
    },

    setFilter(filter) {
        if (!['all', 'external', 'gpk'].includes(filter)) return;
        this.state.filter = filter;
        // fix.mods-categories-ui (iter 85): scoped to the kind-filter group
        // so the active-class flip doesn't leak into the category row.
        this.$page.querySelectorAll('.mods-filter-group .mods-filter-chip').forEach(btn => {
            btn.classList.toggle('active', btn.dataset.filter === filter);
        });
        this.render();
    },

    setCategory(category) {
        this.state.category = category || 'all';
        if (this.$categoryRow) {
            // fix.mods-categories-ui (iter 85): category chips now share the
            // `.mods-filter-chip` class with the kind-filter above — same
            // pill geometry, same active-state treatment. We still scope
            // the query to this.$categoryRow so the kind-filter chips above
            // don't get toggled by the category dispatcher.
            this.$categoryRow.querySelectorAll('.mods-filter-chip').forEach(btn => {
                btn.classList.toggle('active', btn.dataset.category === this.state.category);
            });
        }
        this.render();
    },

    /**
     * Rebuild the category chip row from whichever categories the current
     * catalog advertises. Keeps the "All" chip first, always clickable,
     * and preserves the active selection when possible.
     */
    renderCategoryChips() {
        if (!this.$categoryRow) return;
        const set = new Set();
        for (const entry of this.state.catalog) {
            const cat = (entry.category || '').trim();
            if (cat) set.add(cat);
        }
        const sorted = [...set].sort();
        const active = this.state.category;
        const stillValid = active === 'all' || set.has(active);
        if (!stillValid) this.state.category = 'all';

        const chips = [
            `<button class="mods-filter-chip ${this.state.category === 'all' ? 'active' : ''}" data-category="all" data-translate="MODS_CATEGORY_ALL">${window.App?.t('MODS_CATEGORY_ALL') ?? 'All categories'}</button>`,
        ];
        for (const cat of sorted) {
            const label = this.formatCategoryLabel(cat);
            const isActive = this.state.category === cat;
            chips.push(`<button class="mods-filter-chip ${isActive ? 'active' : ''}" data-category="${cat}">${label}</button>`);
        }
        this.$categoryRow.innerHTML = chips.join('');
        this.$categoryRow.querySelectorAll('.mods-filter-chip').forEach(btn => {
            btn.addEventListener('click', () => this.setCategory(btn.dataset.category));
        });
    },

    /** Capitalise a raw category slug ("ui" → "UI", "fun" → "Fun"). */
    formatCategoryLabel(cat) {
        if (!cat) return '';
        if (cat.length <= 3) return cat.toUpperCase();
        return cat.charAt(0).toUpperCase() + cat.slice(1);
    },

    async loadInstalled() {
        try {
            this.state.installed = await modsInvoke('list_installed_mods');
            // Freshen each installed entry with the current catalog entry so
            // UI fields (icon_url, description, license, credits) always
            // reflect the latest catalog instead of whatever got persisted
            // at install time. This is what keeps old broken icon_urls
            // (from an earlier catalog revision) from lingering in rows.
            if (Array.isArray(this.state.catalog) && this.state.catalog.length) {
                const byId = new Map(this.state.catalog.map(c => [c.id, c]));
                this.state.installed.forEach(m => {
                    const cat = byId.get(m.id);
                    if (!cat) return;
                    m.icon_url = cat.icon_url || null;
                    m.source_url = cat.source_url || m.source_url;
                    m.license = cat.license || m.license;
                    m.credits = cat.credits || m.credits;

                    // Update detection: if the catalog advertises a
                    // version different from what's installed, flip the
                    // row into update_available so the UI shows an
                    // "Update" button. Skip if the mod is mid-flow
                    // (installing/error) or actively running — we don't
                    // want to stomp a live state. String inequality is
                    // fine here; versions are opaque strings like
                    // "2.0.1-classicplus" or "2026-04".
                    const skipStatuses = new Set(['installing', 'error', 'running', 'starting']);
                    if (!skipStatuses.has(m.status)
                        && cat.version
                        && m.version
                        && cat.version !== m.version) {
                        m.status = 'update_available';
                    }
                });
            }
        } catch (e) {
            console.error('list_installed_mods failed:', e);
            this.state.installed = [];
        }
    },

    async loadCatalog(forceRefresh = false) {
        try {
            const catalog = await modsInvoke('get_mods_catalog', { forceRefresh });
            this.state.catalog = (catalog && Array.isArray(catalog.mods)) ? catalog.mods : [];
            this._catalogError = null;
        } catch (e) {
            // Surface the actual Rust error (network, TLS, JSON parse…) in
            // the console so "catalog unavailable" has a real diagnostic.
            console.warn('get_mods_catalog failed:', e);
            this.state.catalog = [];
            this._catalogError = String(e);
        }
        // Category chips depend on what's in the catalog, so rebuild them
        // whenever the catalog reloads.
        this.renderCategoryChips();
    },

    async subscribeToProgress() {
        try {
            this._eventUnlisten = await modsListen('mod_download_progress', (event) => {
                const payload = event && event.payload;
                if (!payload || !payload.id) return;
                if (payload.state === 'done' || payload.state === 'error') {
                    this.state.downloads.delete(payload.id);
                    // Terminal event — refresh installed list so the row
                    // flips from progress bar to toggle/error state. This
                    // re-render happens once per install, not per 5% tick.
                    this.loadInstalled().then(() => this.render());
                } else {
                    const pct = payload.progress || 0;
                    this.state.downloads.set(payload.id, {
                        progress: pct,
                        state: payload.state || 'downloading',
                        received_bytes: payload.received_bytes || 0,
                        total_bytes: payload.total_bytes || 0,
                    });
                    // IMPORTANT: don't re-render the whole pane on every 5%
                    // tick — that's what caused the "flashes the whole
                    // screen" feedback. Just patch the progress bars in
                    // place via updateDownloadProgress.
                    this.updateDownloadProgress(payload.id, pct, payload.received_bytes || 0, payload.total_bytes || 0);
                }
            });
        } catch (e) {
            console.warn('Could not listen to mod_download_progress:', e);
        }
    },

    /**
     * Surgical DOM update: finds any progress bar belonging to `id` and
     * updates its width + label text, plus the download tray row's bar.
     * Never touches any other DOM node, so the rest of the pane doesn't
     * flash during a download.
     */
    updateDownloadProgress(id, pct, received, total) {
        const rows = document.querySelectorAll(`.mods-row[data-mod-id="${CSS.escape(id)}"]`);
        rows.forEach(row => {
            const bar = row.querySelector('[data-progressbar]');
            if (bar) {
                const fill = bar.querySelector('.mods-row-progressbar-fill');
                const label = bar.querySelector('.mods-row-progressbar-label');
                if (fill) fill.style.width = `${pct}%`;
                if (label) label.textContent = `${pct}%`;
            } else {
                // Row is rendered as browse/Install but we just started
                // downloading — inject the progress bar inline without
                // reflowing the rest of the pane.
                const status = row.querySelector('.mods-row-status');
                if (status) status.innerHTML = this.buildProgressBar(pct);
            }
        });
        this.updateDownloadTrayItem(id, pct, received, total);
    },

    /** Update just one row in the download tray, without rebuilding it. */
    updateDownloadTrayItem(id, pct, received, total) {
        if (!this.$tray || !this.$trayItems) return;
        if (this.$tray.hidden) {
            // Tray wasn't visible yet — render it fresh once.
            this.renderDownloadTray();
            return;
        }
        const selector = `.mods-download-tray-item[data-dl-id="${CSS.escape(id)}"]`;
        let item = this.$trayItems.querySelector(selector);
        if (!item) {
            // New download appearing mid-session; add the row surgically.
            this.renderDownloadTray();
            item = this.$trayItems.querySelector(selector);
            if (!item) return;
        }
        const bar = item.querySelector('.mods-download-tray-bar-fill');
        const label = item.querySelector('.mods-download-tray-progress');
        const detail = item.querySelector('.mods-download-tray-bytes');
        if (bar) bar.style.width = `${pct}%`;
        if (label) label.textContent = `${pct}%`;
        if (detail && total > 0) {
            detail.textContent = `${formatMB(received)} / ${formatMB(total)}`;
        }
        if (this.$trayCount) this.$trayCount.textContent = String(this.state.downloads.size);
    },

    filterMatches(entry) {
        if (this.state.filter !== 'all') {
            const kindKey = entry.kind === 'external' ? 'external' : 'gpk';
            if (kindKey !== this.state.filter) return false;
        }
        if (this.state.category && this.state.category !== 'all') {
            if ((entry.category || '') !== this.state.category) return false;
        }
        if (this.state.query) {
            const hay = [
                entry.name,
                entry.author,
                entry.category,
                entry.description || entry.short_description || '',
            ].filter(Boolean).join(' ').toLowerCase();
            if (!hay.includes(this.state.query)) return false;
        }
        return true;
    },

    render() {
        if (!this.$page) return;

        // Installed count on the tab badge updates on every render so the
        // number always reflects state even while the user is looking at
        // the Browse tab. Same story for the per-kind group counts and
        // the Browse badge (which counts catalog entries minus installed).
        const installedTotal = this.state.installed.length;
        if (this.$countInstalled) this.$countInstalled.textContent = installedTotal;
        const installedIds = new Set(this.state.installed.map(m => m.id));
        const browseTotal = this.state.catalog.filter(e => !installedIds.has(e.id)).length;
        if (this.$countBrowse) this.$countBrowse.textContent = browseTotal;
        const extCount = this.state.installed.filter(m => m.kind === 'external').length;
        const gpkCount = this.state.installed.filter(m => m.kind === 'gpk').length;
        if (this.$page) {
            const extEl = this.$page.querySelector('[data-count="external"]');
            const gpkEl = this.$page.querySelector('[data-count="gpk"]');
            if (extEl) extEl.textContent = extCount;
            if (gpkEl) gpkEl.textContent = gpkCount;
        }

        // Tab-specific rendering.
        if (this.state.tab === 'installed') {
            const external = this.state.installed.filter(m => m.kind === 'external' && this.filterMatches(m));
            const gpk = this.state.installed.filter(m => m.kind === 'gpk' && this.filterMatches(m));

            this.renderInstalledGroup(this.$installedExt, external, 'external');
            this.renderInstalledGroup(this.$installedGpk, gpk, 'gpk');

            const anyVisible = external.length + gpk.length > 0;
            if (this.$installedEmpty) this.$installedEmpty.hidden = anyVisible;
        } else {
            this.renderBrowse();
        }

        this.renderDownloadTray();
    },

    renderInstalledGroup(container, entries, kind) {
        if (!container) return;
        const countEl = this.$page.querySelector(`[data-count="${kind}"]`);
        if (countEl) countEl.textContent = entries.length;

        container.innerHTML = '';
        for (const entry of entries) {
            container.appendChild(this.buildRow(entry, 'installed'));
        }
    },

    renderBrowse() {
        if (!this.$browseRows) return;
        // Installed mods never appear in Browse — that's what the Installed
        // tab is for. Hide them entirely rather than showing a badge.
        const installedIds = new Set(this.state.installed.map(m => m.id));
        const visible = this.state.catalog.filter(e =>
            !installedIds.has(e.id) && this.filterMatches(e)
        );
        this.$browseRows.innerHTML = '';
        for (const entry of visible) {
            const view = {
                id: entry.id,
                kind: entry.kind,
                name: entry.name,
                author: entry.author,
                description: entry.short_description || '',
                version: entry.version,
                icon_url: entry.icon_url,
                _catalog: entry,
            };
            this.$browseRows.appendChild(this.buildRow(view, 'browse'));
        }
        if (this.$browseEmpty) {
            this.$browseEmpty.hidden = visible.length > 0;
            // If we failed to fetch the catalog entirely, surface the actual
            // error in the empty-state text so "catalog unavailable" isn't
            // the only thing the user sees.
            if (visible.length === 0 && this._catalogError) {
                this.$browseEmpty.innerHTML = `
                    <p>Catalog unavailable.</p>
                    <p style="opacity:.55;font-size:12px;margin-top:6px;">${escapeHtml(this._catalogError)}</p>
                `;
            }
        }
    },

    renderDownloadTray() {
        if (!this.$tray || !this.$trayItems) return;
        if (this.state.downloads.size === 0) {
            this.$tray.hidden = true;
            this.$trayItems.innerHTML = '';
            return;
        }
        this.$tray.hidden = false;
        if (this.$trayCount) this.$trayCount.textContent = this.state.downloads.size;
        this.$trayItems.innerHTML = '';
        for (const [id, info] of this.state.downloads) {
            const entry = this.state.installed.find(m => m.id === id)
                || this.state.catalog.find(m => m.id === id);
            const name = entry ? entry.name : id;
            const pct = info.progress || 0;
            const bytesLine = info.total_bytes
                ? `${formatMB(info.received_bytes || 0)} / ${formatMB(info.total_bytes)}`
                : '';
            const row = document.createElement('div');
            row.className = 'mods-download-tray-item';
            row.dataset.dlId = id;
            row.innerHTML = `
                <div class="mods-download-tray-item-header">
                    <span class="mods-download-tray-name">${escapeHtml(name)}</span>
                    <span class="mods-download-tray-progress">${pct}%</span>
                </div>
                <div class="mods-download-tray-bar">
                    <div class="mods-download-tray-bar-fill" style="width:${pct}%"></div>
                </div>
                <div class="mods-download-tray-bytes">${escapeHtml(bytesLine)}</div>`;
            this.$trayItems.appendChild(row);
        }
    },

    buildRow(entry, context) {
        const row = document.createElement('div');
        row.className = 'mods-row';
        if (!entry.icon_url) row.classList.add('no-icon');
        row.dataset.modId = entry.id;
        row.dataset.modKind = entry.kind;
        row.dataset.context = context;

        // Only render an icon cell when the catalog entry actually carries
        // an icon_url. No initials placeholder — the user explicitly asked
        // for that to go away. If the URL 404s at runtime, the error
        // listener attached below collapses the row to no-icon spacing.
        // Attribute-based onerror is CSP-forbidden under script-src without
        // 'unsafe-inline' (PRD 3.1.12).
        const iconMarkup = entry.icon_url
            ? `<div class="mods-row-icon"><img class="mods-row-icon-img" src="${escapeHtml(entry.icon_url)}" alt="" /></div>`
            : '';

        const statusCell = this.buildStatusCell(entry, context);

        row.innerHTML = `
            ${iconMarkup}
            <div class="mods-row-body">
                <div class="mods-row-title">
                    <span class="mods-row-name">${escapeHtml(entry.name)}</span>
                    <span class="mods-row-author">${escapeHtml(entry.author || '')}</span>
                </div>
                <div class="mods-row-desc">${escapeHtml(entry.description || '')}</div>
                ${entry.last_error ? `<div class="mods-row-error">${escapeHtml(entry.last_error)}</div>` : ''}
            </div>
            <div class="mods-row-status">${statusCell}</div>
            <div class="mods-row-menu">
                <button class="mods-row-overflow" data-action="overflow" aria-label="${window.App?.t('MODS_ARIA_MORE') ?? 'More'}">⋯</button>
            </div>
        `;

        const iconImg = row.querySelector('.mods-row-icon-img');
        if (iconImg) {
            iconImg.addEventListener('error', () => {
                row.classList.add('no-icon');
                const iconCell = row.querySelector('.mods-row-icon');
                if (iconCell) iconCell.remove();
            }, { once: true });
        }

        return row;
    },

    buildStatusCell(entry, context) {
        if (context === 'browse') {
            // installed rows never appear in browse (renderBrowse filters
            // them out), so this is always the Install action.
            return `<button class="mods-row-primary install" data-action="install" data-translate="MODS_ACTION_INSTALL">Install</button>`;
        }

        const download = this.state.downloads.get(entry.id);
        if (download) {
            return this.buildProgressBar(download.progress || 0);
        }

        switch (entry.status) {
            case 'error':
                return `<button class="mods-row-primary error" data-action="retry" data-translate="MODS_ACTION_RETRY">Retry</button>`;
            case 'installing':
                return this.buildProgressBar(entry.progress || 0);
            case 'update_available':
                return `<button class="mods-row-primary update" data-action="update" data-translate="MODS_ACTION_UPDATE">Update</button>`;
            default: {
                // Every other state collapses to "enabled toggle". Enabled
                // external apps auto-launch with the game; enabled GPKs
                // auto-deploy on Launch once the mapper patcher ships.
                const enabled = entry.enabled || entry.status === 'enabled' || entry.status === 'running' || entry.status === 'starting';
                return `
                    <label class="mods-row-toggle" title="${enabled ? (window.App?.t('MODS_TITLE_ENABLED_HINT') ?? 'Enabled — runs with the game') : (window.App?.t('MODS_TITLE_DISABLED_HINT') ?? 'Disabled — click to enable')}">
                        <input type="checkbox" data-action="toggle" ${enabled ? 'checked' : ''} />
                        <span class="mods-row-toggle-track"><span class="mods-row-toggle-thumb"></span></span>
                    </label>
                    ${entry.status === 'running' ? `<span class="mods-row-running-pill"><span class="mods-row-running-dot"></span>${window.App?.t('MODS_STATUS_RUNNING') ?? 'Running'}</span>` : ''}
                `;
            }
        }
    },

    /** Renders an inline progress bar for the status cell. */
    buildProgressBar(pct) {
        const clamped = Math.max(0, Math.min(100, Math.round(pct)));
        return `
            <div class="mods-row-progressbar" data-progressbar>
                <div class="mods-row-progressbar-track">
                    <div class="mods-row-progressbar-fill" style="width:${clamped}%"></div>
                </div>
                <span class="mods-row-progressbar-label">${clamped}%</span>
            </div>
        `;
    },

    async handleRowClick(event) {
        const btn = event.target.closest('[data-action]');
        const row = event.target.closest('.mods-row');

        // No action button — drill into detail panel if the click was on the
        // row body (author/description area). Status cell and action buttons
        // are handled by the normal switch below.
        if (!btn) {
            if (row && event.target.closest('.mods-row-body')) {
                this.openDetail(row.dataset.modId, row.dataset.context);
            }
            return;
        }
        if (!row) return;
        const id = row.dataset.modId;
        const action = btn.dataset.action;

        // Checkbox toggle: let the browser commit the flip natively so the
        // switch moves immediately even if the IPC call takes a moment.
        // Everything else (Install/Update/Retry buttons) has no default
        // action worth keeping, so we block it.
        if (action !== 'toggle') {
            event.preventDefault();
        }

        try {
            switch (action) {
                case 'install': {
                    const catalogEntry = this.state.catalog.find(m => m.id === id);
                    if (!catalogEntry) return;
                    this.state.downloads.set(id, { progress: 0, state: 'downloading' });
                    this.render();
                    await modsInvoke('install_mod', { entry: catalogEntry });
                    await this.loadInstalled();
                    this.render();
                    break;
                }
                case 'toggle': {
                    // Checkbox toggle: the input's checked state *before*
                    // this handler fires reflects the user's intent. In an
                    // HTML checkbox the `change` has already flipped `.checked`
                    // by the time we see it on click — treat .checked as
                    // the target state.
                    const checkbox = btn;
                    const shouldEnable = checkbox.checked;
                    try {
                        if (shouldEnable) {
                            await modsInvoke('enable_mod', { id });
                        } else {
                            await modsInvoke('disable_mod', { id });
                        }
                    } catch (err) {
                        // Revert visual state if the command failed.
                        checkbox.checked = !shouldEnable;
                        throw err;
                    }
                    await this.loadInstalled();
                    this.render();
                    break;
                }
                case 'launch':
                case 'enable': {
                    await modsInvoke('enable_mod', { id });
                    await this.loadInstalled();
                    this.render();
                    break;
                }
                case 'disable':
                case 'stop': {
                    await modsInvoke('disable_mod', { id });
                    await this.loadInstalled();
                    this.render();
                    break;
                }
                case 'update': {
                    // Reinstall with the current catalog entry — the
                    // backend overwrites the dest_dir/file, so this is
                    // a clean version swap. The version field on the
                    // registry row gets refreshed on success.
                    const catalogEntry = this.state.catalog.find(m => m.id === id);
                    if (!catalogEntry) return;
                    this.state.downloads.set(id, { progress: 0, state: 'downloading' });
                    this.render();
                    await modsInvoke('install_mod', { entry: catalogEntry });
                    await this.loadInstalled();
                    this.render();
                    break;
                }
                case 'retry': {
                    const catalogEntry = this.state.catalog.find(m => m.id === id);
                    if (catalogEntry) {
                        await modsInvoke('install_mod', { entry: catalogEntry });
                        await this.loadInstalled();
                        this.render();
                    }
                    break;
                }
                case 'overflow': {
                    await this.showOverflowMenu(id, btn);
                    break;
                }
                default:
                    console.warn('Unknown mod action:', action);
            }
        } catch (e) {
            console.error(`Action ${action} failed:`, e);
            showModsError(`Action failed: ${action}`, e);
        }
    },

    /**
     * Open the detail panel for a mod. Resolves by id+context — installed
     * rows come from `state.installed`, browse rows from `state.catalog`.
     * Both shapes share most fields; the catalog has `short_description`
     * + `long_description`, the installed entry has merged `description`
     * and an optional `long_description`. We populate from whichever
     * source we find first, with the other filling gaps.
     */
    openDetail(id, context) {
        if (!this.$detailBackdrop || !id) return;
        const installed = this.state.installed.find(m => m.id === id);
        const catalog = this.state.catalog.find(m => m.id === id);
        const entry = context === 'browse'
            ? (catalog || installed)
            : (installed || catalog);
        if (!entry) return;

        const name = entry.name || id;
        const author = entry.author || '';
        const version = entry.version || '';
        const sourceUrl = entry.source_url || (catalog && catalog.source_url) || '';
        const license = entry.license || (catalog && catalog.license) || '';
        const credits = entry.credits || (catalog && catalog.credits) || '';
        const longDesc = entry.long_description
            || (catalog && catalog.long_description)
            || entry.description
            || (catalog && catalog.short_description)
            || '';
        const screenshots = (entry.screenshots && entry.screenshots.length)
            ? entry.screenshots
            : (catalog && catalog.screenshots) || [];
        const iconUrl = entry.icon_url || (catalog && catalog.icon_url) || '';

        this.$detailName.textContent = name;
        this.$detailAuthor.textContent = author || '—';
        this.$detailVersion.textContent = version ? `v${version}` : '';
        this.$detailDescription.textContent = longDesc || '';
        this.$detailFactAuthor.textContent = author || '—';

        if (license) {
            this.$detailFactLicenseRow.hidden = false;
            this.$detailFactLicense.textContent = license;
        } else {
            this.$detailFactLicenseRow.hidden = true;
        }

        if (credits) {
            this.$detailFactCreditsRow.hidden = false;
            this.$detailFactCredits.textContent = credits;
        } else {
            this.$detailFactCreditsRow.hidden = true;
        }

        if (sourceUrl) {
            this.$detailLinkRow.hidden = false;
            this.$detailSourceLink.href = sourceUrl;
        } else {
            this.$detailLinkRow.hidden = true;
            this.$detailSourceLink.href = '#';
        }

        if (iconUrl) {
            this.$detailIcon.innerHTML = `<img src="${escapeHtml(iconUrl)}" alt="" />`;
        } else {
            this.$detailIcon.textContent = toInitials(name);
        }

        if (screenshots.length) {
            this.$detailScreenshotsSection.hidden = false;
            this.$detailScreenshots.innerHTML = screenshots
                .map(url => `<img src="${escapeHtml(url)}" alt="" loading="lazy" />`)
                .join('');
        } else {
            this.$detailScreenshotsSection.hidden = true;
            this.$detailScreenshots.innerHTML = '';
        }

        this.$detailBackdrop.hidden = false;
    },

    closeDetail() {
        if (!this.$detailBackdrop) return;
        this.$detailBackdrop.hidden = true;
    },

    /**
     * Custom confirmation dialog — returns a Promise that resolves with
     * true (confirmed) or false (cancelled / dismissed). Replaces
     * window.confirm, which is unreliable in Tauri's WebView2 and
     * sometimes short-circuits with 'true' before rendering.
     */
    modalConfirm({ title, body = '', confirmLabel = 'Confirm', cancelLabel = 'Cancel', danger = false }) {
        return new Promise(resolve => {
            const backdrop = document.createElement('div');
            backdrop.className = 'mods-confirm-backdrop';
            backdrop.innerHTML = `
                <div class="mods-confirm-card">
                    <h3 class="mods-confirm-title">${escapeHtml(title)}</h3>
                    ${body ? `<p class="mods-confirm-body">${escapeHtml(body)}</p>` : ''}
                    <div class="mods-confirm-actions">
                        <button type="button" class="mods-onboarding-btn secondary" data-confirm-action="cancel">${escapeHtml(cancelLabel)}</button>
                        <button type="button" class="mods-onboarding-btn ${danger ? 'danger' : 'primary'}" data-confirm-action="ok">${escapeHtml(confirmLabel)}</button>
                    </div>
                </div>`;
            document.body.appendChild(backdrop);
            const finish = (value) => {
                backdrop.remove();
                document.removeEventListener('keydown', keyHandler, true);
                resolve(value);
            };
            const keyHandler = (e) => {
                if (e.key === 'Escape') { e.stopPropagation(); finish(false); }
                if (e.key === 'Enter')  { e.stopPropagation(); finish(true); }
            };
            backdrop.addEventListener('click', e => {
                const btn = e.target.closest('[data-confirm-action]');
                if (btn) finish(btn.dataset.confirmAction === 'ok');
                else if (e.target === backdrop) finish(false);
            });
            document.addEventListener('keydown', keyHandler, true);
            // Focus the primary button so Enter just works.
            requestAnimationFrame(() => {
                backdrop.querySelector('[data-confirm-action="ok"]')?.focus();
            });
        });
    },

    async showOverflowMenu(id, anchor) {
        // Small inline popover — click outside / Escape dismisses. Actions
        // depend on whether the mod is installed: installed gets Details /
        // Open source / Uninstall, browse rows get Details / Open source.
        const installed = this.state.installed.find(m => m.id === id);
        const catalog = this.state.catalog.find(m => m.id === id);
        const entry = installed || catalog;
        if (!entry) return;

        // Dismiss any prior popover so we never stack two.
        document.querySelectorAll('.mods-row-popover').forEach(el => el.remove());

        const sourceUrl = entry.source_url || (catalog && catalog.source_url) || '';
        const isInstalled = !!installed;

        const popover = document.createElement('div');
        popover.className = 'mods-row-popover';
        popover.innerHTML = `
            <button class="mods-row-popover-item" data-popover-action="details">
                <span data-translate="MODS_MENU_DETAILS">${window.App?.t('MODS_MENU_DETAILS') ?? 'Details'}</span>
            </button>
            ${sourceUrl
                ? `<button class="mods-row-popover-item" data-popover-action="source">
                      <span data-translate="MODS_MENU_OPEN_SOURCE">${window.App?.t('MODS_MENU_OPEN_SOURCE') ?? 'Open source'}</span>
                   </button>`
                : ''}
            ${isInstalled
                ? `<button class="mods-row-popover-item danger" data-popover-action="uninstall">
                      <span data-translate="MODS_MENU_UNINSTALL">${window.App?.t('MODS_MENU_UNINSTALL') ?? 'Uninstall'}</span>
                   </button>`
                : ''}
        `;

        // Position to the left of the anchor so the menu doesn't clip off
        // the right edge of the modal.
        const rect = anchor.getBoundingClientRect();
        popover.style.position = 'fixed';
        popover.style.top = `${rect.bottom + 6}px`;
        popover.style.right = `${Math.max(16, window.innerWidth - rect.right)}px`;
        document.body.appendChild(popover);

        const dismiss = () => {
            popover.remove();
            document.removeEventListener('click', outsideClick, true);
            document.removeEventListener('keydown', escKey, true);
        };
        const outsideClick = (e) => {
            if (!popover.contains(e.target) && e.target !== anchor) dismiss();
        };
        const escKey = (e) => { if (e.key === 'Escape') dismiss(); };

        popover.addEventListener('click', async (e) => {
            const item = e.target.closest('[data-popover-action]');
            if (!item) return;
            const action = item.dataset.popoverAction;
            dismiss();
            if (action === 'details') {
                this.openDetail(id, isInstalled ? 'installed' : 'browse');
            } else if (action === 'source' && sourceUrl) {
                try {
                    const { open: openShell } = window.__TAURI__.shell;
                    await openShell(sourceUrl);
                } catch (err) {
                    window.open(sourceUrl, '_blank');
                }
            } else if (action === 'uninstall' && isInstalled) {
                // window.confirm() is unreliable inside the Tauri WebView2
                // — on some machines it returns true without ever showing a
                // dialog, which is why the old flow uninstalled instantly.
                // Use a real in-page confirm that blocks on a promise.
                const ok = await this.modalConfirm({
                    title: `Uninstall "${entry.name}"?`,
                    body: 'The mod files and its entry in the Installed tab will be removed.',
                    confirmLabel: 'Uninstall',
                    danger: true,
                });
                if (!ok) return;
                try {
                    await modsInvoke('uninstall_mod', { id, deleteSettings: null });
                    await this.loadInstalled();
                    this.render();
                } catch (err) {
                    showModsError('Uninstall failed', err);
                }
            }
        });

        // Delay binding so the click that opened this menu doesn't immediately
        // close it.
        setTimeout(() => {
            document.addEventListener('click', outsideClick, true);
            document.addEventListener('keydown', escKey, true);
        }, 0);
    },
};

function formatMB(bytes) {
    if (!bytes) return '0 MB';
    return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

function toInitials(name) {
    if (!name) return '??';
    const parts = name.split(/\s+/).filter(Boolean);
    if (parts.length === 0) return '??';
    if (parts.length === 1) return parts[0].slice(0, 2).toUpperCase();
    return (parts[0][0] + parts[1][0]).toUpperCase();
}

function escapeHtml(str) {
    if (str == null) return '';
    return String(str)
        .replace(/&/g, '&amp;')
        .replace(/</g, '&lt;')
        .replace(/>/g, '&gt;')
        .replace(/"/g, '&quot;')
        .replace(/'/g, '&#39;');
}

function showModsError(title, detail) {
    // Route through the launcher's existing notification system if present.
    if (typeof window.showUpdateNotification === 'function') {
        const message = (detail && detail.message) ? detail.message : String(detail || '');
        window.showUpdateNotification('error', title, message);
    } else {
        console.error(`[Mods] ${title}:`, detail);
    }
}

// Expose for the top-right Mods toolbar button. The router no longer has
// a 'mods' route — see router.js. app.js binds #mods-button to ModsView.open.
if (typeof window !== 'undefined') {
    window.ModsView = ModsView;
    // initMods retained as a no-op alias in case any legacy code still looks
    // it up; real open path is window.ModsView.open().
    window.initMods = async function () { await ModsView.open(); };
}

export { ModsView };
