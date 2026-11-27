# Single sign-on through an outside identity provider, and the owner link from the host

> **Status:** In force. See [the index](README.md).

[ADR 0049](0049-a-browser-signs-in-to-a-token-guarded-serve.md) signs a browser in with a token or a sign-in key typed into the login form, and [ADR 0052](0052-members-and-personal-tokens.md) makes members come only through the People screen or the operator. A self-hoster who wants people to sign in with the accounts they already have, and the hosted product, which signs an account in on its control plane and hands it over, both need the instance to trust an outside identity provider. This record decides how far that trust goes, what identity a member has, who may be let in by sign-in, and how the owner gets back in when no admin can. It amends ADR 0049 and ADR 0052 by quotation; the parts of those records that are not named here stand.

Decided 2026-11-26 (TAN-903, grill rounds 1 and 2, with the human).

## Decision

- **The instance is a relying party only.** `tandem serve` trusts an outside identity provider: any OpenID Connect issuer, with presets for Google, GitHub and Apple. The instance never becomes an authorization server (the direction set on TAN-898). This is self-hosted only. On the hosted product the control plane signs the account in and hands it over through TAN-498, so the instance receives an assertion it does not verify itself.
- **Outbound network only when configured.** The serve calls the provider (discovery, keys, token exchange, and GitHub's API) only when a provider is configured. With no provider the serve makes no outbound call, and the README's "no account and no network" stays true by default.
- **Identity is the (issuer, subject) pair.** A member's identity is the pair the provider reports for that account, one pair per provider. A member gains an email address. The first sign-in through any configured provider whose verified email equals the member's address links that provider's pair to the member. Later sign-ins match on the pair alone, so a changed email at the provider does not move the member.
- **Who gets in.** Two doors let a person in through a provider:
  - a person the admin added by email on the People screen, as ADR 0052 part 5 already allows;
  - through Google and generic OIDC only, a verified email whose domain is in `TANDEM_SSO_ALLOW_DOMAINS`, or a token carrying the claim named by `TANDEM_SSO_ALLOW_CLAIM`. Either creates a `member` at first sign-in.
  Creation by sign-in is possible only once an admin exists, so there is no first-visitor path, and ADR 0052 part 5's refusal of one stands. Roles stay on the People screen, and there is no mapping from provider groups to roles. Public mail domains are refused by the domain rule only: a single personal address added on the People screen always works.
- **A configured provider means SSO only.** With a provider configured, the login page shows one button per provider and no password-style form, `POST /login` answers `404`, and the People screen issues no sign-in keys. Personal `tdm_` tokens are unchanged, since they never reach the login page. With no provider configured, ADR 0049 and ADR 0052 apply as they stand.
- **The SSO session has a ceiling.** A session opened through a provider ends at most `TANDEM_SSO_SESSION_DAYS` after sign-in (default 7), in addition to ADR 0049's idle rule. No refresh token is stored, and nothing calls the provider in the background. Removing a member on the People screen still ends that member's sessions at once.
- **The owner link.** `tandem-cli owner-link`, run against the home on the host, makes a single-use code and prints `<public origin>/login/owner#<code>`. The code is valid for 10 minutes, is stored only as a digest, and is spent by its first use. The login page reads the code from the fragment and posts it, which opens an owner session. The owner link is how the owner bootstraps the first admin and recovers when no admin can sign in. It differs from the one-time URL that ADR 0049 rejected: it is made on demand, not at startup; it is single use and lasts 10 minutes; the code sits in the fragment, so it reaches no server, proxy or access log; and only shell access to the home can make one, which already holds the whole workspace through `export`.
- **SAML is out.** SCIM provisioning is a later ticket.

## Credential handling

The builds that follow this record hold to these rules:

- No secret is logged or printed: not a provider's client secret, an authorization code, a token exchanged at the provider, a session id, a cookie, the owner-link code or its digest.
- The owner link is stored as a digest only, looked up by that digest, and spent on first use.
- The owner link travels in the URL fragment and never in a query string, so it is not sent to a server and does not appear in a proxy log.

## Supersedes

By quotation. The text quoted is checked against the files as they stand.

Of [ADR 0049](0049-a-browser-signs-in-to-a-token-guarded-serve.md):

- "A one-time login URL printed at startup" is rejected in 0049. Narrowed here: an owner link made on demand, single use, valid 10 minutes, with the code in the fragment, is the bootstrap and recovery door, for the reasons in the Decision above.
- "`/login` and `GET /static/*` are exempt" from the token by name. The SSO routes and the owner-link route join that list.

Of [ADR 0052](0052-members-and-personal-tokens.md):

- "A sign-in key (`tdk_`) is accepted only at `POST /login`" is now: accepted only at `POST /login`, and only with no provider configured.
- Part 5's "`/people` answers a request that holds `manage-people` when a sign-in key can be used on that run" is now: when a sign-in key can be used on that run, which also includes a run with a provider configured.
- Part 5's "There is no first-visitor path, no variable that mints an admin on start" is kept. The owner link is named as the bootstrap door.
- The rule that members come only through the People screen or the operator is now also met through the domain or claim rule above, for a sign-in through a provider.

Status lines on ADR 0049 and ADR 0052 point to this record.

## Why

- **The instance stays a relying party.** An authorization server in the core would make the product run a login service for its own sake. Trusting a provider keeps the credential with the people who already hold it.
- **Outbound network only when asked for.** A default install that calls nothing keeps the self-hoster's promise and keeps the hosted fleet's network surface unchanged for a serve with no provider.
- **The pair, not the email, is identity.** An email can change or be reused at a provider. Matching on the pair alone keeps a member bound to the account that first proved them.
- **Creation by sign-in only after an admin exists.** The first admin is always chosen by an owner, so no stranger can become the first member of a new workspace.
- **A ceiling on the SSO session.** Without a refresh token, the instance cannot follow a provider's revocation, so a fixed ceiling bounds how long a session outlives a sign-out at the provider.
- **The owner link over a password.** A short-lived, single-use code printed on the host gives a recovery door without a standing secret the owner must keep, and the fragment keeps it out of logs.

## Rejected

- An OAuth authorization server in the core: the human's direction on TAN-898.
- A first-visitor admin, a CLI verb that adds an admin, and an unlinked owner-token page: grill round 2 (human, 2026-11-26).
- Logo-only provider buttons, and the provider's own label on a generic button: the human chose labelled buttons and "SSO".
- Mapping provider groups to roles: roles stay on the People screen.
- A refresh token stored for the SSO session.
