# Product screenshots

The bilingual README uses six scenarios rendered with current production React components: library overview, document reading, installation, batch tasks, import review and repository update review. All content is fictional; these images demonstrate the UI, not completed remote operations.

## Reproduce

1. Install the project's dependencies and start a dedicated development server:

   ```sh
   pnpm exec vite --host 127.0.0.1 --port 24208
   ```

2. Provide Playwright through your development environment and install its Chromium browser (`playwright install chromium`), or set `CHROME_PATH` to an existing Chromium executable. Playwright is optional documentation tooling, not a runtime dependency.
3. Run `node docs/screenshots/capture.cjs`. Set `SCREENSHOT_URL` if using another local port. The script writes twelve PNGs into `docs/images/product/`.

Inspect a scene interactively at `/docs/screenshots/index.html?lang=zh&scene=review`. Scenes: `library`, `document`, `install`, `tasks`, `import`, `review`; languages: `zh`, `en`.

The fixture uses its own browser context, resets only that origin's localStorage, substitutes read-only IPC responses and reads no desktop configuration or library data. Use a dedicated local development origin. The capture script blocks outbound requests. Do not use personal data in the fixture. The production Vite build rejects screenshot-demo modules if they enter the application dependency graph.

Images use a 1440 × 900 viewport and 1.5× scale, with fonts loaded and animations disabled. Inspect both languages after capture, especially dialog footers, long labels and conflict controls.

Type-check the fixture with `pnpm exec tsc --noEmit -p docs/screenshots/tsconfig.json`.

## Editorial references

The structure was informed by [LocalSend](https://github.com/localsend/localsend) (brief introduction, screenshots, clear download path), [BongoCat](https://github.com/ayangweb/BongoCat) (visual introduction and concise features) and [Zen Browser](https://github.com/zen-browser/desktop) (product identity and a direct entry point). Text and screenshots are original to Arsenal; no other project's UI or product claims are reused.
