# Feature Specification: noviq.pk Domain Cutover

**Feature Branch**: `028-noviq-pk-cutover`  
**Created**: 2026-08-20  
**Status**: Draft  
**Input**: User description: "Cut Noviq over from the rotating trycloudflare quick tunnel to the permanent noviq.pk domain: named Cloudflare tunnel serving the app on the apex hostname, and Zoho Mail on the same zone for official correspondence."

## User Scenarios & Testing *(mandatory)*

### User Story 1 - A tester reaches Noviq at an address that does not change (Priority: P1)

A tester at the department is told to open Noviq. They type `noviq.pk` and reach the
sign-in page. Tomorrow, next week, and after any server restart, the same address
still works. Nobody has to ask "what is the current link?", and no link that was
shared last week has silently died.

**Why this priority**: This is the whole point of owning the domain. Today the public
address is a `trycloudflare` quick-tunnel URL that is regenerated on every restart of
the tunnel process, so every restart invalidates every link already handed out. For a
product being demonstrated to government users, an address that expires without notice
is worse than a slow one — it reads as an outage and it cannot be printed in any
document, application, or email signature.

**Independent Test**: Fully testable on its own. Open `https://noviq.pk`, sign in,
run a query and confirm a real answer comes back. Then restart the tunnel service and
repeat without changing the address. Delivers the stable public entry point even if
no mail is ever configured.

**Acceptance Scenarios**:

1. **Given** the cutover is complete, **When** a user opens `https://noviq.pk` in a browser, **Then** the Noviq sign-in page loads over a valid TLS certificate with no warning.
2. **Given** a signed-in user on `https://noviq.pk`, **When** they submit a corpus question, **Then** they receive an answer with citations, matching the behaviour available on the previous address.
3. **Given** the tunnel service is restarted, **When** a user reloads `https://noviq.pk`, **Then** the same address still serves the app — no new URL is issued.
4. **Given** a user opens `https://www.noviq.pk`, **When** the page resolves, **Then** they arrive at the same application rather than an error.

---

### User Story 2 - Official correspondence comes from a noviq.pk address and arrives (Priority: P2)

Staff send and receive mail at `@noviq.pk` — for NIC and SEIC applications, for
department contacts, and for tester onboarding. Mail sent from the domain lands in
the recipient's inbox rather than their spam folder, and mail sent to the domain
arrives in the mailbox.

**Why this priority**: Second because the app being reachable is the harder dependency
and the more visible failure. Mail matters for credibility — the business applications
currently list no website and no domain address — but the product works without it.
It ranks above cleanup because outsiders see it.

**Independent Test**: Send a message from a `@noviq.pk` mailbox to an external Gmail
and an external Outlook account and confirm it lands in the inbox, not spam. Reply to
it and confirm the reply arrives. This is testable the moment mail is configured,
independent of whether the app cutover has happened.

**Acceptance Scenarios**:

1. **Given** mail is configured for the domain, **When** a message is sent from a `@noviq.pk` address to an external Gmail account, **Then** it is delivered to the inbox and shows passing sender authentication.
2. **Given** mail is configured, **When** an external sender writes to a `@noviq.pk` address, **Then** the message arrives in the mailbox.
3. **Given** authentication policy is published in report-only mode, **When** two weeks of delivery reports have been collected, **Then** the reports show whether any legitimate mail is failing before the policy is tightened.

---

### User Story 3 - Nothing in the system still points at the dead address (Priority: P3)

After cutover, no script, document, or ops file advertises the old rotating URL.
Anyone reading the runbooks or the recorded "current URL" gets the permanent address.

**Why this priority**: Lowest because it harms nobody externally, but it is the
failure mode most likely to waste a future session. The current URL tracker only
recognises `trycloudflare` addresses, so after cutover it will never match again and
will sit holding a dead address indefinitely — stale rather than empty, which is the
kind of thing that gets trusted and then wastes an hour.

