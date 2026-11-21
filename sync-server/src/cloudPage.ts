const CLOUD_PAGE_HTML = `<!doctype html>
<html lang="en">
  <head>
    <meta charset="UTF-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1.0" />
    <meta
      name="description"
      content="Join early access for rem Cloud — optional hosted sync for rem across desktop, CLI, and the upcoming iPhone companion. Local storage and open-source self-hosting stay free."
    />
    <meta name="theme-color" content="#f8f7f4" />
    <title>rem Cloud — early access</title>
    <style>
      :root {
        --bg: #f8f7f4;
        --surface: #ffffff;
        --surface-alt: #eeece7;
        --ink: #1c1917;
        --muted: #6f6965;
        --faint: #99928c;
        --line: #dedbd5;
        --line-soft: #eae8e3;
        --purple: #5a47e8;
        --purple-dark: #4937d0;
        --green: #20875a;
        --danger: #b42318;
        --sans: -apple-system, BlinkMacSystemFont, 'Segoe UI', Helvetica, Arial, sans-serif;
        --serif: Georgia, 'Times New Roman', serif;
        --mono: ui-monospace, 'SFMono-Regular', Menlo, Consolas, monospace;
        color-scheme: light;
      }

      * {
        box-sizing: border-box;
      }

      body {
        margin: 0;
        min-width: 320px;
        background: var(--bg);
        color: var(--ink);
        font-family: var(--sans);
        font-size: 16px;
        line-height: 1.6;
        -webkit-font-smoothing: antialiased;
      }

      a {
        color: inherit;
        text-decoration: none;
      }

      .shell {
        width: min(1080px, calc(100% - 40px));
        margin-inline: auto;
      }

      .site-header {
        border-bottom: 1px solid var(--line);
        background: rgba(248, 247, 244, 0.9);
      }

      .nav {
        display: flex;
        align-items: center;
        justify-content: space-between;
        min-height: 68px;
        gap: 20px;
      }

      .brand {
        display: inline-flex;
        align-items: center;
        gap: 8px;
        font-size: 20px;
        font-weight: 700;
        letter-spacing: -0.04em;
      }

      .brand-dot {
        width: 6px;
        height: 6px;
        border-radius: 999px;
        background: var(--purple);
      }

      .nav-links {
        display: flex;
        align-items: center;
        gap: 24px;
        color: var(--muted);
        font-size: 14px;
        font-weight: 500;
      }

      .nav-links a:hover {
        color: var(--purple);
      }

      .hero {
        display: grid;
        grid-template-columns: minmax(0, 1.05fr) minmax(320px, 0.95fr);
        align-items: start;
        gap: 64px;
        padding-block: 76px 96px;
      }

      .eyebrow {
        display: flex;
        align-items: center;
        gap: 10px;
        margin: 0 0 20px;
        color: var(--purple);
        font-family: var(--mono);
        font-size: 11px;
        letter-spacing: 0.1em;
        text-transform: uppercase;
      }

      .eyebrow span {
        width: 18px;
        height: 1px;
        background: currentColor;
      }

      h1,
      h2 {
        margin: 0;
        font-family: var(--serif);
        font-weight: 400;
        line-height: 1.04;
        letter-spacing: -0.03em;
      }

      h1 {
        font-size: clamp(44px, 5vw, 66px);
      }

      h1 em,
      h2 em {
        color: var(--purple);
        font-style: italic;
      }

      .lede {
        margin: 24px 0 0;
        max-width: 540px;
        color: var(--muted);
        font-size: 17px;
        line-height: 1.68;
      }

      .highlights {
        display: grid;
        gap: 10px;
        margin: 28px 0 0;
        padding: 0;
        list-style: none;
        color: #4f4a46;
        font-size: 14px;
      }

      .highlights li {
        display: flex;
        align-items: baseline;
        gap: 10px;
      }

      .highlights span {
        color: var(--purple);
        font-weight: 700;
      }

      .card {
        padding: 34px;
        border: 1px solid var(--line);
        border-radius: 14px;
        background: var(--surface);
        box-shadow: 0 20px 50px rgba(31, 26, 55, 0.08), 0 2px 8px rgba(31, 26, 55, 0.04);
      }

      .card-kicker {
        margin: 0 0 10px;
        color: var(--purple);
        font-family: var(--mono);
        font-size: 10px;
        letter-spacing: 0.1em;
        text-transform: uppercase;
      }

      .card h2 {
        font-size: 34px;
      }

      .card-desc {
        margin: 12px 0 0;
        color: var(--muted);
        font-size: 14px;
        line-height: 1.65;
      }

      .form {
        display: grid;
        gap: 16px;
        margin-top: 24px;
      }

      .field {
        display: grid;
        gap: 6px;
      }

      .field label {
        font-size: 13px;
        font-weight: 600;
      }

      .field label span {
        font-weight: 400;
        color: var(--faint);
      }

      .field input {
        width: 100%;
        min-height: 44px;
        padding: 0 14px;
        border: 1px solid var(--line);
        border-radius: 8px;
        background: var(--bg);
        color: var(--ink);
        font: inherit;
        font-size: 14px;
      }

      .field input:focus {
        outline: none;
        border-color: var(--purple);
        background: var(--surface);
        box-shadow: 0 0 0 3px rgba(90, 71, 232, 0.14);
      }

      .hp {
        position: absolute;
        width: 1px;
        height: 1px;
        overflow: hidden;
        clip: rect(0 0 0 0);
        white-space: nowrap;
      }

      .btn {
        display: inline-flex;
        align-items: center;
        justify-content: center;
        width: 100%;
        min-height: 48px;
        padding: 0 20px;
        border: 0;
        border-radius: 8px;
        background: var(--purple);
        color: #fff;
        font: inherit;
        font-size: 14px;
        font-weight: 600;
        cursor: pointer;
        box-shadow: 0 8px 20px rgba(90, 71, 232, 0.22);
      }

      .btn:hover {
        background: var(--purple-dark);
      }

      .btn:disabled {
        opacity: 0.7;
        cursor: wait;
      }

      .feedback {
        margin: 0;
        min-height: 1.4em;
        font-size: 13px;
        line-height: 1.5;
      }

      .feedback.is-success {
        color: var(--green);
      }

      .feedback.is-error {
        color: var(--danger);
      }

      .footnote {
        margin: 18px 0 0;
        padding-top: 16px;
        border-top: 1px solid var(--line-soft);
        color: var(--muted);
        font-size: 12px;
        line-height: 1.6;
      }

      .modes {
        padding-block: 84px;
        border-top: 1px solid var(--line);
        background: var(--surface-alt);
      }

      .modes-head {
        max-width: 620px;
        margin: 0 auto 48px;
        text-align: center;
      }

      .modes-head .eyebrow {
        justify-content: center;
      }

      .modes-head h2 {
        font-size: clamp(36px, 4vw, 50px);
      }

      .modes-head p:last-child {
        margin: 16px 0 0;
        color: var(--muted);
        font-size: 15px;
      }

      .compare {
        display: grid;
        grid-template-columns: repeat(3, 1fr);
        gap: 20px;
      }

      .compare article {
        display: flex;
        flex-direction: column;
        padding: 28px;
        border: 1px solid var(--line);
        border-radius: 12px;
        background: rgba(255, 255, 255, 0.74);
      }

      .compare article.is-featured {
        border-color: rgba(90, 71, 232, 0.35);
        background: var(--surface);
        box-shadow: 0 12px 30px rgba(90, 71, 232, 0.08);
      }

      .tier-label {
        margin: 0 0 12px;
        color: var(--faint);
        font-family: var(--mono);
        font-size: 10px;
        letter-spacing: 0.08em;
        text-transform: uppercase;
      }

      .compare article.is-featured .tier-label {
        color: var(--purple);
      }

      .compare h3 {
        margin: 0 0 10px;
        font-size: 18px;
      }

      .compare article > p:not(.tier-label) {
        margin: 0 0 22px;
        color: var(--muted);
        font-size: 14px;
        line-height: 1.65;
        flex: 1;
      }

      .tier-link {
        color: var(--purple);
        font-size: 13px;
        font-weight: 600;
      }

      .tier-link:hover {
        text-decoration: underline;
        text-underline-offset: 3px;
      }

      footer {
        border-top: 1px solid var(--line);
        padding-block: 28px;
        color: var(--muted);
        font-size: 12px;
      }

      .footer-inner {
        display: flex;
        flex-wrap: wrap;
        align-items: center;
        justify-content: space-between;
        gap: 16px;
      }

      .footer-links {
        display: flex;
        flex-wrap: wrap;
        gap: 20px;
      }

      .footer-links a:hover {
        color: var(--purple);
      }

      @media (max-width: 860px) {
        .hero {
          grid-template-columns: 1fr;
          gap: 40px;
          padding-block: 52px 72px;
        }

        .compare {
          grid-template-columns: 1fr;
        }
      }
    </style>
  </head>
  <body>
    <header class="site-header">
      <nav class="nav shell" aria-label="Main navigation">
        <a class="brand" href="https://shettyh.github.io/rem/">
          <span>rem</span>
          <span class="brand-dot" aria-hidden="true"></span>
        </a>
        <div class="nav-links">
          <a href="https://shettyh.github.io/rem/">Website</a>
          <a href="https://github.com/shettyh/rem/tree/main/sync-server">Self-host</a>
          <a href="https://github.com/shettyh/rem">GitHub</a>
        </div>
      </nav>
    </header>

    <main>
      <section class="hero shell">
        <div>
          <p class="eyebrow"><span></span> rem Cloud · Early access</p>
          <h1>Sync everywhere.<br /><em>Keep owning your data.</em></h1>
          <p class="lede">
            rem Cloud is an optional hosted sync service for your decks, FSRS review history, and
            media across desktop, CLI, and the upcoming iPhone companion. Local storage and
            self-hosting remain free and open source.
          </p>
          <ul class="highlights">
            <li><span>✓</span> Local-first SQLite with automatic conflict-free merges</li>
            <li><span>✓</span> Isolated per-user Cloudflare Durable Object storage + R2 media</li>
            <li><span>✓</span> Sign out or delete cloud data anytime without touching local decks</li>
          </ul>
        </div>

        <div class="card">
          <p class="card-kicker">Early access waitlist</p>
          <h2>Request hosted sync</h2>
          <p class="card-desc">
            Hosted sync is rolling out in small batches while we build the iPhone companion.
            Enter the email you use with Google or Apple sign-in in rem.
          </p>

          <form class="form" id="waitlist-form" novalidate>
            <div class="field">
              <label for="waitlist-email">Email address</label>
              <input
                id="waitlist-email"
                name="email"
                type="email"
                required
                autocomplete="email"
                placeholder="you@example.com"
              />
            </div>

            <div class="field">
              <label for="waitlist-note">Devices or workflow <span>(optional)</span></label>
              <input
                id="waitlist-note"
                name="note"
                type="text"
                maxlength="280"
                placeholder="e.g. macOS + Linux CLI, interested in iPhone"
              />
            </div>

            <div class="hp" aria-hidden="true">
              <label for="waitlist-website">Website</label>
              <input
                id="waitlist-website"
                name="website"
                type="text"
                tabindex="-1"
                autocomplete="off"
              />
            </div>

            <button class="btn" type="submit" id="waitlist-submit">
              Join early access waitlist
            </button>

            <p class="feedback" id="waitlist-feedback" role="status" aria-live="polite"></p>
          </form>

          <p class="footnote">
            Once your email is approved, open <strong>Settings → Cloud sync</strong> in rem and
            click <strong>Retry sync</strong> (or sign in) to start syncing immediately.
          </p>
        </div>
      </section>

      <section class="modes">
        <div class="shell">
          <div class="modes-head">
            <p class="eyebrow">No lock-in</p>
            <h2>Three ways to <em>run rem.</em></h2>
            <p>
              Every copy of rem ships with the same local engine and configurable sync server URL.
              No features are locked behind a subscription.
            </p>
          </div>

          <div class="compare">
            <article>
              <p class="tier-label">Default · Free</p>
              <h3>Local only</h3>
              <p>
                No account required. Your decks, reviews, drafts, and media stay in local SQLite
                with portable JSON export and import.
              </p>
              <a class="tier-link" href="https://github.com/shettyh/rem/releases/latest">
                Download rem →
              </a>
            </article>

            <article class="is-featured">
              <p class="tier-label">Optional · Early access</p>
              <h3>rem Cloud</h3>
              <p>
                Zero-setup hosted sync run by the rem maintainer. Covers desktop and CLI today,
                with the iPhone companion coming next.
              </p>
              <a class="tier-link" href="#waitlist-form">Request early access ↑</a>
            </article>

            <article>
              <p class="tier-label">Open source · Apache-2.0</p>
              <h3>Self-hosted</h3>
              <p>
                Deploy the exact same Cloudflare Workers + Durable Objects + R2 sync server to your
                own account and enter its URL in Settings.
              </p>
              <a
                class="tier-link"
                href="https://github.com/shettyh/rem/tree/main/sync-server#byo-cloudflare-deployment"
              >
                Self-hosting guide →
              </a>
            </article>
          </div>
        </div>
      </section>
    </main>

    <footer>
      <div class="shell footer-inner">
        <span>rem — local-first spaced repetition for durable memory.</span>
        <div class="footer-links">
          <a href="https://shettyh.github.io/rem/">Home</a>
          <a href="https://github.com/shettyh/rem/blob/main/docs/cli.md">CLI docs</a>
          <a href="https://github.com/shettyh/rem/tree/main/sync-server">Self-host sync</a>
          <a href="https://github.com/shettyh/rem/blob/main/LICENSE">Apache-2.0</a>
        </div>
      </div>
    </footer>

    <script>
      ;(() => {
        const form = document.getElementById('waitlist-form')
        const emailInput = document.getElementById('waitlist-email')
        const noteInput = document.getElementById('waitlist-note')
        const websiteInput = document.getElementById('waitlist-website')
        const submitBtn = document.getElementById('waitlist-submit')
        const feedback = document.getElementById('waitlist-feedback')

        function setFeedback(text, kind) {
          feedback.textContent = text
          feedback.className = kind ? 'feedback is-' + kind : 'feedback'
        }

        form.addEventListener('submit', async (event) => {
          event.preventDefault()
          const email = emailInput.value.trim()
          if (!email || !emailInput.checkValidity()) {
            setFeedback('Please enter a valid email address.', 'error')
            emailInput.focus()
            return
          }

          submitBtn.disabled = true
          submitBtn.textContent = 'Submitting…'
          setFeedback('', '')

          try {
            const response = await fetch('/v1/waitlist', {
              method: 'POST',
              headers: { 'Content-Type': 'application/json' },
              body: JSON.stringify({
                email,
                note: noteInput.value.trim() || undefined,
                website: websiteInput.value,
              }),
            })
            const data = await response.json().catch(() => ({}))
            if (!response.ok) {
              throw new Error(data.message || 'Could not submit your request. Please try again.')
            }
            form.reset()
            setFeedback(
              'You’re on the early-access waitlist. Once approved, click Retry sync in rem Settings.',
              'success',
            )
          } catch (err) {
            setFeedback(
              err instanceof Error ? err.message : 'Network error. Please try again.',
              'error',
            )
          } finally {
            submitBtn.disabled = false
            submitBtn.textContent = 'Join early access waitlist'
          }
        })
      })()
    </script>
  </body>
</html>
`

export function renderCloudPage(): Response {
  return new Response(CLOUD_PAGE_HTML, {
    status: 200,
    headers: {
      'Content-Type': 'text/html; charset=utf-8',
      'Cache-Control': 'public, max-age=300',
      'X-Content-Type-Options': 'nosniff',
    },
  })
}
