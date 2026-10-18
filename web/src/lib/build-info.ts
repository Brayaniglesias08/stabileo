/**
 * Which build this is: the commit the bundle was built from, stamped by `vite.config.ts`.
 * `dev` where nothing stamped it (the test runner, a checkout without git).
 */
declare const __STABILEO_COMMIT__: string | undefined;

export const BUILD_COMMIT: string = typeof __STABILEO_COMMIT__ === 'string' && __STABILEO_COMMIT__ ? __STABILEO_COMMIT__ : 'dev';

/**
 * The app version, `web/package.json`'s `version`, stamped by `vite.config.ts`.
 * `dev` where nothing stamped it (a bundle built without the project's Vite config).
 */
declare const __STABILEO_VERSION__: string | undefined;

export const APP_VERSION: string = typeof __STABILEO_VERSION__ === 'string' && __STABILEO_VERSION__ ? __STABILEO_VERSION__ : 'dev';
