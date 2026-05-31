# 15 — Monetization Strategy

The goal is a venture-scale, durable business with **high gross margin** (local-first compute, BYO-key AI) and a **clear path from individual developers to enterprise**. The model mirrors what works for JetBrains and is being validated by Cursor: a great free/cheap tier that earns trust, a paid pro tier developers happily expense, and an enterprise tier with the controls orgs require.

## 1. Positioning & willingness to pay

PhpStorm costs roughly **$99/year** for individuals (first-year; cheaper after) and more per-seat for orgs — and Laravel devs already pay it, often **plus** Laravel Idea (~$60/year) on top. That stacked ~$150+/yr for "PhpStorm + Laravel Idea" is the price umbrella Photon sits under: deliver that combined experience, faster and natively, at a comparable or better price. Developers pay for tools that save daily friction; the bar is "does this make me meaningfully more productive," and the performance + Laravel-native combination is a yes.

> Pricing figures are indicative market context, not quotes; finalize against current competitor pricing at launch.

## 2. Packaging tiers

### Free — "Community"
- Full editor, PHP intelligence, core navigation, Search Everywhere, git basics, terminal.
- A meaningful slice of Laravel intelligence (e.g. routes + Eloquent navigation) so the magic is visible.
- AI with **your own key** (we add no markup).
- **Purpose:** mass adoption, word-of-mouth, the funnel. Generous enough to be a real daily driver for hobbyists/students/OSS.

### Pro — individual (subscription, ~$8–12/mo or ~$90–120/yr)
- **Full Laravel intelligence** (container, events, queues, Blade depth, factories, i18n detection, runtime reflection).
- **Full refactoring engine**, database tools, debugger, advanced git.
- AI agent mode (still BYO-key, or bundled credits — see §4).
- Premium plugins/themes; priority updates.
- **This is the core revenue engine** — the "PhpStorm + Laravel Idea replacement" most individual buyers land on.

### Team (per-seat, ~$15–25/seat/mo)
- Everything in Pro, plus: shared settings/keymaps, private plugin registry, centralized license/seat management, team onboarding/codebase Q&A, SSO (basic).
- Volume-friendly billing for agencies and product teams (a huge share of Laravel work is agencies).

### Enterprise (custom, annual)
- SSO/SAML/SCIM, policy controls (restrict/disable AI providers, force local-only, telemetry off), audit logs, fleet management, air-gapped/on-prem update + private marketplace, security review artifacts, SLA support.
- Procurement-friendly: invoicing, security questionnaires, DPA.

## 3. Marketplace revenue

A signed plugin marketplace ([07](./07-plugin-sdk.md)) creates a second, compounding revenue line and deepens the moat:
- **Revenue share** on paid plugins/themes (e.g. 15–30% platform fee).
- First-party premium packs (advanced Livewire/Filament/Inertia/testing tooling) sold directly.
- Enterprise private marketplaces (part of the Enterprise tier).
- The marketplace also lowers our own roadmap burden — the community fills long-tail needs (R6 mitigation in [14](./14-risk-analysis.md)).

## 4. AI monetization (carefully)

Default is **BYO-key** — zero marginal cost to us, maximum trust, and a strong privacy story. On top of that, an **optional managed-AI add-on**:
- Bundled credits / managed keys for users who don't want to manage providers, priced with margin (cheap models for completion, strong models metered for agent/chat).
- Cost controlled via model routing, token budgets, caching, and per-tier quotas ([13](./13-scaling-strategy.md)).
- Enterprise can bring their own model endpoints (Azure OpenAI, Bedrock, self-hosted) under policy.
- AI is **additive revenue, never a cost trap** — the IDE is fully valuable without our AI, so we're insulated from AI-cost volatility.

## 5. Go-to-market

1. **Beachhead: the Laravel community.** It's large, passionate, concentrated (Laracon, Laravel News, large Twitter/Discord/Reddit presence, influential creators), and already pays for tooling. Win it decisively before broadening to general PHP, then to the JS/TS-adjacent stack.
2. **Performance as the hook.** "Open your Laravel app in under 2 seconds, under 500MB" is a demoable, shareable wow — benchmark videos against PhpStorm spread on their own.
3. **Free tier + OSS goodwill.** Sponsor/integrate with Laravel OSS; make Photon the obvious free pick for students and indie devs, then convert to Pro as they go professional.
4. **Creator & conference motion.** Laravel educators and conference sponsorship; "switch from PhpStorm" content.
5. **Land-and-expand for teams/agencies.** Individual Pro adoption inside agencies pulls in Team/Enterprise seats.
6. **Migration ease.** PhpStorm and VS Code keymap presets + settings import lower the switching cost to near zero.

## 6. Unit economics (why the margin is strong)

- **Compute is the user's** (local-first) and **AI is BYO-key by default** — marginal cost per user ≈ CDN bandwidth + a sliver of light backend (license, marketplace, opt-in telemetry).
- High gross margin (software-with-thin-backend economics, JetBrains-like), so revenue scales far ahead of infra cost ([13](./13-scaling-strategy.md) §3).
- Subscription (recurring) + marketplace take (compounding) + enterprise (high ACV, sticky) = three reinforcing streams.

## 7. Indicative model & milestones

Illustrative, not a forecast:
- A community of even a few hundred thousand free users with a **single-digit %** conversion to ~$100/yr Pro is an eight-figure ARR business; team/enterprise seats and marketplace take push it higher.
- The Laravel ecosystem is large enough (millions of developers touch Laravel/PHP) that a niche-dominant IDE is venture-scale on the niche alone — and the architecture ([02](./02-module-design.md)) generalizes to broader PHP and the JS/TS stack as a second act.

| Milestone | Signal |
|---|---|
| Product-market fit | Daily-active Laravel devs who've uninstalled PhpStorm; organic NPS/word-of-mouth |
| Monetization fit | Free→Pro conversion rate stabilizing at target; low churn |
| Expansion | Team/Enterprise seat growth; marketplace GMV |
| Platform | Third-party plugin ecosystem self-sustaining |

## 8. Pricing principles
- **Never paywall correctness or performance** — those are the brand and must be in the free tier. Paywall *breadth and power* (full Laravel depth, DB tools, debugger, agent, team/enterprise controls).
- **Be cheaper-or-equal to PhpStorm+Laravel Idea combined**, while being faster — value is obvious.
- **Respect developers:** no ads, no dark patterns, transparent AI data handling, generous free tier. Trust is the acquisition engine.

## 9. Strategic optionality
The same Rust core enables later expansion — remote/cloud dev, a browser surface, broader-language IDEs — each a potential new revenue surface without re-architecting. The monetization model (subscription + marketplace + enterprise) carries across all of them.

---

*End of specification. Return to the [index](./README.md).*
