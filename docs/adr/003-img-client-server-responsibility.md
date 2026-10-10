
## Problem
Using a backend headless browser (`chromedp`) in `woodhouse` to fetch URLs hits paywalls and requires outbound network access, breaking container isolation. Furthermore, leaving remote `<img>` tags in the generated Markdown leaks the server IP upon viewing and results in broken images if the remote host deletes them. Attempting to download images backend-side fails for authenticated content.

READABILITY.JS and naturalwidth/hieight > x px

## Decision
Move the initial HTML rendering and asset extraction entirely to the client (Firefox).

1. A client-side script parses the active, authenticated DOM.
2. It strips trackers/UI garbage and extracts valid images as base64/binary payloads.
3. It sends a multipart payload (Clean HTML + Images) to `cheryl`.
4. `cheryl` persists the payload and images into NATS Object Stores, rewriting the `<img>` `src` paths to local `cheryl` URLs.
5. `woodhouse` reads the HTML from NATS and runs a pure string-to-markdown conversion.

## Consequences
- **Positive:** Deletes Chromium from the backend, massively reducing RAM and container size.
- **Positive:** Achieves 100% network isolation for `woodhouse` (no outbound requests).
- **Positive:** Bypasses paywalls and captures images using the user's active session cookies.
- **Negative:** Increases network payload size between the laptop and the server.
