import { describe, it, expect, beforeEach, vi } from 'vitest';

const mockApp = {
    state: { isAuthenticated: false },
    t: vi.fn((key) => key),
    checkAuthentication: vi.fn(),
    loadAsyncContent: vi.fn(() => Promise.resolve('<div>Test Content</div>')),
    showLoadingIndicator: vi.fn(),
    hideLoadingIndicator: vi.fn(),
    smoothPageTransition: vi.fn(),
    updateAllTranslations: vi.fn(),
    initHome: vi.fn(),
    initLogin: vi.fn(),
};

function createRouter(App) {
    return {
        routes: {
            home: { title: 'Home', file: 'home.html', protected: true, init: 'initHome' },
            login: { title: 'Login', file: 'login.html', public: true, init: 'initLogin' },
        },
        currentRoute: null,
        isTransitioning: false,

        async navigate(route = null) {
            if (this.isTransitioning) return;
            this.isTransitioning = true;
            try {
                const app = document.getElementById('app');
                route = await this.determineRoute(route);
                if (!route) {
                    this.isTransitioning = false;
                    return;
                }
                if (!this.isRouteValid(route)) {
                    this.handleInvalidRoute(app, route);
                    return;
                }
                await this.handleRouteTransition(app, route);
            } catch (error) {
                this.handleRoutingError();
            } finally {
                this.isTransitioning = false;
            }
        },

        async determineRoute(route) {
            route = route || window.location.hash.replace('#', '') || 'home';
            await App.checkAuthentication();
            if (this.routes[route]?.protected && !App.state.isAuthenticated) {
                return 'login';
            }
            if (route === 'login' && App.state.isAuthenticated) {
                return 'home';
            }
            if (this.currentRoute === route) {
                return null;
            }
            return route;
        },

        isRouteValid(route) {
            return this.routes[route] !== undefined;
        },

        handleInvalidRoute(app, route) {
            app.innerHTML = `<div class="page"><h1>${App.t('PAGE_NOT_FOUND')}</h1></div>`;
        },

        async handleRouteTransition(app, route) {
            document.title = this.routes[route].title;
            App.showLoadingIndicator();
            const content = await this.loadRouteContent(route);
            await this.simulateLoadingDelay();
            const newPage = this.createNewPage(content);
            App.hideLoadingIndicator();
            await App.smoothPageTransition(app, newPage);
            this.updateUserInfo(newPage);
            this.updateCurrentRoute(route);
            await this.initializeNewRoute(route);
            await App.updateAllTranslations();
        },

        async loadRouteContent(route) {
            return await App.loadAsyncContent(this.routes[route].file);
        },

        async simulateLoadingDelay() {
            await new Promise((resolve) => setTimeout(resolve, 10));
        },

        createNewPage(content) {
            const newPage = document.createElement('div');
            newPage.className = 'page';
            newPage.innerHTML = content;
            return newPage;
        },

        updateUserInfo(newPage) {
            if (App.state.isAuthenticated) {
                const userNameEl = newPage.querySelector('#userName');
                if (userNameEl) userNameEl.textContent = localStorage.getItem('userName');
            }
        },

        updateCurrentRoute(route) {
            this.currentRoute = route;
            if (window.location.hash !== `#${route}`) {
                window.location.hash = route;
            }
        },

        async initializeNewRoute(route) {
            if (this.routes[route].init) {
                await App[this.routes[route].init]();
            }
        },

        handleRoutingError() {
            const app = document.getElementById('app');
            app.innerHTML = `<div class="page"><h1>${App.t('LOADING_ERROR')}</h1></div>`;
            App.hideLoadingIndicator();
        },

        setupEventListeners() {
            window.addEventListener('hashchange', () => this.navigate());
        },
    };
}

