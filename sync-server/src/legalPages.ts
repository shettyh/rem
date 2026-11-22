function renderLegalDocument(title: string, subtitle: string, bodyHtml: string): Response {
  const html = `<!doctype html>
<html lang="en">
  <head>
    <meta charset="UTF-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1.0" />
    <meta name="theme-color" content="#f8f7f4" />
    <title>${title} — rem</title>
    <style>
      :root {
        --bg: #f8f7f4;
        --surface: #ffffff;
        --ink: #1c1917;
        --muted: #6f6965;
        --line: #dedbd5;
        --purple: #5a47e8;
        --sans: -apple-system, BlinkMacSystemFont, 'Segoe UI', Helvetica, Arial, sans-serif;
      }
      * { box-sizing: border-box; }
      body {
        margin: 0;
        background: var(--bg);
        color: var(--ink);
        font-family: var(--sans);
        font-size: 15.5px;
        line-height: 1.65;
        -webkit-font-smoothing: antialiased;
      }
      .shell {
        max-width: 720px;
        margin: 0 auto;
        padding: 36px 24px 72px;
      }
      .brand {
        display: inline-flex;
        align-items: center;
        gap: 8px;
        font-size: 19px;
        font-weight: 700;
        letter-spacing: -0.04em;
        color: var(--ink);
        text-decoration: none;
        margin-bottom: 24px;
      }
      .brand-dot {
        width: 6px;
        height: 6px;
        border-radius: 999px;
        background: var(--purple);
      }
      .card {
        background: var(--surface);
        border: 1px solid var(--line);
        border-radius: 14px;
        padding: 28px 30px;
      }
      h1 {
        margin: 0 0 6px;
        font-size: 26px;
        letter-spacing: -0.03em;
      }
      .subtitle {
        margin: 0 0 22px;
        color: var(--muted);
        font-size: 13.5px;
      }
      h2 {
        margin: 24px 0 8px;
        font-size: 17px;
        letter-spacing: -0.01em;
      }
      p, li {
        color: var(--ink);
        margin: 0 0 12px;
      }
      ul {
        padding-left: 20px;
        margin: 0 0 14px;
      }
      a {
        color: var(--purple);
      }
      footer {
        margin-top: 24px;
        font-size: 13px;
        color: var(--muted);
        display: flex;
        gap: 16px;
      }
    </style>
  </head>
  <body>
    <main class="shell">
      <div class="brand">rem<span class="brand-dot"></span></div>
      <article class="card">
        <h1>${title}</h1>
        <p class="subtitle">${subtitle}</p>
        ${bodyHtml}
      </article>
      <footer>
        <a href="/privacy">Privacy Policy</a>
        <a href="/terms">Terms of Use</a>
      </footer>
    </main>
  </body>
</html>`
  return new Response(html, {
    status: 200,
    headers: { 'Content-Type': 'text/html; charset=utf-8' },
  })
}

export function renderPrivacyPage(): Response {
  return renderLegalDocument(
    'Privacy Policy',
    'Last updated: November 22, 2026',
    `
      <h2>Local-First by Default</h2>
      <p>rem stores your decks, flashcards, review history, and images locally on your device in SQLite. You do not need an account to use rem locally.</p>

      <h2>What We Store When You Enable Cloud Sync</h2>
      <ul>
        <li><strong>Account identity:</strong> Your email address and sign-in provider identifier (Apple or Google) so you can sign in across devices.</li>
        <li><strong>Synced study data:</strong> Your decks, cards, review logs, and attached images are stored in an isolated per-user Cloudflare Durable Object and private R2 storage bucket solely to synchronize your devices.</li>
        <li><strong>Billing &amp; entitlement state:</strong> Subscription status, plan identifier, renewal period end date, and provider transaction identifiers needed to verify your subscription. We never store your payment card numbers.</li>
      </ul>

      <h2>No Tracking or Selling Data</h2>
      <p>rem does not include third-party advertising SDKs, cross-app tracking, or data brokers. We never sell or share your study content or personal information.</p>

      <h2>Account Deletion &amp; Data Export</h2>
      <p>You can export a complete JSON backup of your collection at any time from Settings. You can permanently delete your cloud account and all synced cloud data and images directly inside Settings &rarr; Delete cloud account on Desktop or iOS.</p>
    `,
  )
}

export function renderTermsPage(): Response {
  return renderLegalDocument(
    'Terms of Use',
    'Last updated: November 22, 2026',
    `
      <h2>Open-Source Software &amp; Hosted Cloud Sync</h2>
      <p>The rem application and sync server source code are available under the Apache-2.0 License. Local storage and self-hosted sync remain free forever. <strong>rem Premium</strong> is an optional paid subscription that provides managed Cloud Sync across your devices.</p>

      <h2>Auto-Renewable Subscriptions</h2>
      <ul>
        <li><strong>rem Premium Annual:</strong> Optional annual auto-renewable subscription providing managed Cloud Sync across your devices at the price displayed in the app prior to purchase.</li>
        <li><strong>Auto-renewal:</strong> Payment is charged to your Apple Account at confirmation of purchase. Subscriptions automatically renew annually unless canceled at least 24 hours before the end of the current billing period.</li>
        <li><strong>Managing or canceling:</strong> App Store subscriptions can be managed or canceled anytime in iOS Settings &rarr; Apple Account &rarr; Subscriptions.</li>
        <li><strong>Lapse &amp; local data:</strong> If a trial or subscription ends, cloud sync pauses cleanly while all cards and review history stored on your devices remain yours and fully usable offline.</li>
      </ul>

      <h2>Apple Standard EULA</h2>
      <p>For subscriptions purchased through the Apple App Store, Apple&rsquo;s <a href="https://www.apple.com/legal/internet-services/itunes/dev/stdeula/" rel="noopener noreferrer">Standard Licensed Application End User License Agreement (EULA)</a> also applies.</p>

      <h2>Acceptable Use</h2>
      <p>Managed Cloud Sync is intended for personal spaced-repetition study collections within the documented 500 MB per-account media storage quota. Abuse or automated misuse of the hosted sync service may result in suspension.</p>
    `,
  )
}
