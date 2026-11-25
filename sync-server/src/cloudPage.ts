import { getPriceDisplay, getTrialDays } from './entitlements'
import type { Env } from './types'

function escapeHtml(raw: string): string {
  return raw
    .replaceAll('&', '&amp;')
    .replaceAll('<', '&lt;')
    .replaceAll('>', '&gt;')
    .replaceAll('"', '&quot;')
    .replaceAll("'", '&#39;')
}

function buildCloudPageHtml(trialDays: number, priceDisplay: string): string {
  const safePrice = escapeHtml(priceDisplay)
  return `<!doctype html>
<html lang="en">
  <head>
    <meta charset="UTF-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1.0" />
    <meta
      name="description"
      content="rem Premium — optional managed Cloud Sync for rem across desktop, CLI, and iPhone. Start a ${trialDays}-day free trial with no card required, or keep local storage and open-source self-hosting free forever."
    />
    <meta name="theme-color" content="#f8f7f4" />
    <title>rem Premium — Cloud Sync across all your devices</title>
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
        --green-bg: rgba(32, 135, 90, 0.1);
        --amber: #b45309;
        --amber-bg: rgba(180, 83, 9, 0.1);
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
        font-size: 32px;
      }

      .card-desc {
        margin: 12px 0 0;
        color: var(--muted);
        font-size: 14px;
        line-height: 1.65;
      }

      .stack {
        display: grid;
        gap: 12px;
        margin-top: 22px;
      }

      .account-bar {
        display: flex;
        align-items: center;
        justify-content: space-between;
        gap: 12px;
        padding: 12px 14px;
        border: 1px solid var(--line-soft);
        border-radius: 10px;
        background: var(--bg);
      }

      .account-email {
        font-size: 13px;
        font-weight: 600;
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
      }

      .badge {
        display: inline-flex;
        align-items: center;
        padding: 3px 9px;
        border-radius: 999px;
        font-size: 11px;
        font-weight: 600;
        white-space: nowrap;
      }

      .badge.is-ok {
        color: var(--green);
        background: var(--green-bg);
      }

      .badge.is-warn {
        color: var(--amber);
        background: var(--amber-bg);
      }

      .badge.is-neutral {
        color: var(--muted);
        background: var(--surface-alt);
      }

      .field {
        display: grid;
        gap: 6px;
      }

      .field label {
        font-size: 13px;
        font-weight: 600;
      }

      .inline-form {
        display: flex;
        gap: 8px;
      }

      .field input {
        width: 100%;
        min-height: 44px;
        padding: 0 14px;
        border: 1px solid var(--line);
        border-radius: 8px;
        background: var(--bg);
        color: var(--ink);
        font-family: var(--mono);
        font-size: 14px;
        letter-spacing: 0.04em;
        text-transform: uppercase;
      }

      .field input:focus {
        outline: none;
        border-color: var(--purple);
        background: var(--surface);
        box-shadow: 0 0 0 3px rgba(90, 71, 232, 0.14);
      }

      .btn {
        display: inline-flex;
        align-items: center;
        justify-content: center;
        width: 100%;
        min-height: 44px;
        padding: 0 18px;
        border: 0;
        border-radius: 8px;
        background: var(--purple);
        color: #fff;
        font: inherit;
        font-size: 14px;
        font-weight: 600;
        cursor: pointer;
        box-shadow: 0 8px 20px rgba(90, 71, 232, 0.2);
      }

      .btn:hover {
        background: var(--purple-dark);
      }

      .btn:disabled {
        opacity: 0.65;
        cursor: wait;
      }

      .btn-secondary {
        background: var(--surface);
        color: var(--ink);
        border: 1px solid var(--line);
        box-shadow: none;
      }

      .btn-secondary:hover {
        background: var(--bg);
      }

      .btn-compact {
        width: auto;
        white-space: nowrap;
      }

      .btn-link {
        background: transparent;
        color: var(--muted);
        border: 0;
        box-shadow: none;
        min-height: 32px;
        width: auto;
        padding: 0 4px;
        font-size: 13px;
        font-weight: 500;
        justify-self: start;
      }

      .btn-link:hover {
        background: transparent;
        color: var(--ink);
        text-decoration: underline;
      }

      .divider {
        height: 1px;
        background: var(--line-soft);
        margin: 6px 0;
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

      .hidden {
        display: none !important;
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
          <p class="eyebrow"><span></span> rem Premium · ${trialDays}-day free trial</p>
          <h1>Sync everywhere.<br /><em>Keep owning your data.</em></h1>
          <p class="lede">
            All local features and self-hosting stay free forever. rem Premium (${safePrice})
            unlocks managed Cloud Sync across macOS, Linux, Windows, CLI, and iPhone—and supports
            development.
          </p>
          <ul class="highlights">
            <li><span>✓</span> Optional ${trialDays}-day free trial with no card required</li>
            <li><span>✓</span> Local-first SQLite with automatic conflict-free merges</li>
            <li><span>✓</span> Isolated per-user Cloudflare Durable Objects + 500 MB R2 media</li>
            <li><span>✓</span> Sign out or delete cloud data anytime without touching local decks</li>
          </ul>
        </div>

        <div class="card" id="account-card">
          <p class="card-kicker">Account &amp; billing</p>
          <h2 id="card-title">Manage rem Premium</h2>
          <p class="card-desc" id="card-desc">
            Sign in to start your ${trialDays}-day free trial, subscribe, or check your rem Premium
            status. Changes unlock Desktop, CLI, and iPhone automatically.
          </p>

          <div id="signed-out-panel" class="stack">
            <button type="button" class="btn hidden" id="btn-google">Continue with Google</button>
            <button type="button" class="btn btn-secondary hidden" id="btn-apple">
              Continue with Apple
            </button>
            <p class="card-desc hidden" id="no-providers-msg">
              Sign-in providers are not configured on this server.
            </p>
          </div>

          <div id="signed-in-panel" class="stack hidden">
            <div class="account-bar">
              <span class="account-email" id="account-email"></span>
              <span class="badge is-neutral" id="status-badge">Loading…</span>
            </div>

            <p class="card-desc" id="status-detail" style="margin-top: 0"></p>

            <button type="button" class="btn hidden" id="btn-start-trial">
              Start ${trialDays}-day free trial
            </button>

            <button type="button" class="btn btn-secondary hidden" id="btn-subscribe-polar">
              Upgrade to Premium — ${safePrice}
            </button>

            <button type="button" class="btn btn-secondary hidden" id="btn-manage-billing">
              Manage billing ↗
            </button>

            <div class="divider"></div>
            <button type="button" class="btn btn-link" id="btn-sign-out">Sign out</button>
          </div>

          <p class="feedback" id="account-feedback" role="status" aria-live="polite"></p>

          <p class="footnote">
            After starting a trial or subscribing here, return to <strong>rem</strong> on Desktop
            or iPhone — sync activates automatically (or choose <strong>Check access</strong> in
            Settings).
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
              No study features are locked behind a subscription.
            </p>
          </div>

          <div class="compare">
            <article>
              <p class="tier-label">Default · Free forever</p>
              <h3>Local only</h3>
              <p>
                No account required. Your decks, FSRS reviews, drafts, and media stay in local
                SQLite with portable JSON export and import.
              </p>
              <a class="tier-link" href="https://github.com/shettyh/rem/releases/latest">
                Download rem →
              </a>
            </article>

            <article class="is-featured">
              <p class="tier-label">Optional · ${trialDays}-day free trial, then ${safePrice}</p>
              <h3>rem Premium</h3>
              <p>
                Zero-setup managed Cloud Sync across Desktop, CLI, and iPhone. Start a ${trialDays}-day
                free trial with no card required, then ${safePrice} to keep syncing and support rem.
              </p>
              <a class="tier-link" href="#account-card">Start free trial ↑</a>
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
        const SESSION_KEY = 'rem_web_session_v1'
        const PKCE_KEY = 'rem_web_pkce_v1'

        const signedOutPanel = document.getElementById('signed-out-panel')
        const signedInPanel = document.getElementById('signed-in-panel')
        const btnGoogle = document.getElementById('btn-google')
        const btnApple = document.getElementById('btn-apple')
        const noProvidersMsg = document.getElementById('no-providers-msg')
        const accountEmail = document.getElementById('account-email')
        const statusBadge = document.getElementById('status-badge')
        const statusDetail = document.getElementById('status-detail')
        const btnStartTrial = document.getElementById('btn-start-trial')
        const btnSubscribePolar = document.getElementById('btn-subscribe-polar')
        const btnManageBilling = document.getElementById('btn-manage-billing')
        const btnSignOut = document.getElementById('btn-sign-out')
        const feedback = document.getElementById('account-feedback')

        function setFeedback(text, kind) {
          feedback.textContent = text || ''
          feedback.className = kind ? 'feedback is-' + kind : 'feedback'
        }

        function base64Url(bytes) {
          let bin = ''
          for (let i = 0; i < bytes.length; i++) bin += String.fromCharCode(bytes[i])
          return btoa(bin).replace(/\\+/g, '-').replace(/\\//g, '_').replace(/=+$/, '')
        }

        function randomBase64Url(len) {
          const buf = new Uint8Array(len)
          crypto.getRandomValues(buf)
          return base64Url(buf)
        }

        async function pkceChallenge(verifier) {
          const digest = await crypto.subtle.digest('SHA-256', new TextEncoder().encode(verifier))
          return base64Url(new Uint8Array(digest))
        }

        function getCanonicalRedirectUri() {
          const path = location.pathname.replace(/\\/+$/, '') || '/premium'
          return location.origin + (path === '/cloud' ? '/cloud' : '/premium')
        }

        function loadSession() {
          try {
            const raw = sessionStorage.getItem(SESSION_KEY)
            return raw ? JSON.parse(raw) : null
          } catch {
            return null
          }
        }

        function saveSession(session) {
          if (!session) {
            sessionStorage.removeItem(SESSION_KEY)
          } else {
            sessionStorage.setItem(SESSION_KEY, JSON.stringify(session))
          }
        }

        function daysLeft(expiresAt) {
          if (!expiresAt) return null
          return Math.max(1, Math.ceil((expiresAt - Date.now()) / 86400000))
        }

        async function authedFetch(path, init = {}) {
          let session = loadSession()
          if (!session || !session.accessToken) throw new Error('Please sign in first.')

          const doFetch = (token) =>
            fetch(path, {
              ...init,
              headers: {
                ...(init.headers || {}),
                Authorization: 'Bearer ' + token,
              },
            })

          let resp = await doFetch(session.accessToken)
          if (resp.status === 401 && session.refreshToken) {
            const refreshResp = await fetch('/v1/auth/refresh', {
              method: 'POST',
              headers: { 'Content-Type': 'application/json' },
              body: JSON.stringify({ refreshToken: session.refreshToken }),
            })
            if (refreshResp.ok) {
              session = await refreshResp.json()
              saveSession(session)
              resp = await doFetch(session.accessToken)
            } else {
              saveSession(null)
              renderSignedOut()
              throw new Error('Your session expired. Please sign in again.')
            }
          }
          return resp
        }

        function renderSignedOut() {
          signedInPanel.classList.add('hidden')
          signedOutPanel.classList.remove('hidden')
        }

        function renderAccount(account) {
          signedOutPanel.classList.add('hidden')
          signedInPanel.classList.remove('hidden')
          accountEmail.textContent = account.email || account.id

          const ent = account.entitlement || {}
          const state = ent.state || 'none'
          const dLeft = daysLeft(ent.expiresAt)

          btnStartTrial.classList.toggle('hidden', !ent.canStartTrial)
          btnSubscribePolar.classList.toggle('hidden', !ent.canSubscribePolar)
          btnSubscribePolar.textContent =
            'Upgrade to Premium — ' + (ent.priceDisplay || '${safePrice}')
          btnSubscribePolar.className =
            'btn' +
            (ent.canStartTrial ? ' btn-secondary' : '') +
            (ent.canSubscribePolar ? '' : ' hidden')
          btnManageBilling.classList.toggle('hidden', !ent.canManageBilling)
          btnManageBilling.className =
            'btn' +
            (state === 'past_due' ? '' : ' btn-secondary') +
            (ent.canManageBilling ? '' : ' hidden')

          if (state === 'self_hosted') {
            statusBadge.textContent = 'Self-hosted'
            statusBadge.className = 'badge is-ok'
            statusDetail.textContent = 'Sync is enabled on this self-hosted server.'
          } else if (state === 'trialing') {
            statusBadge.textContent = 'Trial · ' + (dLeft || 1) + 'd left'
            statusBadge.className = 'badge is-ok'
            statusDetail.textContent =
              'Your free trial is active across Desktop, CLI, and iPhone.'
          } else if (state === 'active') {
            statusBadge.textContent = ent.provider === 'allowlist' ? 'Active' : 'Premium active'
            statusBadge.className = 'badge is-ok'
            if (ent.provider === 'apple') {
              statusDetail.textContent =
                'Active via Apple App Store. Manage renewal in iOS Subscriptions.'
            } else if (ent.provider === 'allowlist') {
              statusDetail.textContent =
                'Cloud Sync is active for your account across Desktop, CLI, and iPhone.'
            } else {
              const dateLabel = ent.expiresAt
                ? new Date(ent.expiresAt).toLocaleDateString(undefined, {
                    month: 'short',
                    day: 'numeric',
                    year: 'numeric',
                  })
                : null
              const renewalNote = dateLabel
                ? (ent.cancelAtPeriodEnd ? ' Ends ' : ' Renews ') + dateLabel + '.'
                : ''
              statusDetail.textContent =
                'Your rem Premium subscription is active across all devices.' + renewalNote
            }
          } else if (state === 'past_due') {
            statusBadge.textContent = 'Payment issue'
            statusBadge.className = 'badge is-warn'
            statusDetail.textContent =
              'There was a renewal issue. Please update your payment method to keep syncing.'
          } else if (state === 'expired') {
            statusBadge.textContent = 'Sync paused'
            statusBadge.className = 'badge is-warn'
            statusDetail.textContent =
              'Your trial or plan ended. Your local cards remain safe on your devices.'
          } else {
            statusBadge.textContent = 'Not active yet'
            statusBadge.className = 'badge is-neutral'
            statusDetail.textContent =
              'Start your ' +
              (ent.trialDays || ${trialDays}) +
              '-day free trial (no card required) or upgrade to Premium below.'
          }
        }

        async function startOAuth(provider) {
          setFeedback('', '')
          const verifier = randomBase64Url(48)
          const challenge = await pkceChallenge(verifier)
          const state = randomBase64Url(16)
          const redirectUri = getCanonicalRedirectUri()
          sessionStorage.setItem(PKCE_KEY, JSON.stringify({ verifier, state, redirectUri }))

          const url = new URL('/v1/auth/' + provider + '/start', location.origin)
          url.searchParams.set('redirect_uri', redirectUri)
          url.searchParams.set('code_challenge', challenge)
          url.searchParams.set('code_challenge_method', 'S256')
          url.searchParams.set('state', state)
          location.assign(url.toString())
        }

        async function handleOAuthCallbackIfPresent() {
          const params = new URLSearchParams(location.search)
          const code = params.get('code')
          const state = params.get('state')
          const err = params.get('error')
          if (!code && !err) return false

          history.replaceState({}, '', location.pathname)

          if (err) {
            setFeedback('Sign-in was canceled or failed (' + err + ').', 'error')
            return false
          }

          let pkce = null
          try {
            pkce = JSON.parse(sessionStorage.getItem(PKCE_KEY) || 'null')
          } catch {
            pkce = null
          }
          sessionStorage.removeItem(PKCE_KEY)

          if (!pkce || !pkce.verifier || pkce.state !== state) {
            setFeedback('Sign-in session mismatch. Please try signing in again.', 'error')
            return false
          }

          const resp = await fetch('/v1/auth/exchange', {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({
              code,
              codeVerifier: pkce.verifier,
              redirectUri: pkce.redirectUri,
              deviceId: 'web-portal',
            }),
          })
          const data = await resp.json().catch(() => ({}))
          if (!resp.ok) {
            setFeedback(data.error || 'Could not complete sign-in.', 'error')
            return false
          }
          saveSession(data)
          return true
        }

        async function refreshAccount() {
          const resp = await authedFetch('/v1/account')
          if (!resp.ok) throw new Error('Could not load account status.')
          const account = await resp.json()
          renderAccount(account)
          return account
        }

        btnGoogle.addEventListener('click', () => startOAuth('google'))
        btnApple.addEventListener('click', () => startOAuth('apple'))

        btnSubscribePolar.addEventListener('click', async () => {
          btnSubscribePolar.disabled = true
          setFeedback('', '')
          try {
            const resp = await authedFetch('/v1/billing/polar/checkout', { method: 'POST' })
            const data = await resp.json().catch(() => ({}))
            if (!resp.ok || !data.checkoutUrl) {
              throw new Error(data.message || data.error || 'Could not start Polar checkout.')
            }
            location.assign(data.checkoutUrl)
          } catch (err) {
            setFeedback(err instanceof Error ? err.message : 'Failed to start checkout.', 'error')
            btnSubscribePolar.disabled = false
          }
        })

        btnManageBilling.addEventListener('click', async () => {
          btnManageBilling.disabled = true
          setFeedback('', '')
          try {
            const resp = await authedFetch('/v1/billing/polar/portal', { method: 'POST' })
            const data = await resp.json().catch(() => ({}))
            if (!resp.ok || !data.portalUrl) {
              throw new Error(data.message || data.error || 'Could not open billing portal.')
            }
            location.assign(data.portalUrl)
          } catch (err) {
            setFeedback(
              err instanceof Error ? err.message : 'Failed to open billing portal.',
              'error',
            )
            btnManageBilling.disabled = false
          }
        })

        btnStartTrial.addEventListener('click', async () => {
          btnStartTrial.disabled = true
          setFeedback('', '')
          try {
            const resp = await authedFetch('/v1/trial/start', { method: 'POST' })
            const data = await resp.json().catch(() => ({}))
            if (!resp.ok) {
              throw new Error(data.message || data.error || 'Could not start free trial.')
            }
            renderAccount(data)
            setFeedback(
              'Your free trial is now active. Return to rem on Desktop or iPhone to sync.',
              'success',
            )
          } catch (err) {
            setFeedback(err instanceof Error ? err.message : 'Failed to start trial.', 'error')
          } finally {
            btnStartTrial.disabled = false
          }
        })

        btnSignOut.addEventListener('click', async () => {
          const session = loadSession()
          saveSession(null)
          renderSignedOut()
          setFeedback('Signed out.', '')
          if (session && session.refreshToken) {
            fetch('/v1/auth/logout', {
              method: 'POST',
              headers: { 'Content-Type': 'application/json' },
              body: JSON.stringify({ refreshToken: session.refreshToken }),
            }).catch(() => {})
          }
        })

        ;(async () => {
          try {
            const providersResp = await fetch('/v1/auth/providers')
            if (providersResp.ok) {
              const providers = await providersResp.json()
              if (providers.google) btnGoogle.classList.remove('hidden')
              if (providers.apple) btnApple.classList.remove('hidden')
              if (!providers.google && !providers.apple) {
                noProvidersMsg.classList.remove('hidden')
              }
            }
          } catch {
            // Keep default state on transient network failure.
          }

          const initialParams = new URLSearchParams(location.search)
          const billingSuccess = initialParams.get('billing') === 'success'
          if (billingSuccess && !initialParams.get('code') && !initialParams.get('error')) {
            history.replaceState({}, '', location.pathname)
          }

          await handleOAuthCallbackIfPresent()
          if (loadSession()) {
            try {
              let account = await refreshAccount()
              if (billingSuccess) {
                if (account.entitlement && account.entitlement.state === 'active') {
                  setFeedback(
                    'rem Premium is active! Return to rem on Desktop or iPhone — sync activates automatically.',
                    'success',
                  )
                } else {
                  setFeedback(
                    'Payment received! Updating your rem Premium status…',
                    'success',
                  )
                  for (let attempt = 0; attempt < 4; attempt++) {
                    await new Promise((r) => setTimeout(r, 1500))
                    account = await refreshAccount()
                    if (account.entitlement && account.entitlement.state === 'active') {
                      setFeedback(
                        'rem Premium is active! Return to rem on Desktop or iPhone — sync activates automatically.',
                        'success',
                      )
                      break
                    }
                  }
                }
              }
            } catch (err) {
              setFeedback(err instanceof Error ? err.message : 'Could not load account.', 'error')
            }
          } else {
            renderSignedOut()
            if (billingSuccess) {
              setFeedback(
                'Thank you for subscribing to rem Premium! Return to the rem desktop app — Cloud Sync will activate automatically.',
                'success',
              )
            }
          }
        })()
      })()
    </script>
  </body>
</html>
`
}

export function renderCloudPage(env?: Env): Response {
  const trialDays = env ? getTrialDays(env) : 30
  const priceDisplay = env ? getPriceDisplay(env) : '$5/year'
  return new Response(buildCloudPageHtml(trialDays, priceDisplay), {
    status: 200,
    headers: {
      'Content-Type': 'text/html; charset=utf-8',
      'Cache-Control': 'public, max-age=300',
      'X-Content-Type-Options': 'nosniff',
    },
  })
}