**Independent Test**: Read the recorded current-URL file and grep the ops docs for the
old hostname; confirm both show the permanent address and that nothing is still
watching for a rotating one.

**Acceptance Scenarios**:

1. **Given** cutover is complete, **When** the recorded current-URL value is read, **Then** it holds the permanent address and is not being rewritten by a watcher that can no longer match.
2. **Given** cutover is complete, **When** the ops and connectivity documents are searched for the previous quick-tunnel hostname, **Then** every occurrence has been updated or explicitly marked historical.

---

### Edge Cases

- **Delegation does not appear within 48 hours.** The registry may not publish the
  nameservers promptly. There must be a definite check that distinguishes "not yet
  published" from "entered incorrectly", so the wait does not silently repeat the last
  six days, during which the domain was assumed to be propagating when in fact no
  nameservers had ever been set.
- **The mail provider's regional data centre differs from the documented one.** The
  mail host names vary by region and the provider's public help pages do not flag the
  split. Records copied from documentation rather than from the account's own console
  will accept mail nowhere.
- **A second sender-policy record is added later.** A domain may publish only one such
  record; adding a second invalidates both, and the policy is a hard-fail, so all mail
  from the domain would begin bouncing. Any future sender must be merged into the
  existing single record.
- **The credential that lets a machine serve traffic on the domain leaks.** It grants
  the ability to publish on the zone and must not be committed to the repository or
  stored alongside other plaintext credentials on a desktop.
- **Live streaming behaviour may not change.** Answers have never streamed token by
  token to browsers through the current tunnel; they appear only when the request
  completes. The new path may or may not fix this. It must be measured after cutover
  and reported honestly, not assumed resolved.
- **Cutover happens while a user is mid-question.** Switching the public entry point
  interrupts in-flight requests. The interruption must be brief and recoverable by
  reload, and must not corrupt saved conversations.
- **The change needs to be undone.** If the new path fails, the previous working
  configuration must be restorable quickly rather than reconstructed from memory.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The application MUST be reachable at the permanent address `https://noviq.pk` over HTTPS with a certificate that browsers accept without warning.
- **FR-002**: The public address MUST survive a restart of the tunnel process, the application server, and the host machine, without being reissued or changed.
- **FR-003**: `www.noviq.pk` MUST resolve to the same application as the apex address.
- **FR-004**: Every capability available on the previous address — sign-in, corpus search, answers with citations, document upload, admin console — MUST be available on the new address with no functional regression.
- **FR-005**: The domain MUST accept inbound mail at `@noviq.pk` addresses and MUST allow sending from those addresses.
- **FR-006**: Mail sent from the domain MUST pass sender authentication at major external providers, verified by delivery to the inbox rather than the spam folder.
- **FR-007**: Domain authentication policy MUST be published initially in report-only mode with a reporting address, so failures are observed before enforcement is tightened.
- **FR-008**: Mail-related records MUST NOT be routed through the web proxy, which would prevent mail delivery.
- **FR-009**: The credential authorising the host to serve traffic on the domain MUST be stored with owner-only read permission outside the repository, and MUST NOT appear in version control, process listings, or shared credential files.
- **FR-010**: The previous configuration MUST be retained so the system can be returned to the working quick-tunnel path with a single, documented action.
- **FR-011**: The mechanism that tracked the rotating address MUST be retired, and the recorded current address MUST be set to the permanent one, so no component reports an address that no longer exists.
- **FR-012**: Delegation, mail records, and application reachability MUST each be verified by observed external behaviour — a query answered from the public internet, a message delivered, a page loaded and used — and not by the absence of errors in logs.
- **FR-013**: The verified state of live streaming on the new path MUST be recorded as a measured result, whether it improved, regressed, or was unchanged.

### Key Entities

