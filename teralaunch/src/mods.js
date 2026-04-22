/**
 * Mods view controller.
 *
 * Owns: the Installed / Browse tab UI, row rendering, primary-action state
 * machine, download tray, search/filter. Talks to the Rust backend via the
 * nine `commands::mods::*` Tauri commands.
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
    _globalDismissBound: false,

    t(key, fallback) {
        return window.App?.t?.(key) ?? fallback;
    },

    async open() {
        const backdrop = document.getElementById('mods-modal');
        if (!backdrop) return;

        if (!this._mounted) {
            const container = document.getElementById('mods-modal-content');
            try {
                const response = await fetch('./mods.html');
                const html = await response.text();
                container.innerHTML = html;
            } catch (e) {
                console.error('Failed to load mods.html:', e);
                container.innerHTML = `<div style="padding:24px;color:#f88">${escapeHtml(this.t('MODS_UI_LOAD_FAILED', 'Failed to load mods UI.'))}</div>`;
                return;
            }
            this.cacheDom();
            this.bindEvents();
            this._mounted = true;
            await Promise.all([this.loadInstalled(), this.loadCatalog(true)]);
            this.render();
            this.subscribeToProgress();
            if (window.App?.updateAllTranslations) {
                await window.App.updateAllTranslations();
            }
        } else {
            await Promise.all([this.loadInstalled(), this.loadCatalog(false)]);
            this.render();
        }

        // Re-wire close button every open to ensure it works even if DOM was swapped
        document.getElementById('mods-titlebar-close')?.addEventListener('click', (e) => {
            e.preventDefault();
            this.close();
        }, { once: true });

        this._bindModalDismissOnce(backdrop);
        backdrop.hidden = false;
        backdrop.setAttribute('aria-hidden', 'false');
        backdrop.style.display = 'flex';
    },

    close() {
        const backdrop = document.getElementById('mods-modal');
        if (!backdrop) return;
        backdrop.hidden = true;
        backdrop.setAttribute('aria-hidden', 'true');
        backdrop.style.display = 'none';
    },

    _bindModalDismissOnce(backdrop) {
        if (this._modalBound) return;
        this._modalBound = true;
        
        // Background click to close
        backdrop.addEventListener('click', (e) => {
            if (e.target === backdrop) this.close();
        });

        if (!this._globalDismissBound) {
            this._globalDismissBound = true;
            document.addEventListener('click', (e) => {
                const backdropEl = document.getElementById('mods-modal');
                if (!backdropEl || backdropEl.hidden) return;

                const closeBtn = e.target?.closest?.('#mods-titlebar-close');
                if (closeBtn) {
                    e.preventDefault();
                    e.stopPropagation();
                    this.close();
                }
            }, true);
        }

        // Escape key to close (priority to Detail panel)
        document.addEventListener('keydown', (e) => {
            if (e.key === 'Escape' && !backdrop.hidden) {
                const detail = document.getElementById('mods-detail-backdrop');
                if (!detail || detail.hidden) {
                    this.close();
                } else {
                    this.closeDetail();
                }
            }
        });
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

        // Close buttons (Main & Detail) — Re-wired on every mount for reliability
        document.getElementById('mods-titlebar-close')?.addEventListener('click', () => this.close());
        document.getElementById('mods-detail-close')?.addEventListener('click', () => this.closeDetail());

        // Tabs
        this.$page.querySelectorAll('.mods-tab').forEach(btn => {
            btn.addEventListener('click', () => this.setTab(btn.dataset.tab));
        });

        // Search
        if (this.$search) {
            this.$search.addEventListener('input', (e) => {
                this.state.query = e.target.value.trim().toLowerCase();
                this.$page.querySelector('#mods-search-wrap')?.classList.toggle('has-query', !!this.state.query);
                this.render();
            });
        }
        document.getElementById('mods-search-clear')?.addEventListener('click', () => {
            if (this.$search) {
                this.$search.value = '';
                this.state.query = '';
                this.$page.querySelector('#mods-search-wrap')?.classList.remove('has-query');
                this.render();
                this.$search.focus();
            }
        });

        // Filter chips (Kind)
        this.$page.querySelectorAll('.mods-filter-group .mods-filter-chip').forEach(btn => {
            btn.addEventListener('click', () => this.setFilter(btn.dataset.filter));
        });

        // Secondary Actions
        document.getElementById('mods-import-btn')?.addEventListener('click', async () => {
            try {
                const { open: openDialog } = window.__TAURI__.dialog;
                const selected = await openDialog({
                    multiple: false,
                    filters: [{ name: 'Mod Files', extensions: ['zip', 'gpk', 'exe'] }]
                });
                if (selected) {
                    this.state.downloads.set('manual-import', { progress: 0, state: 'installing' });
                    this.render();
                    await modsInvoke('install_mod', { entry: { id: 'manual-import', local_path: selected } });
                    await this.loadInstalled();
                    this.render();
                }
            } catch (e) { showModsError(this.t('MODS_IMPORT_FAILED', 'Import failed'), e); }
        });

        document.getElementById('mods-folder-btn')?.addEventListener('click', () => modsInvoke('open_mods_folder'));

        // Delegated clicks for mod rows
        const rowListener = (e) => this.handleRowClick(e);
        this.$installedExt?.addEventListener('click', rowListener);
        this.$installedGpk?.addEventListener('click', rowListener);
        this.$browseRows?.addEventListener('click', rowListener);

        // Detail backdrop click to close
        this.$detailBackdrop?.addEventListener('click', (e) => {
            if (e.target === this.$detailBackdrop) this.closeDetail();
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
        const panes = this.$page.querySelectorAll('.mods-pane');
        panes.forEach(p => p.classList.toggle('active', p.dataset.pane === tab));
        this.render();
    },

    setFilter(filter) {
        this.state.filter = filter;
        this.$page.querySelectorAll('.mods-filter-group .mods-filter-chip').forEach(btn => {
            btn.classList.toggle('active', btn.dataset.filter === filter);
        });
        this.render();
    },

    setCategory(cat) {
        this.state.category = cat;
        this.$categoryRow?.querySelectorAll('.mods-filter-chip').forEach(btn => {
            btn.classList.toggle('active', btn.dataset.category === cat);
        });
        this.render();
    },

    async loadInstalled() {
        try {
            this.state.installed = await modsInvoke('list_installed_mods');
            if (Array.isArray(this.state.catalog) && this.state.catalog.length) {
                const byId = new Map(this.state.catalog.map(c => [c.id, c]));
                this.state.installed.forEach(m => {
                    const cat = byId.get(m.id);
                    if (!cat) return;
                    m.icon_url = cat.icon_url || null;
                    m.source_url = cat.source_url || m.source_url;
                    m.license = cat.license || m.license;
                    m.credits = cat.credits || m.credits;
                    const skip = new Set(['installing', 'error', 'running', 'starting']);
                    if (!skip.has(m.status) && cat.version && m.version && cat.version !== m.version) {
                        m.status = 'update_available';
                    }
                });
            }
        } catch (e) { console.error('list_installed_mods failed:', e); this.state.installed = []; }
    },

    async loadCatalog(forceRefresh = false) {
        try {
            const catalog = await modsInvoke('get_mods_catalog', { forceRefresh });
            this.state.catalog = (catalog && Array.isArray(catalog.mods)) ? catalog.mods : [];
            this._catalogError = null;
        } catch (e) {
            console.warn('get_mods_catalog failed:', e);
            this.state.catalog = [];
            this._catalogError = String(e);
        }
        this.renderCategoryChips();
    },

    renderCategoryChips() {
        if (!this.$categoryRow) return;
        const seen = new Set();
        this.state.catalog.forEach(m => { if (m.category) seen.add(m.category); });
        const sorted = ['all', ...Array.from(seen).sort()];
        const chips = sorted.map(cat => {
            const isActive = this.state.category === cat;
            return `<button class="mods-filter-chip ${isActive ? 'active' : ''}" data-category="${cat}">${this.formatCategoryLabel(cat)}</button>`;
        });
        this.$categoryRow.innerHTML = chips.join('');
        this.$categoryRow.querySelectorAll('.mods-filter-chip').forEach(btn => {
            btn.addEventListener('click', () => this.setCategory(btn.dataset.category));
        });
    },

    formatCategoryLabel(cat) {
        if (!cat || cat === 'all') return this.t('MODS_CATEGORY_ALL', 'All categories');
        if (cat.length <= 3) return cat.toUpperCase();
        return cat.charAt(0).toUpperCase() + cat.slice(1);
    },

    async subscribeToProgress() {
        try {
            this._eventUnlisten = await modsListen('mod_download_progress', (event) => {
                const payload = event?.payload;
                if (!payload || !payload.id) return;
                if (payload.state === 'done' || payload.state === 'error') {
                    this.state.downloads.delete(payload.id);
                    this.loadInstalled().then(() => this.render());
                } else {
                    const pct = payload.progress || 0;
                    this.state.downloads.set(payload.id, {
                        progress: pct, state: payload.state || 'downloading',
                        received_bytes: payload.received_bytes || 0, total_bytes: payload.total_bytes || 0,
                    });
                    this.updateDownloadProgress(payload.id, pct, payload.received_bytes || 0, payload.total_bytes || 0);
                }
            });
        } catch (e) { console.warn('Progress subscribe failed:', e); }
    },

    updateDownloadProgress(id, pct, received, total) {
        document.querySelectorAll(`.mods-row[data-mod-id="${CSS.escape(id)}"]`).forEach(row => {
            const bar = row.querySelector('[data-progressbar]');
            if (bar) {
                const fill = bar.querySelector('.mods-row-progressbar-fill');
                const label = bar.querySelector('.mods-row-progressbar-label');
                if (fill) fill.style.width = `${pct}%`;
                if (label) label.textContent = `${pct}%`;
            } else {
                const status = row.querySelector('.mods-row-status');
                if (status) status.innerHTML = this.buildProgressBar(pct);
            }
        });
        this.updateDownloadTrayItem(id, pct, received, total);
    },

    updateDownloadTrayItem(id, pct, received, total) {
        if (!this.$tray || !this.$trayItems) return;
        if (this.$tray.hidden) { this.renderDownloadTray(); return; }
        const item = this.$trayItems.querySelector(`.mods-download-tray-item[data-dl-id="${CSS.escape(id)}"]`);
        if (!item) { this.renderDownloadTray(); return; }
        const bar = item.querySelector('.mods-download-tray-bar-fill');
        const label = item.querySelector('.mods-download-tray-progress');
        const detail = item.querySelector('.mods-download-tray-bytes');
        if (bar) bar.style.width = `${pct}%`;
        if (label) label.textContent = `${pct}%`;
        if (detail && total > 0) detail.textContent = `${formatMB(received)} / ${formatMB(total)}`;
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
            const hay = [entry.name, entry.author, entry.category, entry.description || entry.short_description || ''].filter(Boolean).join(' ').toLowerCase();
            if (!hay.includes(this.state.query)) return false;
        }
        return true;
    },

    render() {
        if (!this.$page) return;
        const installedIds = new Set(this.state.installed.map(m => m.id));
        if (this.$countInstalled) this.$countInstalled.textContent = this.state.installed.length;
        if (this.$countBrowse) this.$countBrowse.textContent = this.state.catalog.filter(e => !installedIds.has(e.id)).length;

        if (this.state.tab === 'installed') {
            const ext = this.state.installed.filter(m => m.kind === 'external' && this.filterMatches(m));
            const gpk = this.state.installed.filter(m => m.kind === 'gpk' && this.filterMatches(m));
            this.renderInstalledGroup(this.$installedExt, ext, 'external');
            this.renderInstalledGroup(this.$installedGpk, gpk, 'gpk');
            if (this.$installedEmpty) this.$installedEmpty.hidden = (ext.length + gpk.length > 0);
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
        entries.forEach(e => container.appendChild(this.buildRow(e, 'installed')));
    },

    renderBrowse() {
        if (!this.$browseRows) return;
        const installedIds = new Set(this.state.installed.map(m => m.id));
        const visible = this.state.catalog.filter(e => !installedIds.has(e.id) && this.filterMatches(e));
        this.$browseRows.innerHTML = '';
        visible.forEach(e => this.$browseRows.appendChild(this.buildRow(e, 'browse')));
        if (this.$browseEmpty) {
            this.$browseEmpty.hidden = visible.length > 0;
            if (visible.length === 0 && this._catalogError) {
                this.$browseEmpty.innerHTML = `<p>Catalog unavailable.</p><p style="opacity:.55;font-size:12px;margin-top:6px;">${escapeHtml(this._catalogError)}</p>`;
            }
        }
    },

    renderDownloadTray() {
        if (!this.$tray || !this.$trayItems) return;
        if (this.state.downloads.size === 0) { this.$tray.hidden = true; return; }
        this.$tray.hidden = false;
        this.$trayItems.innerHTML = '';
        for (const [id, info] of this.state.downloads) {
            const entry = this.state.installed.find(m => m.id === id) || this.state.catalog.find(m => m.id === id);
            const name = entry ? entry.name : id;
            const pct = info.progress || 0;
            const row = document.createElement('div');
            row.className = 'mods-download-tray-item';
            row.innerHTML = `<div class="mods-download-tray-item-header"><span>${escapeHtml(name)}</span><span>${pct}%</span></div>
                             <div class="mods-download-tray-bar"><div class="mods-download-tray-bar-fill" style="width:${pct}%"></div></div>`;
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
        const icon = entry.icon_url ? `<div class="mods-row-icon"><img class="mods-row-icon-img" src="${escapeHtml(entry.icon_url)}" alt="" /></div>` : '';
        row.innerHTML = `${icon}<div class="mods-row-body"><div class="mods-row-title"><span class="mods-row-name">${escapeHtml(entry.name)}</span><span class="mods-row-author">${escapeHtml(entry.author || '')}</span></div>
                         <div class="mods-row-desc">${escapeHtml(entry.description || '')}</div></div>
                         <div class="mods-row-status">${this.buildStatusCell(entry, context)}</div>
                         <div class="mods-row-menu"><button class="mods-row-overflow" data-action="overflow">⋯</button></div>`;
        return row;
    },

    buildStatusCell(entry, context) {
        if (context === 'browse') return `<button class="mods-row-primary install" data-action="install">Install</button>`;
        const download = this.state.downloads.get(entry.id);
        if (download) return this.buildProgressBar(download.progress || 0);
        if (entry.status === 'error') return `<button class="mods-row-primary error" data-action="retry">Retry</button>`;
        if (entry.status === 'update_available') return `<button class="mods-row-primary update" data-action="update">Update</button>`;
        const enabled = entry.enabled || entry.status === 'enabled' || entry.status === 'running' || entry.status === 'starting';
        return `<label class="mods-row-toggle"><input type="checkbox" data-action="toggle" ${enabled ? 'checked' : ''} /><span class="mods-row-toggle-track"><span class="mods-row-toggle-thumb"></span></span></label>
                ${entry.status === 'running' ? `<span class="mods-row-running-pill"><span class="mods-row-running-dot"></span>Active</span>` : ''}`;
    },

    buildProgressBar(pct) {
        const clamped = Math.max(0, Math.min(100, Math.round(pct)));
        return `<div class="mods-row-progressbar" data-progressbar><div class="mods-row-progressbar-track"><div class="mods-row-progressbar-fill" style="width:${clamped}%"></div></div>
                <span class="mods-row-progressbar-label">${clamped}%</span></div>`;
    },

    async handleRowClick(event) {
        const btn = event.target.closest('[data-action]');
        const row = event.target.closest('.mods-row');
        if (!btn) { if (row && event.target.closest('.mods-row-body')) this.openDetail(row.dataset.modId, row.dataset.context); return; }
        if (!row) return;
        const id = row.dataset.modId;
        const action = btn.dataset.action;
        if (action !== 'toggle') event.preventDefault();
        try {
            switch (action) {
                case 'install':
                case 'update':
                case 'retry':
                    const cat = this.state.catalog.find(m => m.id === id);
                    if (!cat) return;
                    this.state.downloads.set(id, { progress: 0, state: 'downloading' });
                    this.render();
                    await modsInvoke('install_mod', { entry: cat });
                    await this.loadInstalled();
                    this.render();
                    break;
                case 'toggle':
                    const cb = btn;
                    const target = cb.checked;
                    try { await modsInvoke(target ? 'enable_mod' : 'disable_mod', { id }); } catch (err) { cb.checked = !target; throw err; }
                    await this.loadInstalled();
                    this.render();
                    break;
                case 'overflow':
                    await this.showOverflowMenu(id, btn);
                    break;
            }
        } catch (e) { showModsError(`Action failed: ${action}`, e); }
    },

    openDetail(id, context) {
        if (!this.$detailBackdrop || !id) return;
        const inst = this.state.installed.find(m => m.id === id);
        const cat = this.state.catalog.find(m => m.id === id);
        const entry = context === 'browse' ? (cat || inst) : (inst || cat);
        if (!entry) return;
        this.$detailName.textContent = entry.name || id;
        this.$detailAuthor.textContent = entry.author || '—';
        this.$detailVersion.textContent = entry.version ? `v${entry.version}` : '';
        this.$detailDescription.textContent = entry.long_description || entry.description || cat?.short_description || '';
        this.$detailFactAuthor.textContent = entry.author || '—';
        this.$detailIcon.innerHTML = entry.icon_url ? `<img src="${escapeHtml(entry.icon_url)}" alt="" />` : toInitials(entry.name || id);
        const screens = entry.screenshots || cat?.screenshots || [];
        this.$detailScreenshotsSection.hidden = (screens.length === 0);
        this.$detailScreenshots.innerHTML = screens.map(url => `<img src="${escapeHtml(url)}" alt="" loading="lazy" />`).join('');
        this.$detailBackdrop.hidden = false;
    },

    closeDetail() { if (this.$detailBackdrop) this.$detailBackdrop.hidden = true; },

    modalConfirm({ title, body = '', confirmLabel = 'Confirm', cancelLabel = 'Cancel', danger = false }) {
        return new Promise(resolve => {
            const backdrop = document.createElement('div');
            backdrop.className = 'mods-confirm-backdrop';
            backdrop.innerHTML = `<div class="mods-confirm-card"><h3 class="mods-confirm-title">${escapeHtml(title)}</h3><p class="mods-confirm-body">${escapeHtml(body)}</p>
                                  <div class="mods-confirm-actions"><button type="button" class="mods-onboarding-btn secondary" data-confirm-action="cancel">${escapeHtml(cancelLabel)}</button>
                                  <button type="button" class="mods-onboarding-btn ${danger ? 'danger' : 'primary'}" data-confirm-action="ok">${escapeHtml(confirmLabel)}</button></div></div>`;
            document.body.appendChild(backdrop);
            const finish = (v) => { backdrop.remove(); document.removeEventListener('keydown', kh, true); resolve(v); };
            const kh = (e) => { if (e.key === 'Escape') { e.stopPropagation(); finish(false); } if (e.key === 'Enter') { e.stopPropagation(); finish(true); } };
            backdrop.addEventListener('click', e => {
                const btn = e.target.closest('[data-confirm-action]');
                if (btn) finish(btn.dataset.confirmAction === 'ok'); else if (e.target === backdrop) finish(false);
            });
            document.addEventListener('keydown', kh, true);
        });
    },

    async showOverflowMenu(id, anchor) {
        const entry = this.state.installed.find(m => m.id === id) || this.state.catalog.find(m => m.id === id);
        if (!entry) return;
        document.querySelectorAll('.mods-row-popover').forEach(el => el.remove());
        const popover = document.createElement('div');
        popover.className = 'mods-row-popover';
        const isInst = this.state.installed.some(m => m.id === id);
        popover.innerHTML = `<button class="mods-row-popover-item" data-popover-action="details">Details</button>
                             ${isInst ? `<button class="mods-row-popover-item danger" data-popover-action="uninstall">Uninstall</button>` : ''}`;
        const rect = anchor.getBoundingClientRect();
        popover.style.top = `${rect.bottom + 6}px`;
        popover.style.right = `${Math.max(16, window.innerWidth - rect.right)}px`;
        document.body.appendChild(popover);
        const dismiss = () => { popover.remove(); document.removeEventListener('click', oc, true); document.removeEventListener('keydown', ek, true); };
        const oc = (e) => { if (!popover.contains(e.target) && e.target !== anchor) dismiss(); };
        const ek = (e) => { if (e.key === 'Escape') dismiss(); };
        popover.addEventListener('click', async (e) => {
            const action = e.target.closest('[data-popover-action]')?.dataset.popoverAction;
            if (!action) return;
            dismiss();
            if (action === 'details') this.openDetail(id, isInst ? 'installed' : 'browse');
            else if (action === 'uninstall') {
                const ok = await this.modalConfirm({ title: `Uninstall "${entry.name}"?`, body: 'Mod files will be removed.', confirmLabel: 'Uninstall', danger: true });
                if (ok) { await modsInvoke('uninstall_mod', { id, deleteSettings: null }); await this.loadInstalled(); this.render(); }
            }
        });
        setTimeout(() => { document.addEventListener('click', oc, true); document.addEventListener('keydown', ek, true); }, 0);
    }
};

function formatMB(bytes) { return bytes ? `${(bytes / (1024 * 1024)).toFixed(1)} MB` : '0 MB'; }
function toInitials(name) { if (!name) return '??'; const p = name.split(/\s+/).filter(Boolean); return p.length === 0 ? '??' : (p.length === 1 ? p[0].slice(0, 2).toUpperCase() : (p[0][0] + p[1][0]).toUpperCase()); }
function escapeHtml(s) { return s == null ? '' : String(s).replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;').replace(/'/g, '&#39;'); }
function showModsError(t, d) { if (typeof window.showUpdateNotification === 'function') window.showUpdateNotification('error', t, d?.message || String(d || '')); else console.error(`[Mods] ${t}:`, d); }

if (typeof window !== 'undefined') {
    window.ModsView = ModsView;
    window.initMods = async function () { await ModsView.open(); };
}
export { ModsView };
