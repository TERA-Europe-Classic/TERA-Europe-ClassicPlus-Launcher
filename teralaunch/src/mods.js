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

const { invoke: modsInvoke } = window.__TAURI__.tauri;
const { listen: modsListen } = window.__TAURI__.event;

const ModsView = {
    state: {
        tab: 'installed',              // 'installed' | 'browse'
        filter: 'all',                 // 'all' | 'external' | 'gpk'
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
            await Promise.all([this.loadInstalled(), this.loadCatalog()]);
            this.render();
            this.subscribeToProgress();
            // Translate labels that were in the freshly-loaded fragment.
            if (window.App?.updateAllTranslations) {
                await window.App.updateAllTranslations();
            }
        } else {
            // Refresh installed list on re-open so catalog changes land.
            await this.loadInstalled();
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
        const closeBtn = document.getElementById('mods-modal-close');
        if (closeBtn) closeBtn.addEventListener('click', () => this.close());
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
        this.$page.querySelectorAll('.mods-filter-chip').forEach(btn => {
            btn.addEventListener('click', () => this.setFilter(btn.dataset.filter));
        });

        if (this.$search) {
            this.$search.addEventListener('input', (e) => {
                this.state.query = e.target.value.trim().toLowerCase();
                this.render();
            });
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
            // GPK import requires the mapper patcher (Phase C). Until that
            // ships, disable the button and show a neutral coming-soon toast
            // instead of the previous red "error" toast.
            importBtn.disabled = true;
            importBtn.title = 'Coming soon — local GPK import requires the mapper patcher (Phase C).';
            importBtn.classList.add('is-disabled');
            importBtn.addEventListener('click', () => {
                // Safety: in case the disabled attribute is ever removed by theming.
                showModsError('Add mod from file', 'Coming soon — GPK import will land with the mapper patcher in a later update.');
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
        this.$page.querySelectorAll('.mods-filter-chip').forEach(btn => {
            btn.classList.toggle('active', btn.dataset.filter === filter);
        });
        this.render();
    },

    async loadInstalled() {
        try {
            this.state.installed = await modsInvoke('list_installed_mods');
        } catch (e) {
            console.error('list_installed_mods failed:', e);
            this.state.installed = [];
        }
    },

    async loadCatalog() {
        try {
            const catalog = await modsInvoke('get_mods_catalog', { forceRefresh: false });
            this.state.catalog = (catalog && Array.isArray(catalog.mods)) ? catalog.mods : [];
        } catch (e) {
            console.warn('get_mods_catalog failed:', e);
            this.state.catalog = [];
        }
    },

    async subscribeToProgress() {
        try {
            this._eventUnlisten = await modsListen('mod_download_progress', (event) => {
                const payload = event && event.payload;
                if (!payload || !payload.id) return;
                if (payload.state === 'done' || payload.state === 'error') {
                    this.state.downloads.delete(payload.id);
                    // Refresh installed list — the entry's status just changed.
                    this.loadInstalled().then(() => this.render());
                } else {
                    this.state.downloads.set(payload.id, {
                        progress: payload.progress || 0,
                        state: payload.state || 'downloading',
                    });
                    this.render();
                }
            });
        } catch (e) {
            console.warn('Could not listen to mod_download_progress:', e);
        }
    },

    filterMatches(entry) {
        if (this.state.filter !== 'all') {
            const kindKey = entry.kind === 'external' ? 'external' : 'gpk';
            if (kindKey !== this.state.filter) return false;
        }
        if (this.state.query) {
            const hay = `${entry.name} ${entry.author} ${entry.description || entry.short_description || ''}`.toLowerCase();
            if (!hay.includes(this.state.query)) return false;
        }
        return true;
    },

    render() {
        if (!this.$page) return;

        // Tab-specific rendering.
        if (this.state.tab === 'installed') {
            const external = this.state.installed.filter(m => m.kind === 'external' && this.filterMatches(m));
            const gpk = this.state.installed.filter(m => m.kind === 'gpk' && this.filterMatches(m));

            this.renderInstalledGroup(this.$installedExt, external, 'external');
            this.renderInstalledGroup(this.$installedGpk, gpk, 'gpk');

            const total = this.state.installed.length;
            if (this.$countInstalled) this.$countInstalled.textContent = total;
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
        const installedIds = new Set(this.state.installed.map(m => m.id));
        const visible = this.state.catalog.filter(e => this.filterMatches(e));
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
                installed: installedIds.has(entry.id),
                _catalog: entry,
            };
            this.$browseRows.appendChild(this.buildRow(view, 'browse'));
        }
        if (this.$browseEmpty) this.$browseEmpty.hidden = visible.length > 0;
    },

    renderDownloadTray() {
        if (!this.$tray || !this.$trayItems) return;
        if (this.state.downloads.size === 0) {
            this.$tray.hidden = true;
            return;
        }
        this.$tray.hidden = false;
        if (this.$trayCount) this.$trayCount.textContent = this.state.downloads.size;
        this.$trayItems.innerHTML = '';
        for (const [id, info] of this.state.downloads) {
            const entry = this.state.installed.find(m => m.id === id)
                || this.state.catalog.find(m => m.id === id);
            const name = entry ? entry.name : id;
            const row = document.createElement('div');
            row.className = 'mods-download-tray-item';
            row.innerHTML = `
                <span class="mods-download-tray-name">${escapeHtml(name)}</span>
                <span class="mods-download-tray-progress">${info.progress || 0}%</span>
                <div class="mods-download-tray-bar">
                    <div class="mods-download-tray-bar-fill" style="width:${info.progress || 0}%"></div>
                </div>`;
            this.$trayItems.appendChild(row);
        }
    },

    buildRow(entry, context) {
        const row = document.createElement('div');
        row.className = 'mods-row';
        row.dataset.modId = entry.id;
        row.dataset.modKind = entry.kind;
        row.dataset.context = context;

        const initials = toInitials(entry.name);
        // If icon_url 404s (GitHub raw path that moved, catalog URL was never
        // uploaded, etc.), swap the broken <img> for the initials fallback so
        // the row doesn't render a visible placeholder glyph.
        const iconMarkup = entry.icon_url
            ? `<img class="mods-row-icon-img" src="${escapeHtml(entry.icon_url)}" alt="" onerror="this.outerHTML='<div class=&quot;mods-row-icon-fallback&quot;>${escapeHtml(initials).replace(/"/g, '&quot;')}</div>'" />`
            : `<div class="mods-row-icon-fallback">${escapeHtml(initials)}</div>`;

        const statusCell = this.buildStatusCell(entry, context);

        row.innerHTML = `
            <div class="mods-row-icon">${iconMarkup}</div>
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
                <button class="mods-row-overflow" data-action="overflow" aria-label="More">⋯</button>
            </div>
        `;
        return row;
    },

    buildStatusCell(entry, context) {
        if (context === 'browse') {
            if (entry.installed) {
                return `<span class="mods-row-badge installed" data-translate="MODS_INSTALLED">Installed</span>`;
            }
            return `<button class="mods-row-primary install" data-action="install" data-translate="MODS_ACTION_INSTALL">Install</button>`;
        }

        const download = this.state.downloads.get(entry.id);
        if (download) {
            return `<span class="mods-row-progress">${download.progress || 0}%</span>`;
        }

        switch (entry.status) {
            case 'running':
                return `
                    <span class="mods-row-badge running">
                        <span class="mods-row-running-dot"></span>
                        <span data-translate="MODS_STATUS_RUNNING">Running</span>
                    </span>
                    <button class="mods-row-secondary" data-action="stop" data-translate="MODS_ACTION_STOP">Stop</button>
                `;
            case 'starting':
                return `<span class="mods-row-badge starting" data-translate="MODS_STATUS_STARTING">Starting…</span>`;
            case 'enabled':
                return `<button class="mods-row-primary enabled" data-action="disable" data-translate="MODS_ACTION_DISABLE">Disable</button>`;
            case 'update_available':
                return `<button class="mods-row-primary update" data-action="update" data-translate="MODS_ACTION_UPDATE">Update</button>`;
            case 'error':
                return `<button class="mods-row-primary error" data-action="retry" data-translate="MODS_ACTION_RETRY">Retry</button>`;
            case 'installing':
                return `<span class="mods-row-progress">${entry.progress || 0}%</span>`;
            case 'disabled':
            default: {
                const action = entry.kind === 'external' ? 'launch' : 'enable';
                const label = entry.kind === 'external' ? 'Launch' : 'Enable';
                const key = entry.kind === 'external' ? 'MODS_ACTION_LAUNCH' : 'MODS_ACTION_ENABLE';
                return `<button class="mods-row-primary" data-action="${action}" data-translate="${key}">${label}</button>`;
            }
        }
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

        event.preventDefault();

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
                <span>Details</span>
            </button>
            ${sourceUrl
                ? `<button class="mods-row-popover-item" data-popover-action="source">
                      <span>Open source</span>
                   </button>`
                : ''}
            ${isInstalled
                ? `<button class="mods-row-popover-item danger" data-popover-action="uninstall">
                      <span>Uninstall</span>
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
                if (!window.confirm(`Uninstall "${entry.name}"?`)) return;
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
