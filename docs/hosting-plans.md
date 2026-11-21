# Hosting plans

This page is the plan and price record for the paid hosted product. It states what is free and what is paid, the plans and their prices, the limits, how a plan changes, refunds, and the cancel and non-payment timeline. The hosting shape itself (one `tandem serve` per workspace behind a control plane) is recorded in [ADR 0048](adr/0048-hosted-tandem-is-one-serve-per-workspace.md) and is not restated here.

## Free and paid

- Self-hosting is the free tier: the same `tandem serve`, the same features, and no account.
- The paid product is a hosted workspace. There is no trial and no free hosted tier.
- Both plans have the same features as self-hosted. The plans differ only in the number of humans and the storage ceiling.
- One subscription is one workspace. A second workspace is a second subscription.

## Plans and prices

| | Solo | Team |
| --- | --- | --- |
| Humans | 1 | 2 or more |
| Unit | flat, per workspace | per human seat, minimum 2 seats billed |
| Monthly | EUR 2 | EUR 4 per seat (EUR 8 floor) |
| Yearly | EUR 12 | not offered at launch |
| Agent tokens | free, not counted | free, not counted |
| Storage ceiling | 1 GB | 10 GB |

- Solo may be described as "from EUR 1 a month" only together with the words that this is the yearly price, EUR 12 billed once a year.
- A seat is a human member of the workspace. Agents and their tokens are never seats. How a pending invite counts is set by HOS-15.
- The EU and US regions cost the same. The region is chosen at creation (see ADR 0048).
- Prices are in euro only, US-region workspaces included.
- VAT treatment is not settled on this page. VAT is set by HOS-11 and HOS-6, and this line is to be replaced when they settle it.

## Limits

- The storage ceilings in the table are per workspace, store and notes together. A ceiling is raised on request. There is no self-serve add-on at launch.
- Request rates, note size, the warning before a ceiling and the refusal at it are set by HOS-9.

## Changing plan

- Solo to team is an upgrade in place, with the unused solo payment credited. Team to solo is allowed only when one human member remains. The mechanics are set by HOS-15 and HOS-6.
- Seat changes mid-period are prorated by the payment provider's default (HOS-6).

## Refunds

- A refund on request within 14 days of the first payment of a subscription.
- No refund on renewals, and no pro-rata refund on cancel.

## Cancel and non-payment

The timeline is the same on both plans.

1. Cancel: the workspace runs to the end of the paid period, then enters step 3.
2. Failed payment: 7 days past due with full service while the charge retries.
3. Suspended for 30 days: the instance is stopped, the data is kept, the export archive (CUT-35) is downloadable from the account, and paying resumes the workspace as it was.
4. Deleted: the volume is deleted, and backups are purged within a further 30 days. Warning emails go out 7 days and 1 day before deletion. Deletion cannot be undone.

## Support

- Solo is self-service: documentation and the community channel, with no promised response time.
- No response time is promised on team either at launch. The support channel is set by HOS-10.

## Not offered

- No enterprise plan: no SSO, no audit log, no contracts.
- No coupons, launch discounts, education or open-source pricing.

## Owners

Each item below is owned by the named ticket. This page points at it and does not design it.

- Billing and the payment provider: HOS-6.
- VAT wording, the consumer withdrawal right and price-change notice: HOS-11.
- Quotas and abuse limits: HOS-9.
- Seats and roles: HOS-15.
- Provisioning, regions, and how an export is produced while the instance is stopped: HOS-17, with CUT-35.
- The pricing section of the site: HOS-12.
