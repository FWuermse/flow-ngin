# Hitbox Playground

The browser example is a technical demo hosted at `hitbox.fortressflow.com`.
It does not require Google sign-in or store an authentication token.

Deploy `index.html`, `styles.css`, `legal.css`, and `legal.js` alongside the
generated `pkg/` WASM output. Impressum, privacy, and terms dialogs remain
available independently of WASM startup and support direct URL fragments
(`#impressum`, `#datenschutz`, and `#terms`).

The legal text is adapted from the supplied Fortressflow page, retaining its
hosting/CDN details. Fill in the actual server-log retention period in
`index.html` before publication; the supplied value is still a placeholder.
