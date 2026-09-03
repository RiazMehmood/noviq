/**
 * Privacy policy and terms of service for noviq.pk.
 *
 * ⚠️ These are REQUIRED to publish the Google OAuth consent screen out of
 * Testing mode — Google will not let a public app run without both URLs.
 *
 * ⚠️⚠️ Every sentence here is a statement about what Noviq actually does with
 * real people's data, on a public page, and some of it is a promise a court
 * could read. Two rules for anyone editing this file:
 *
 *   1. Describe only what the code does TODAY. If a claim here stops matching
 *      the implementation, the page is not "slightly out of date", it is untrue.
 *      The specific claims below map to: `api/quotes.rs` (contact intake),
 *      `db/repositories/trial.rs` (trial accounts), migration 025 (audit log),
 *      and `RequestScope.trial` (demo-only retrieval).
 *   2. This is NOT legal advice and was not written by a lawyer. It is an honest
 *      description of the system. Before Noviq takes paying customers or
 *      processes anyone's confidential documents under contract, have it
 *      reviewed by someone qualified.
 *
 * Deliberately plain: a privacy policy nobody can read protects nobody.
 */
import { Component, For } from "solid-js";

/** Last substantive review of the text below. Update when the content changes. */
export const LEGAL_UPDATED = "25 August 2026";

interface Section {
  heading: string;
  paragraphs?: string[];
  bullets?: string[];
}

const PRIVACY: Section[] = [
  {
    heading: "Who this covers",
    paragraphs: [
      "This describes what happens to information on noviq.pk — the public website, " +
        "the free trial, and the request-a-quote form. It does not describe deployments " +
        "of Noviq installed on an organisation's own servers: in those, the organisation " +
        "controls the system and its data, and its own policies apply.",
    ],
  },
  {
    heading: "What we collect, and why",
    bullets: [
      "Request a quote: the organisation, name, email, phone, sector and message you type. " +
        "We use them to reply to you and to talk to you about Noviq.",
      "Free trial: when you sign in with Google we receive your Google account identifier, " +
        "your email address and your name. We use them to hold your trial and its limits, and " +
        "to contact you about Noviq — to ask how the trial went, and to tell you about the " +
        "product. Reply to any message asking us to stop and we will.",
      "Questions you ask during the trial: stored so the conversation works and so we can " +
        "count your daily usage against the trial limit.",
      "A record of access: accounts, sign-ins and document reads are logged so an " +
        "organisation can audit who saw what.",
      "Your IP address is recorded with a quote request, for rate limiting and abuse " +
        "handling only. It is not shown in our admin console.",
    ],
  },
  {
    heading: "What we do not do",
    bullets: [
      "We do not sell your information, we do not share it with anyone else, and we do not " +
        "share it for advertising. We contact you ourselves, about Noviq, and that is all.",
      "We do not use the free trial to train models.",
      "We have no analytics, advertising or social media trackers on this site.",
      "The trial does not give you access to any organisation's private documents, and it " +
        "does not put anything you write into anyone else's answers.",
    ],
  },
  {
    heading: "Where the free trial's answers come from",
    paragraphs: [
      "During the trial, Noviq answers only from a small set of already-published " +
        "instruments that we have chosen for the demonstration. It cannot reach any " +
        "organisation's internal or private files, by construction rather than by policy: " +
        "the retrieval scope for a trial account is restricted to those documents in the " +
        "server itself.",
    ],
  },
  {
    heading: "Where your information is held",
    paragraphs: [
      "Data for noviq.pk is held on servers we operate. Google receives your sign-in " +
        "request when you use the trial, because that is how signing in with Google works; " +
        "their handling of it is governed by Google's own privacy policy.",
      "Where a notification email is sent to us about your quote request, it passes through " +
        "our email provider.",
    ],
  },
  {
    heading: "How long we keep it",
    paragraphs: [
      "Quote requests are kept while we are in contact with you and afterwards as a record " +
        "of the enquiry. Trial accounts and their questions are kept while the trial is " +
        "active and for a short period afterwards. Write to us if you want yours removed " +
        "sooner.",
    ],
  },
  {
    heading: "Your choices",
    paragraphs: [
      "Ask us for a copy of what we hold about you, ask us to correct it, or ask us to " +
        "delete it, by writing to admin@noviq.pk. We will do it. If you signed in with " +
        "Google, you can also remove Noviq's access from your Google account at any time.",
    ],
  },
  {
    heading: "Changes",
    paragraphs: [
      "If this changes materially we will update this page and the date at the top of it.",
    ],
  },
];