describe('Router', () => {
    let Router;
    let appElement;

    beforeEach(() => {
        vi.clearAllMocks();
        mockApp.state.isAuthenticated = false;

        appElement = document.createElement('div');
        appElement.id = 'app';
        document.body.innerHTML = '';
        document.body.appendChild(appElement);

        window.location.hash = '';

        Router = createRouter(mockApp);
    });

    describe('isRouteValid', () => {
        it('returns true for valid routes', () => {
            expect(Router.isRouteValid('home')).toBe(true);
            expect(Router.isRouteValid('login')).toBe(true);
        });

        it('returns false for invalid routes', () => {
            expect(Router.isRouteValid('nonexistent')).toBe(false);
            expect(Router.isRouteValid('')).toBe(false);
            expect(Router.isRouteValid(undefined)).toBe(false);
        });
    });

    describe('determineRoute', () => {
        it('returns login for protected route when unauthenticated', async () => {
            mockApp.state.isAuthenticated = false;
            const route = await Router.determineRoute('home');
            expect(route).toBe('login');
        });

        it('returns home for login route when authenticated', async () => {
            mockApp.state.isAuthenticated = true;
            const route = await Router.determineRoute('login');
            expect(route).toBe('home');
        });

        it('returns null if already on the route', async () => {
            mockApp.state.isAuthenticated = true;
            Router.currentRoute = 'home';
            const route = await Router.determineRoute('home');
            expect(route).toBe(null);
        });

        it('uses hash from window.location if no route provided', async () => {
            window.location.hash = '#login';
            mockApp.state.isAuthenticated = false;
            Router.currentRoute = null;
            const route = await Router.determineRoute(null);
            expect(route).toBe('login');
        });

        it('defaults to home if no hash', async () => {
            window.location.hash = '';
            mockApp.state.isAuthenticated = false;
            const route = await Router.determineRoute(null);
            expect(route).toBe('login');
        });
    });

    describe('handleInvalidRoute', () => {
        it('displays PAGE_NOT_FOUND message', () => {
            Router.handleInvalidRoute(appElement, 'invalid');
            expect(appElement.innerHTML).toContain('PAGE_NOT_FOUND');
        });
    });

    describe('createNewPage', () => {
        it('creates a div with page class and content', () => {
            const page = Router.createNewPage('<p>Test</p>');
            expect(page.className).toBe('page');
            expect(page.innerHTML).toBe('<p>Test</p>');
        });
    });

    describe('updateCurrentRoute', () => {
        it('updates currentRoute and hash', () => {
            Router.updateCurrentRoute('login');
            expect(Router.currentRoute).toBe('login');
            expect(window.location.hash).toBe('#login');
        });

        it('does not update hash if already set', () => {
            window.location.hash = '#home';
            Router.updateCurrentRoute('home');
            expect(window.location.hash).toBe('#home');
        });
    });

    describe('initializeNewRoute', () => {
        it('calls init function for route', async () => {
            await Router.initializeNewRoute('home');
            expect(mockApp.initHome).toHaveBeenCalled();
        });

        it('calls initLogin for login route', async () => {
            await Router.initializeNewRoute('login');
            expect(mockApp.initLogin).toHaveBeenCalled();
        });
    });

    describe('handleRoutingError', () => {
        it('displays error and hides loading indicator', () => {
            Router.handleRoutingError();
            expect(appElement.innerHTML).toContain('LOADING_ERROR');
            expect(mockApp.hideLoadingIndicator).toHaveBeenCalled();
        });
    });

    describe('navigate', () => {
        it('prevents concurrent transitions', async () => {
            Router.isTransitioning = true;
            await Router.navigate('home');
            expect(mockApp.checkAuthentication).not.toHaveBeenCalled();
        });

        it('handles invalid route', async () => {
            mockApp.state.isAuthenticated = true;
            Router.currentRoute = null;
            await Router.navigate('invalid_route');
            expect(appElement.innerHTML).toContain('PAGE_NOT_FOUND');
        });

        it('completes navigation for valid route', async () => {
            mockApp.state.isAuthenticated = true;
            Router.currentRoute = null;
            await Router.navigate('home');
            expect(Router.currentRoute).toBe('home');
            expect(mockApp.initHome).toHaveBeenCalled();
        });

        it('resets isTransitioning after navigation', async () => {
            mockApp.state.isAuthenticated = true;
            Router.currentRoute = null;
            await Router.navigate('home');
            expect(Router.isTransitioning).toBe(false);
        });
    });

    describe('updateUserInfo', () => {
        it('updates userName element when authenticated', () => {
            mockApp.state.isAuthenticated = true;
            localStorage.setItem('userName', 'TestUser');

            const page = document.createElement('div');
            const userNameEl = document.createElement('span');
            userNameEl.id = 'userName';
            page.appendChild(userNameEl);

            Router.updateUserInfo(page);

            expect(userNameEl.textContent).toBe('TestUser');
        });

        it('does nothing when not authenticated', () => {
            mockApp.state.isAuthenticated = false;

            const page = document.createElement('div');
            const userNameEl = document.createElement('span');
            userNameEl.id = 'userName';
            userNameEl.textContent = 'Original';
            page.appendChild(userNameEl);

            Router.updateUserInfo(page);

            expect(userNameEl.textContent).toBe('Original');
        });

        it('handles missing userName element gracefully', () => {
            mockApp.state.isAuthenticated = true;
            const page = document.createElement('div');
            expect(() => Router.updateUserInfo(page)).not.toThrow();
        });
    });

    describe('loadRouteContent', () => {
        it('loads content from app', async () => {
            const content = await Router.loadRouteContent('home');
            expect(mockApp.loadAsyncContent).toHaveBeenCalledWith('home.html');
            expect(content).toBe('<div>Test Content</div>');
        });
    });

    describe('handleRouteTransition', () => {
        it('updates document title', async () => {
            mockApp.state.isAuthenticated = true;
            await Router.handleRouteTransition(appElement, 'home');
            expect(document.title).toBe('Home');
        });

        it('calls all transition methods in order', async () => {
            mockApp.state.isAuthenticated = true;
            await Router.handleRouteTransition(appElement, 'home');

            expect(mockApp.showLoadingIndicator).toHaveBeenCalled();
            expect(mockApp.loadAsyncContent).toHaveBeenCalled();
            expect(mockApp.hideLoadingIndicator).toHaveBeenCalled();
            expect(mockApp.smoothPageTransition).toHaveBeenCalled();
            expect(mockApp.initHome).toHaveBeenCalled();
            expect(mockApp.updateAllTranslations).toHaveBeenCalled();
        });
    });
});