- **Domain registration**: The `noviq.pk` record held at the national registry. Carries the delegation — the list of nameservers authoritative for the domain. Currently registered and paid through 2028-08-14 with an empty delegation, which is why the domain does not resolve at all.
- **DNS zone**: The authoritative set of records for the domain once delegation points at the chosen provider. Holds both the application's address records and the mail records; a single zone serves both purposes.
- **Named tunnel**: A persistent outbound connection from the application host to the edge network, identified by a stable identifier and authorised by a token. Unlike the current temporary tunnel, its public hostname is fixed rather than regenerated per run.
- **Mailbox domain**: The domain as registered with the mail provider, carrying its verification record, signing key, and delivery policy. Its exact values are generated per account and cannot be written in advance.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: A first-time tester given only the words "noviq.pk" reaches the sign-in page without being sent a link, on the first attempt.
- **SC-002**: The public address remains identical across at least three consecutive service restarts and one host reboot — zero address changes, where the previous arrangement changed on every restart.
- **SC-003**: 100% of the capabilities exercised in the pre-cutover check produce the same result on the new address, with no capability lost.
- **SC-004**: Messages sent from the domain to at least two major external providers are delivered to the inbox, with 0 of them classified as spam.
- **SC-005**: Total interruption to users during the switch is under 60 seconds, and any interrupted session recovers on reload with saved conversations intact.
- **SC-006**: After cutover, 0 files, scripts, or documents in the project report a public address that does not resolve.
- **SC-007**: The complete cutover, once delegation is live, is finished within a single working session rather than spread across days of waiting.

## Assumptions

- **Apex hostname chosen.** The application answers on the bare domain `noviq.pk`, with
  `www` reaching the same place. Confirmed by the architect during specification. The
  apex can carry both the application address and the mail records simultaneously.
- **Managed mail, not self-hosted.** Self-hosting was ruled out on measurement, not
  preference: the host has no inbound public address and no reverse-DNS control over
  its egress range, so outbound mail would be filtered as spam by major providers
  regardless of how correctly authentication records are published. For a
  government-facing product, silently filtered official correspondence is worse than
  having no mailbox. Zoho Mail is the chosen provider.
- **The zone moves to the tunnel provider's DNS.** A named tunnel's hostname resolves
  only as a proxied record on that provider's network, so the zone must be hosted
  there. This also places the mail records in the same panel, which is a convenience
  rather than a requirement.
- **Moving the delegation is currently risk-free.** The domain resolves to nothing
  today, so there is no live service to disturb. This is the cheapest possible moment
  to change nameservers, and that will not remain true once the domain is in use.
- **The existing quick tunnel keeps running until cutover.** The current address stays
  live throughout, so testers are never without access while the domain is prepared.
- **Mailbox count is small.** A handful of addresses for staff and a reporting address;
  no bulk provisioning, mailing lists, or shared-inbox workflows are in scope.

## Dependencies

- **Registry panel access** (architect only) — to set the delegation. Nothing in this
  feature can begin until this is done, and no agent-side work can substitute for it.
- **DNS and tunnel provider account** (architect only) — to create the zone, obtain the
  nameservers, create the named tunnel, and issue its token.
- **Mail provider account** (architect only) — to register the domain and generate its
  verification record, signing key, and regional mail hosts, none of which can be
  predicted ahead of signup.
- **Registry publication delay** — up to 48 hours between setting the delegation and
  the wider internet observing it. This is a wait, not a task.
- **Application host access** (agent) — to install the tunnel, swap the service
  configuration, retire the URL tracker, and verify behaviour.

## Out of Scope

- A marketing or landing site on the domain; the apex serves the application itself.
- Migrating existing conversations, accounts, or documents — the underlying service and
  its data are untouched by this change.
- Fixing live token streaming. Its behaviour on the new path is measured and reported
  (FR-013), but changing it is separate work.
- Retiring the VPN-based direct access path used for administration and deployment.
- Mailing lists, shared inboxes, or mail archiving and retention policy.
- Any change to how the application authenticates or authorises users.