const TERMS: Section[] = [
  {
    heading: "What Noviq is",
    paragraphs: [
      "Noviq answers questions from a body of documents and shows you the document, the " +
        "provision and the page each answer came from. It is a research tool.",
    ],
  },
  {
    heading: "It is not advice, and you remain responsible",
    paragraphs: [
      "Noviq does not give legal advice and does not make decisions. It retrieves and " +
        "quotes; the judgement is yours. Check the cited source before you rely on an " +
        "answer, and before anything goes into a file, a note or an order.",
      "Retrieval systems can miss a relevant provision or surface the wrong one. An answer " +
        "that cites its source is easier to verify than one that does not — it is not a " +
        "guarantee that the answer is complete or correct.",
    ],
  },
  {
    heading: "The free trial",
    bullets: [
      "Three questions per day, for three days from your first sign-in.",
      "Chat and Search only. The other features are for organisations and paid accounts.",
      "Answers come from a small demonstration set of published instruments, not from any " +
        "organisation's own records.",
      "It is provided as-is, for evaluation. We may end or change it at any time.",
      "One trial per person. Do not use it to build a competing service, to scrape the " +
        "documents, or to place automated load on the system.",
    ],
  },
  {
    heading: "Organisational deployments",
    paragraphs: [
      "Where Noviq is installed for an organisation, the written agreement with that " +
        "organisation governs it, and these terms do not limit or replace it.",
    ],
  },
  {
    heading: "Availability",
    paragraphs: [
      "This is a young product and the public site is offered without any uptime promise. " +
        "We may change or withdraw features here.",
    ],
  },
  {
    heading: "Contact",
    paragraphs: [
      "Write to admin@noviq.pk with anything about these terms.",
    ],
  },
];

const LegalBody: Component<{ title: string; intro: string; sections: Section[] }> = (p) => (
  <div class="mx-auto max-w-3xl px-6 py-16 sm:px-10 lg:py-24">
    <h1 class="text-3xl font-bold tracking-tight text-noviq-ink sm:text-4xl">{p.title}</h1>
    <p class="mt-3 text-[13px] text-noviq-600">Last updated {LEGAL_UPDATED}</p>
    <p class="mt-6 text-base leading-relaxed text-noviq-800">{p.intro}</p>

    <For each={p.sections}>
      {(s) => (
        <section class="mt-10">
          <h2 class="text-lg font-semibold tracking-tight text-noviq-ink">{s.heading}</h2>
          <For each={s.paragraphs ?? []}>
            {(t) => <p class="mt-3 text-[15px] leading-relaxed text-noviq-800">{t}</p>}
          </For>
          <ul class="mt-3 space-y-2.5">
            <For each={s.bullets ?? []}>
              {(t) => (
                <li class="flex gap-2.5 text-[15px] leading-relaxed text-noviq-800">
                  <span class="mt-2 h-1.5 w-1.5 flex-shrink-0 rounded-full bg-noviq-400" />
                  {t}
                </li>
              )}
            </For>
          </ul>
        </section>
      )}
    </For>

    <p class="mt-14 rounded-xl border border-noviq-line bg-noviq-panel/50 px-5 py-4 text-[13px] leading-relaxed text-noviq-700">
      Questions about any of this go to{" "}
      <a class="font-medium text-noviq-800 underline" href="mailto:admin@noviq.pk">
        admin@noviq.pk
      </a>
      .
    </p>
  </div>
);

export const PrivacyPolicy: Component = () => (
  <LegalBody
    title="Privacy"
    intro="What we collect on noviq.pk, why, and what we do not do with it. In plain language, because a privacy policy nobody can read protects nobody."
    sections={PRIVACY}
  />
);

export const TermsOfService: Component = () => (
  <LegalBody
    title="Terms"
    intro="The terms for using noviq.pk and its free trial. Noviq is a research tool that cites its sources — it does not give advice and does not decide anything."
    sections={TERMS}
  />
);