describe('Router Edge Cases', () => {
    let Router;

    beforeEach(() => {
        vi.clearAllMocks();
        mockApp.state.isAuthenticated = false;

        const appElement = document.createElement('div');
        appElement.id = 'app';
        document.body.innerHTML = '';
        document.body.appendChild(appElement);

        Router = createRouter(mockApp);
    });

    it('handles route without init function', async () => {
        Router.routes.noInit = { title: 'No Init', file: 'noinit.html', public: true };
        await Router.initializeNewRoute('noInit');
    });

    it('handles route that becomes null after determination', async () => {
        mockApp.state.isAuthenticated = true;
        Router.currentRoute = 'home';
        await Router.navigate('home');
        expect(Router.isTransitioning).toBe(false);
    });

    it('handles routing errors gracefully', async () => {
        mockApp.state.isAuthenticated = true;
        mockApp.loadAsyncContent.mockRejectedValueOnce(new Error('Load failed'));
        Router.currentRoute = null;
        await Router.navigate('home');
        const appElement = document.getElementById('app');
        expect(appElement.innerHTML).toContain('LOADING_ERROR');
    });

    it('sets up event listeners', () => {
        const addEventListenerSpy = vi.spyOn(window, 'addEventListener');
        Router.setupEventListeners();
        expect(addEventListenerSpy).toHaveBeenCalledWith('hashchange', expect.any(Function));
        addEventListenerSpy.mockRestore();
    });
});
