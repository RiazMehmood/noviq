/**
 * LoginScreen (F014, US2) — the browser thin client's sign-in surface.
 * Shown by `App` (web transport only) until a valid session exists.
 *
 * 2026-08-21 — rebuilt as the public landing page for noviq.pk. The F028 cutover
 * gave the product a permanent public address, so this is now the first thing an
 * unauthenticated visitor sees, not just a gate for testers.
 *
 * ⚠️ POSITIONING — Noviq is NOT a government-only product. Per the NCS note to the
 * Chief Secretary (§5): "a secure, offline AI Application tailored specifically to
 * the requirements of ANY organization, initially starting with the Government of
 * Sindh." Government is the first deployment, not the ceiling. An earlier draft of
 * this page said "for government documents" throughout and had to be corrected —
 * do not narrow it again.
 *
 * ⚠️ ATTRIBUTION — the TMR Wing assisted at particular points; it did NOT supervise
 * development throughout. Annexure-E and the NCS note both say "under the supervision
 * and guidance of the TMR Wing", but the founders corrected that for public use
 * (2026-08-21). The page must not claim more than the founders stand behind.
 *
 * ⚠️ The sign-in logic is byte-identical to the original — `login()`, the
 * 429/ApiError branches and the `history.replaceState` landing fix. This file is
 * presentation only; do not fold behaviour into it.
 *
 * Palette is the Noviq green research-station scale from `tailwind.config.js`
 * (`noviq-*`, `ink`, `paper`, `panel`, `line`) — the same tokens the signed-in app
 * uses, so the seam between landing and product is invisible.
 */
import { Component, createSignal, For, onMount, Show } from "solid-js";
import { login } from "../stores/auth";
import { ApiError } from "../api/transport";
import QuoteRequestForm from "./QuoteRequestForm";
import { PrivacyPolicy, TermsOfService } from "./LegalPages";
import GoogleSignInButton from "./GoogleSignInButton";
import markUrl from "../assets/noviq_mark.png";

import govtSindhLogo from "../assets/institutions/govt-of-sindh.png";
import ibaLogo from "../assets/institutions/iba-karachi.png";
import sukkurIbaLogo from "../assets/institutions/sukkur-iba.png";
import majuLogo from "../assets/institutions/maju.png";
import fastLogo from "../assets/institutions/fast-nuces.png";
import israLogo from "../assets/institutions/isra.png";
import proximaLogo from "../assets/institutions/proxima-ai.png";

import khojaPhoto from "../assets/people/shakeel-khoja.jpg";
import haiderPhoto from "../assets/people/sajjad-haider.jpg";
import imranKhanPhoto from "../assets/people/imran-khan.jpg";
import tariqMahmoodPhoto from "../assets/people/tariq-mahmood.jpg";
import soomraniPhoto from "../assets/people/abdul-rehman-soomrani.jpg";
import mujtabaPhoto from "../assets/people/ghulam-mujtaba-shaikh.jpg";
import adnanZaidiPhoto from "../assets/people/adnan-zaidi.jpg";
import razaAbbasPhoto from "../assets/people/raza-abbas.jpg";
import balochPhoto from "../assets/people/gul-sher-baloch.jpg";
import tafseerPhoto from "../assets/people/tafseer-ahmed.jpg";
import muteePhoto from "../assets/people/mutee-u-rahman.jpg";
import riazPhoto from "../assets/people/riaz-mehmood-khushik.jpg";
import younusPhoto from "../assets/people/mohammad-younus-shaikh.jpg";
import noorPhoto from "../assets/people/noor-ahmed-samoo.jpg";

const FOUNDERS: {
  name: string;
  role: string;
  /** Fallback monogram; still used if a photo is ever removed. */
  initials: string;
  photo?: string;
  linkedin?: string;
}[] = [
  // URLs and photographs come from the founders themselves — never from a search.
  // Younus Shaikh has not supplied a LinkedIn URL, so his name renders as plain text.
  {
    name: "Riaz Mehmood Khushik",
    role: "Developer",
    initials: "RK",
    photo: riazPhoto,
    linkedin: "https://www.linkedin.com/in/riaz-mehmood-330261401/",
  },
  {
    name: "Mohammad Younus Shaikh",
    role: "Co-Developer",
    initials: "MS",
    photo: younusPhoto,
  },
];

type Person = {
  /** Preferred public name. See the NAMES note above — this is not always Annexure-E. */
  name: string;
  /** Current designation, taken from the institution's own faculty page. */
  title?: string;
  /**
   * A second, concurrent affiliation outside the institution this card groups them
   * under. Exists because two contributors' LinkedIn headlines named a different
   * organisation than Annexure-E did; rather than pick one, the page shows both.
   */
  alsoAt?: string;
  note?: string;
  photo?: string;
  linkedin?: string;
  /**
   * Renders the entry larger, bolder and on a tinted panel. Reserved for the most
   * senior endorsement on the page — not a general emphasis knob. If a second person
   * ever gets it, the distinction stops meaning anything.
   */
  featured?: boolean;
};

/**
 * ⚠️ `path` groups the acknowledgements into the THREE tracks the architect
 * specified (2026-08-25): government, academia, industry. Technical
 * contribution is stated in those three places and NOWHERE else on the page —
 * do not re-list contributors under the audience tracks or in the hero.
 *
 * Note this is a different axis from `AUDIENCES` above, which says who the
 * product is FOR. These say who helped build it.
 */
type ContributorPath = "government" | "academia" | "industry";

type Institution = {
  institution: string;
  logo: string;
  path: ContributorPath;
  people: Person[];
};

/** The three paths, in the order they are shown, with what each one contributed. */
const CONTRIBUTOR_PATHS: Array<{ id: ContributorPath; title: string; blurb: string }> = [
  {
    id: "government",
    title: "Government",
    blurb:
      "The institutional setting the system was built for, and assistance at various " +
      "stages from the TMR Wing.",
  },
  {
    id: "academia",
    title: "Academia",
    blurb:
      "Technical and academic input on artificial intelligence, computing and natural " +
      "language processing — and the computing capacity the system was developed and " +
      "evaluated on.",
  },
  {
    id: "industry",
    title: "Industry",
    blurb: "Independent security review of the deployed system.",
  },
];

/**
 * Source: Annexure-E, "Technical Contributors and Institutional Support".
 * ⚠️ Real people, publicly credited.
 *
 * ⚠️ NAMES DIVERGE FROM ANNEXURE-E BY DECISION (architect, 2026-08-21): where the
 * institution's own faculty page spelled a name differently, the institution wins.
 * Annexure-E says Khowaja / Hyder / Mehmood; IBA's own records say **Khoja / Haider /
 * Mahmood**, and that is what this page shows. Do not "restore" the annexure spelling.
 * Same decision applied to **Gul Sher Baloch** (Annexure-E: "Sher Mohammad Baloch") and
 * **Mutee U Rahman** (Annexure-E: "Matee ur Rehman"). Every name on this page is now the
 * one the person themselves uses; none is left on an Annexure-E spelling.
 *
 * PROVENANCE RULES for the three fields added 2026-08-21 — every one of them makes a
 * factual claim about a real person on a public page, so:
 *
 * - `title` is copied from the institution's OWN faculty page (iba.edu.pk /
 *   iba-suk.edu.pk profile pages), never from a search-engine summary. Designations
 *   change; re-check before citing them elsewhere.
 * - `linkedin` is set only where the profile's own headline names the matching
 *   institution. Where no such profile could be found the field is simply absent and
 *   the name renders as plain text — an absent link is correct, a wrong link is a
 *   misattribution to a real person. Do NOT fill these in by guessing.
 * - `photo` files are the headshots those same official faculty pages publish. They
 *   were NOT supplied by their subjects for this page. Prefer replacing them with
 *   images the contributors provide; a missing photo degrades to an initials monogram.
 */
const CONTRIBUTORS: Institution[] = [
  {
    // Placed first at the architect's instruction (2026-08-21).
    // ⚠️ Name and designation were supplied by the architect and could NOT be
    // corroborated against the department's own site — unlike every other entry here,
    // whose title comes from an official faculty page. Confirm before treating this
    // line as verified.
    // ⚠️ The department's own site (istd.sindh.gov.pk) styles itself "Information
    // Science and Technology Department", not "S&IT". Using the official form.
    institution: "Government of Sindh",
    path: "government",
    logo: govtSindhLogo,
    people: [
      // ⚠️ Supplied by the architect (2026-08-21), like Saif ul Malook — neither name
      // nor designation is corroborated by the department's own site. A public search
      // does place a Noor Ahmed Samoo as Secretary in this department, but "TMR Wing"
      // is the architect's wording and is what the founders stand behind.
      {
        name: "Noor Ahmed Samoo",
        title: "Secretary, TMR Wing",
        photo: noorPhoto,
        linkedin: "https://www.linkedin.com/in/noor-ahmed-samoo-47308b300/",
        featured: true,
      },
      {
        name: "Saif ul Malook",
        title: "Director, Information Science & Technology Department",
      },
    ],
  },
  {
    institution: "IBA Karachi",
    path: "academia",
    logo: ibaLogo,
    people: [
      {
        name: "Dr. Shakeel Ahmed Khoja",
        title: "Professor & Dean, School of Mathematics and Computer Science",
        note: "Extended GPU and computing support for development and testing.",
        photo: khojaPhoto,
        linkedin: "https://www.linkedin.com/in/shakeel-khoja-54209a6/",
      },
      {
        name: "Dr. Sajjad Haider",
        title: "Professor, Computer Science",
        photo: haiderPhoto,
      },
      {
        name: "Dr. Imran Khan",
        title: "Assistant Professor, School of Mathematics and Computer Science",
        photo: imranKhanPhoto,
        linkedin: "https://www.linkedin.com/in/imran-khan-51281b3/",
      },
      {
        name: "Dr. Tariq Mahmood",
        title: "Professor, School of Mathematics and Computer Science",
        photo: tariqMahmoodPhoto,
        linkedin: "https://www.linkedin.com/in/dr-tariq-mahmood-717b7b28/",
      },
    ],
  },
  {
    institution: "Sukkur IBA University",
    path: "academia",
    logo: sukkurIbaLogo,
    people: [
      {
        name: "Dr. Abdul Rehman Soomrani",
        title: "Pro Vice Chancellor (Sub-Campuses)",
        photo: soomraniPhoto,
        linkedin: "https://www.linkedin.com/in/prof-dr-abdul-rehman-008864122/",
      },
      {
        name: "Dr. Ghulam Mujtaba Shaikh",
        title: "Professor & Head of Department, Computer Science",
        photo: mujtabaPhoto,
        linkedin: "https://www.linkedin.com/in/ghulam-mujtaba-shaikh-4459b1a8/",
      },
      // Profile, both affiliations, and the name spelling all confirmed by the
      // architect (2026-08-21). Annexure-E says "Sher Mohammad Baloch"; he goes by
      // **Gul Sher Baloch**, which is what this page shows.
      {
        name: "Dr. Gul Sher Baloch",
        title: "Associate Professor",
        photo: balochPhoto,
        alsoAt: "Founder, Leaveit2AI",
        linkedin: "https://www.linkedin.com/in/gs-baloch/",
      },
    ],
  },
  {
    institution: "Mohammad Ali Jinnah University",
    path: "academia",
    logo: majuLogo,
    people: [
      {
        name: "Prof. Dr. Tafseer Ahmed",
        title: "Director, C-RAAT",
        photo: tafseerPhoto,
        linkedin: "https://www.linkedin.com/in/tafseer-ahmed-8231a22/",
      },
    ],
  },
  {
    institution: "FAST University",
    path: "academia",
    logo: fastLogo,
    // ⚠️ Not listed on FAST-NUCES Karachi's public team directory (2026-08-21). His
    // LinkedIn is headlined "Securiti"; the architect confirmed he holds both, so the
    // card groups him under FAST and names Securiti via `alsoAt`.
    people: [
      {
        name: "Dr. Raza Abbas",
        alsoAt: "Securiti",
        photo: razaAbbasPhoto,
        linkedin: "https://www.linkedin.com/in/raza-abbas-128950121/",
      },
    ],
  },
  {
    institution: "Isra University",
    path: "academia",
    logo: israLogo,
    // Affiliation confirmed by the founders 2026-08-21: Isra University, per Annexure-E.
    // The NCS note §7 grouping him with MAJU is the incorrect one — do not "fix" this back.
    // Annexure-E says "Matee ur Rehman"; he goes by **Mutee U Rahman** per his own
    // LinkedIn, and the architect confirmed that spelling (2026-08-21).
    people: [
      {
        name: "Prof. Dr. Mutee U Rahman",
        photo: muteePhoto,
        linkedin: "https://www.linkedin.com/in/mutee-u-rahman-8914335/",
      },
    ],
  },
  {
    institution: "Proxima AI",
    path: "industry",
    logo: proximaLogo,
    people: [
      {
        name: "Dr. Adnan Zaidi",
        title: "Chief AI Officer",
        photo: adnanZaidiPhoto,
        note: "Independent security review and testing of the platform.",
        linkedin: "https://www.linkedin.com/in/adnan-zaidi-49a8a645/",
      },
    ],
  },
];

/** Initials for the monogram shown when a person has no photo. Honorifics dropped. */
const initialsOf = (name: string) =>
  name
    .replace(/^(?:(?:Prof|Dr|Mr|Ms|Mrs)\.?\s+)+/i, "")
    .trim()
    .split(/\s+/)
    // Drop Arabic/Urdu name particles ("ul", "ur", "bin", "al") — they are not
    // name words, and including them yields "SU" for Saif ul Malook.
    .filter((w) => w.length > 1 && !["ul", "ur", "al", "bin", "el", "der", "van"].includes(w.toLowerCase()))
    .slice(0, 2)
    .map((w) => w[0]!.toUpperCase())
    .join("");

/** LinkedIn "in" glyph. Marked aria-hidden — the link text carries the accessible name. */
const LinkedInMark: Component<{ class?: string }> = (props) => (
  <svg
    class={props.class}
    viewBox="0 0 24 24"
    fill="currentColor"
    aria-hidden="true"
  >
    <path d="M20.45 20.45h-3.56v-5.57c0-1.33-.03-3.04-1.85-3.04-1.85 0-2.14 1.45-2.14 2.94v5.67H9.35V9h3.41v1.56h.05a3.74 3.74 0 0 1 3.37-1.85c3.6 0 4.27 2.37 4.27 5.46v6.28zM5.34 7.43a2.07 2.07 0 1 1 0-4.13 2.07 2.07 0 0 1 0 4.13zm1.78 13.02H3.55V9h3.57v11.45zM22.22 0H1.77C.79 0 0 .77 0 1.72v20.56C0 23.23.79 24 1.77 24h20.45c.98 0 1.78-.77 1.78-1.72V1.72C24 .77 23.2 0 22.22 0z" />
  </svg>
);

/**
 * The five tracks the landing page speaks to.
 *
 * ⚠️ `fear` is the point of this structure. Each card leads with what the READER
 * stands to lose, not with what the product has — the hero sets that tone and
 * these must not drop it into a feature list.
 *
 * ⚠️ Claims discipline: every `body` describes RETRIEVAL WITH CITATIONS, which
 * is what Noviq does. None of them may promise a decision, legal advice, a
 * guarantee of completeness, or an accuracy comparison against another product.
 * `id` must match `SECTORS` in `api/quotes.rs` and `QuoteRequestForm`.
 */
/**
 * What the free demo actually allows. ⚠️ Every line here is a PROMISE on a
 * public page and each is enforced server-side, not by this list. Do not add a
 * capability here before the server refuses it — the deep-search lesson: the UI
 * is presentation, never the gate.
 */
const DEMO_TERMS: string[] = [
  "3 questions a day, for 3 days.",
  "A small set of published instruments — not any organisation's own files.",
  "Chat and Search only; the other tabs are for organisations and paid accounts.",
  "No card, no commitment.",
  // ⚠️ Stated HERE, at the point of sign-in, because burying it in a policy page
  // is not the same as telling someone. Keep in step with "What we collect, and
  // why" in `LegalPages` — if one changes and the other does not, the page lies.
  "We will email you about Noviq afterwards. Ask us to stop and we will.",
];

const AUDIENCES: Array<{ id: string; title: string; fear: string; body: string; icon: string }> = [
  {
    id: "government",
    title: "Government",
    fear: "You are answerable for a rule you never found.",
    body:
      "Ask across acts, rules, notifications and circulars in plain language. Every answer " +
      "carries the instrument, the provision and the page, so a note you draft can be checked " +
      "by whoever reads it next.",
    icon: "M3 10h18M4 10 12 4l8 6M6 10v7M10 10v7M14 10v7M18 10v7M3 20h18",
  },
  {
    id: "academia",
    title: "Academia",
    fear: "The regulation changed and your reading list did not.",
    body:
      "Put statutes, ordinances, syllabi and research material in one place and question it " +
      "directly. Citations point back to the source document, so a claim can be traced rather " +
      "than taken on trust.",
    icon: "M12 4 2 9l10 5 10-5-10-5ZM6 11.5V17c0 1.5 3 3 6 3s6-1.5 6-3v-5.5",
  },
  {
    id: "industry",
    title: "Industry",
    fear: "An auditor asks which clause you relied on.",
    body:
      "Contracts, tender documents, SOPs and compliance manuals become answerable. Ask what a " +
      "clause requires and get the paragraph it comes from, on infrastructure inside your own " +
      "network.",
    icon: "M3 21h18M5 21V8l7-4 7 4v13M9 21v-5h6v5M9 11h.01M15 11h.01",
  },
  {
    id: "developers",
    title: "Developers",
    fear: "The spec is right there and nobody can find it.",
    body:
      "Point Noviq at your specifications, standards and internal documentation. It runs on your " +
      "own machines, with no document leaving them, and answers with the file and section it read.",
    icon: "M8 6 2 12l6 6M16 6l6 6-6 6M13.5 4l-3 16",
  },
  {
    id: "policy",
    title: "Policy",
    fear: "Two instruments say different things and nobody noticed.",
    body:
      "Ask what the current position is across a body of rules and see each source stated " +
      "separately, with its citation, instead of collapsed into one confident paragraph.",
    icon: "M6 3h9l5 5v13a1 1 0 0 1-1 1H6a1 1 0 0 1-1-1V4a1 1 0 0 1 1-1ZM14 3v6h6M9 13h7M9 17h5",
  },
];

/**
 * What an organisation gets. Deliberately concrete and deliberately modest —
 * ⚠️ no pricing appears here. Pricing is quoted per deployment, and a number on
 * the public page would be quoted back at us in every negotiation.
 */
const QUOTE_POINTS: string[] = [
  "Deployed on your own servers — documents and questions never leave them.",
  "Your material indexed: acts, rules, notifications, contracts, manuals, records.",
  "Every answer cites the document, the provision and the page it came from.",
  "Accounts for your people, with an audit trail of what was read and asked.",
  "Onboarding is one-time and fixed-scope; we do not staff on site.",
];

const LoginScreen: Component = () => {
  const [username, setUsername] = createSignal("");
  const [password, setPassword] = createSignal("");
  const [showPassword, setShowPassword] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);
  const [busy, setBusy] = createSignal(false);

  const handleSubmit = async (e: Event) => {
    e.preventDefault();
    if (busy()) return;
    setError(null);
    setBusy(true);
    try {
      await login(username().trim(), password());
      // On success, `currentUser` flips and App swaps in the app shell. The
      // Router mounts at whatever URL the browser was left on (e.g. /settings
      // or /admin from a stale session/bookmark), which strands fresh sign-ins
      // on pages they didn't ask for — and 403s non-admins. Every sign-in
      // lands on Chat; users navigate from there.
      window.history.replaceState(null, "", "/");
    } catch (err) {
      const msg =
        err instanceof ApiError
          ? err.code === "429"
            ? "Too many attempts — please wait and try again."
            : "Invalid username or password."
          : "Could not reach the server. Check your connection and try again.";
      setError(msg);
    } finally {
      setBusy(false);
    }
  };

  // Which track the visitor came from, so the form arrives pre-selected. Set by
  // the audience cards; the anchor jump does the scrolling.
  const [quoteSector, setQuoteSector] = createSignal("government");

  /**
   * Which screen this component is showing. Sign-in and the demo are separate
   * SCREENS, not sections of the landing page (architect, 2026-08-25).
   *
   * Deliberately local state rather than routes: this whole component renders
   * only when there is no session, so these screens must not appear in the app
   * router — a route would be reachable, and meaningless, once signed in.
   */
  const [view, setView] = createSignal<
    "landing" | "signin" | "demo" | "team" | "privacy" | "terms"
  >("landing");

  /**
   * Return to the landing page and, optionally, scroll to one of its sections.
   *
   * The scroll is deferred because the section does not exist in the DOM until
   * the `landing` branch has rendered — setting `location.hash` or calling
   * `scrollIntoView` in the same tick silently does nothing.
   */
  // ⚠️ Anything reading "Request a quote" must call this with "quote". Calling
  // `setView("landing")` bare lands the reader at the top of the page and leaves
  // them to hunt for the form — which is what the sign-in and demo screens did
  // until 2026-08-25. The ONLY bare `setView("landing")` should be the logo.
  /**
   * ⚠️ Privacy and Terms must be REAL URLs, not view state.
   *
   * Google will not publish an OAuth consent screen without a reachable privacy
   * policy and terms URL, and it fetches them. Before this, `/privacy` rendered
   * the landing page — this component is shown for ANY path while signed out,
   * so the view signal alone was invisible to the outside world.
   *
   * The two directions:
   *  - on load, the path selects the view (so a pasted link works, and Google's
   *    fetch of https://noviq.pk/privacy gets the policy);
   *  - `showPage` pushes history, so the URL follows the view and Back works.
   */
  const PATH_VIEWS: Record<string, "privacy" | "terms" | "team" | "demo"> = {
    "/privacy": "privacy",
    "/terms": "terms",
    "/team": "team",
    "/demo": "demo",
  };

  onMount(() => {
    const v = PATH_VIEWS[window.location.pathname.replace(/\/+$/, "") || "/"];
    if (v) setView(v);
    // Back/forward must move between these pages, not out of the site.
    window.addEventListener("popstate", () => {
      const nv = PATH_VIEWS[window.location.pathname.replace(/\/+$/, "") || "/"];
      setView(nv ?? "landing");
    });
  });

  /** Switch view AND url together. */
  const showPage = (v: "privacy" | "terms" | "team" | "demo") => {
    setView(v);
    const path = Object.keys(PATH_VIEWS).find((k) => PATH_VIEWS[k] === v) ?? "/";
    window.history.pushState({}, "", path);
    window.scrollTo({ top: 0 });
  };

  const goLanding = (id?: string) => {
    setView("landing");
    if (window.location.pathname !== "/") window.history.pushState({}, "", "/");
    if (!id) return;
    setTimeout(() => {
      document.getElementById(id)?.scrollIntoView({ behavior: "smooth", block: "start" });
    }, 0);
  };

  const inputClass =
    "w-full px-3.5 py-2.5 text-sm bg-white border border-noviq-line rounded-lg text-noviq-ink " +
    "placeholder:text-noviq-400/70 focus:outline-none focus:border-noviq-500 focus:ring-2 " +
    "focus:ring-noviq-500/20 disabled:opacity-50 transition-colors";

  // ⚠️ The root must NOT carry `overflow-y-auto`. An ancestor with `overflow`
  // becomes the containing scroll box for `position: sticky`, so the header
  // below stuck to THIS element's top edge — which scrolls away with the page —
  // instead of to the viewport. Letting the document scroll is what makes the
  // navbar stick.
  return (
    <div class="min-h-screen w-full bg-noviq-paper text-noviq-ink antialiased">
      {/* ── Top bar ──────────────────────────────────────────────────── */}
      {/* Glass bar: mostly transparent with a heavy blur and a hairline, so the
          page reads THROUGH it as it scrolls past. `supports-[backdrop-filter]`
          keeps it legible where blur is unavailable — without the fallback the
          bar would be a 60%-opaque smear over live text. */}
      <header
        class="sticky top-0 z-30 border-b border-noviq-line/60 bg-noviq-paper/95
               backdrop-blur-xl supports-[backdrop-filter]:bg-noviq-paper/60"
      >
        {/* ⚠️ Split by decision (architect, 2026-08-25): the RIGHT side carries
            only Sign in and Request a quote. Every other destination sits on the
            LEFT beside the wordmark. Do not add a third item to the right.
            A "Back" button lived here briefly and was removed: the left nav and
            the logo are visible on every sub-page, so it was a third right-hand
            item duplicating two controls already on screen. */}
        <div class="mx-auto flex max-w-7xl items-center justify-between gap-4 px-5 py-3 sm:px-8">
          <div class="flex min-w-0 items-center gap-6">
            {/* Mark + wordmark (architect, 2026-08-25). `noviq_mark.png` is the
                head on its own, alpha-trimmed — `noviq_logo_3.png` carries an
                opaque white plate that paints a visible box on the paper canvas,
                and its own wordmark is illegible at this size. */}
            <button
              type="button"
              onClick={() => setView("landing")}
              class="flex flex-shrink-0 items-center gap-2"
              aria-label="Noviq — home"
            >
              <img src={markUrl} alt="" class="h-8 w-8 object-contain" />
              <span class="text-[15px] font-semibold tracking-tight text-noviq-ink">Noviq</span>
            </button>
            {/* Always visible, including on the sub-pages — a nav that vanishes
                when you leave the landing page strands you there. `Team` is a
                PAGE (architect, 2026-08-25); the other two are sections of the
                landing page, so they switch back to it and then scroll. */}
            <nav class="hidden items-center gap-5 text-[13px] sm:flex">
              <button
                type="button"
                onClick={() => goLanding("who")}
                class="text-noviq-700 hover:text-noviq-900"
              >
                Who it is for
              </button>
              <button
                type="button"
                onClick={() => showPage("team")}
                class="hover:text-noviq-900"
                classList={{
                  "font-semibold text-noviq-900": view() === "team",
                  "text-noviq-700": view() !== "team",
                }}
              >
                Team
              </button>
              <button
                type="button"
                onClick={() => goLanding("acknowledgements")}
                class="hidden text-noviq-700 hover:text-noviq-900 md:inline"
              >
                Contributors
              </button>
            </nav>
          </div>

          <nav class="flex flex-shrink-0 items-center gap-3 text-[13px] sm:gap-4">
            <Show when={view() !== "signin"}>
              <button type="button" onClick={() => setView("signin")}
                      class="text-noviq-700 hover:text-noviq-900">
                Sign in
              </button>
            </Show>
            <button
              type="button"
              onClick={() => goLanding("quote")}
              class="rounded-lg bg-noviq-700 px-3.5 py-1.5 font-medium text-white hover:bg-noviq-800"
            >
              Request a quote
            </button>
          </nav>
        </div>
      </header>

      {/* ── Sign in — its own screen ───────────────────────────────────
          Off the landing page by decision (architect, 2026-08-25). A visitor
          without credentials should never have been asked for them; the people
          who DO have them arrive here deliberately from the nav. */}
      <Show when={view() === "signin"}>
        <div class="mx-auto flex min-h-[78vh] max-w-md flex-col justify-center px-6 py-16">
          <div
            id="signin"
            class="w-full rounded-2xl border border-noviq-line bg-white p-6 shadow-sm sm:p-8"
          >
            <h2 class="text-xl font-bold tracking-tight text-noviq-ink">Sign in</h2>
            <p class="mt-1.5 text-[13px] text-noviq-600/85">
              Enter the credentials issued by your organisation.
            </p>

            <form onSubmit={handleSubmit} class="mt-8">
              <label for="noviq-username" class="mb-1.5 block text-xs font-medium text-noviq-700">
                Username
              </label>
              <input
                id="noviq-username"
                type="text"
                autocomplete="username"
                value={username()}
                onInput={(e) => setUsername(e.currentTarget.value)}
                disabled={busy()}
                class={inputClass + " mb-4"}
              />

              <label for="noviq-password" class="mb-1.5 block text-xs font-medium text-noviq-700">
                Password
              </label>
              <div class="relative mb-5">
                <input
                  id="noviq-password"
                  type={showPassword() ? "text" : "password"}
                  autocomplete="current-password"
                  value={password()}
                  onInput={(e) => setPassword(e.currentTarget.value)}
                  disabled={busy()}
                  class={inputClass + " pr-10"}
                />
                <button
                  type="button"
                  onClick={() => setShowPassword((v) => !v)}
                  tabindex="-1"
                  aria-label={showPassword() ? "Hide password" : "Show password"}
                  title={showPassword() ? "Hide password" : "Show password"}
                  class="absolute inset-y-0 right-0 flex items-center px-3 text-noviq-400 hover:text-noviq-600"
                >
                  <Show
                    when={showPassword()}
                    fallback={
                      <svg class="h-4 w-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2"
                          d="M15 12a3 3 0 11-6 0 3 3 0 016 0z" />
                        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2"
                          d="M2.458 12C3.732 7.943 7.523 5 12 5c4.478 0 8.268 2.943 9.542 7-1.274 4.057-5.064 7-9.542 7-4.477 0-8.268-2.943-9.542-7z" />
                      </svg>
                    }
                  >
                    <svg class="h-4 w-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                      <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2"
                        d="M13.875 18.825A10.05 10.05 0 0112 19c-4.478 0-8.268-2.943-9.543-7a9.97 9.97 0 011.563-3.029m5.858.908a3 3 0 114.243 4.243M9.878 9.878l4.242 4.242M9.88 9.88l-3.29-3.29m7.532 7.532l3.29 3.29M3 3l3.59 3.59m0 0A9.953 9.953 0 0112 5c4.478 0 8.268 2.943 9.542 7a10.025 10.025 0 01-4.132 5.411m0 0L21 21" />
                    </svg>
                  </Show>
                </button>
              </div>

              <Show when={error()}>
                <div
                  role="alert"
                  class="mb-4 rounded-lg border border-red-200 bg-red-50 p-2.5 text-sm text-red-700"
                >
                  {error()}
                </div>
              </Show>

              <button
                type="submit"
                disabled={busy() || !username().trim() || !password()}
                class="w-full rounded-lg bg-noviq-700 px-3 py-2.5 text-sm font-semibold text-white shadow-sm transition-colors hover:bg-noviq-800 disabled:cursor-not-allowed disabled:opacity-50"
              >
                {busy() ? "Signing in…" : "Sign in"}
              </button>
            </form>

            <p class="mt-5 text-[11px] leading-relaxed text-noviq-600/75">
              Access is issued by your organisation. Noviq runs on infrastructure it
              controls; documents and activity stay within it.
            </p>
          </div>
          <p class="mt-6 text-center text-[13px] text-noviq-600">
            No account?{" "}
            <button type="button" onClick={() => goLanding("quote")}
                    class="font-medium text-noviq-800 underline">
              Request a quote
            </button>{" "}
            or{" "}
            <button type="button" onClick={() => showPage("demo")}
                    class="font-medium text-noviq-800 underline">
              try the demo
            </button>
            .
          </p>
        </div>
      </Show>

      {/* ── Privacy and Terms ──────────────────────────────────────────
          ⚠️ Both URLs are REQUIRED to publish the Google OAuth consent screen
          out of Testing mode. They are also the only pages here that make
          promises about real people's data — see the header of `LegalPages`
          before editing a word of them. */}
      <Show when={view() === "privacy"}>
        <PrivacyPolicy />
      </Show>
      <Show when={view() === "terms"}>
        <TermsOfService />
      </Show>

      {/* ── Team — its own PAGE ────────────────────────────────────────
          Was a strip in the landing scroll; the architect asked for a page
          (2026-08-25). Given the room, the two founders get portrait-scale
          photographs instead of the 44px avatars they had.

          ⚠️ Name, role, photograph and LinkedIn are ALL that is stated here, and
          all four come from the founders themselves. Do not add designations,
          employers or biographies — these are real people, this is a public
          page, and nothing here may be inferred or filled in from elsewhere. */}
      <Show when={view() === "team"}>
        <div class="mx-auto max-w-5xl px-6 py-20 sm:px-10 lg:py-28">
          <div class="text-center">
            <div class="text-[11px] font-semibold uppercase tracking-[0.16em] text-noviq-500">
              Team
            </div>
            <h1 class="mt-3 text-4xl font-bold leading-tight tracking-tight text-noviq-ink sm:text-5xl">
              Built by two serving officers.
            </h1>
            <p class="mx-auto mt-5 max-w-2xl text-base leading-relaxed text-noviq-700 sm:text-lg">
              Noviq is built by people who do this work, for people who do this work — which
              is why it answers from the rulebook rather than from the internet.
            </p>
          </div>

          <div class="mx-auto mt-16 grid max-w-3xl gap-8 sm:grid-cols-2">
            <For each={FOUNDERS}>
              {(f) => (
                <div class="flex flex-col items-center rounded-3xl border border-noviq-line bg-white px-6 py-10 text-center">
                  <Show
                    when={f.photo}
                    fallback={
                      <span
                        aria-hidden="true"
                        class="flex h-32 w-32 items-center justify-center rounded-full bg-noviq-100
                               text-2xl font-semibold tracking-wide text-noviq-700"
                      >
                        {f.initials}
                      </span>
                    }
                  >
                    <img
                      src={f.photo}
                      alt=""
                      loading="lazy"
                      class="h-32 w-32 rounded-full object-cover ring-4 ring-noviq-50"
                    />
                  </Show>

                  <div class="mt-6 text-xl font-semibold tracking-tight text-noviq-ink">
                    {f.name}
                  </div>
                  <div class="mt-2 text-[11px] font-semibold uppercase tracking-[0.16em] text-noviq-600">
                    {f.role}
                  </div>

                  {/* Rendered only where the founder supplied a URL. An absent
                      link is correct; a guessed one is a misattribution. */}
                  <Show when={f.linkedin}>
                    <a
                      href={f.linkedin}
                      target="_blank"
                      rel="noopener noreferrer"
                      class="mt-5 inline-flex items-center gap-2 rounded-full border border-noviq-line
                             px-4 py-1.5 text-[13px] font-medium text-noviq-700 transition-colors
                             hover:border-noviq-400 hover:text-noviq-900"
                    >
                      <LinkedInMark class="h-3.5 w-3.5" />
                      LinkedIn
                    </a>
                  </Show>
                </div>
              )}
            </For>
          </div>

          {/* The people who are NOT the team, kept one click away so this page
              does not imply the two founders did all of it alone. */}
          <div class="mx-auto mt-16 max-w-3xl rounded-2xl border border-noviq-line bg-noviq-panel/50 px-6 py-8 text-center">
            <h2 class="text-lg font-semibold tracking-tight text-noviq-ink">
              With technical and institutional support
            </h2>
            <p class="mx-auto mt-2 max-w-xl text-[14px] leading-relaxed text-noviq-700">
              Contributors across government, academia and industry helped build and evaluate
              Noviq, and are acknowledged by name.
            </p>
            <button
              type="button"
              onClick={() => goLanding("acknowledgements")}
              class="mt-5 inline-flex items-center gap-1.5 text-[13px] font-semibold text-noviq-800 hover:text-noviq-900"
            >
              See the contributors
              <svg class="h-3.5 w-3.5" viewBox="0 0 24 24" fill="none" stroke="currentColor"
                   stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round">
                <path d="M5 12h14M13 6l6 6-6 6" />
              </svg>
            </button>
          </div>
        </div>
      </Show>

      {/* ── Demo — its own screen ──────────────────────────────────────── */}
      <Show when={view() === "demo"}>
        <div class="mx-auto flex min-h-[78vh] max-w-xl flex-col justify-center px-6 py-16 text-center">
          <h1 class="text-3xl font-bold tracking-tight text-noviq-ink sm:text-4xl">
            Try Noviq on public law.
          </h1>
          <p class="mt-4 text-base leading-relaxed text-noviq-700">
            Sign in with Google and ask questions against a small set of published
            instruments, so you can see how the answers and their citations behave
            before talking to us.
          </p>
          <ul class="mx-auto mt-8 w-full max-w-sm space-y-2.5 text-left">
            <For each={DEMO_TERMS}>
              {(t) => (
                <li class="flex gap-2.5 text-[13px] leading-relaxed text-noviq-800">
                  <svg class="mt-1 h-3.5 w-3.5 flex-shrink-0 text-noviq-600" viewBox="0 0 24 24"
                       fill="none" stroke="currentColor" stroke-width="2.6"
                       stroke-linecap="round" stroke-linejoin="round">
                    <path d="M4 12.5 9 17.5 20 6.5" />
                  </svg>
                  {t}
                </li>
              )}
            </For>
          </ul>

          {/* The real thing. No sign-in state is set here — `loginWithGoogle`
              stores the session exactly as password login does, and the app's
              auth gate takes over the moment `currentUser` is set, which is why
              there is nothing to navigate to on success. */}
          <div class="mt-9">
            <GoogleSignInButton />
          </div>
          <p class="mt-4 text-[12px] leading-relaxed text-noviq-600">
            By signing in you agree to our{" "}
            <button type="button" onClick={() => showPage("terms")}
                    class="font-medium text-noviq-800 underline">
              terms
            </button>{" "}
            and{" "}
            <button type="button" onClick={() => showPage("privacy")}
                    class="font-medium text-noviq-800 underline">
              privacy policy
            </button>
            . Prefer to talk to us?{" "}
            <button type="button" onClick={() => goLanding("quote")}
                    class="font-medium text-noviq-800 underline">
              Request a quote
            </button>
            .
          </p>
        </div>
      </Show>

      <Show when={view() === "landing"}>
      {/* ── Hero ─────────────────────────────────────────────────────── */}
      {/* Height is capped below 100vh on purpose: the old full-bleed hero made
          the page look like it ended at the fold, and visitors never discovered
          the sections underneath. The next section now peeks above the fold. */}
      {/* One continuous green belt across the full width; the sign-in sits on it
          as a light card rather than occupying its own white half. */}
      <section class="relative overflow-hidden bg-gradient-to-br from-noviq-900 via-noviq-800 to-noviq-700 text-noviq-50">
        <div
          aria-hidden="true"
          class="pointer-events-none absolute -right-28 -top-28 h-96 w-96 rounded-full bg-noviq-500/20 blur-3xl"
        />
        <div
          aria-hidden="true"
          class="pointer-events-none absolute -bottom-40 -left-24 h-[26rem] w-[26rem] rounded-full bg-noviq-400/10 blur-3xl"
        />

        {/* One centred column. The sign-in card used to sit beside this in a
            two-column grid; it is now its own screen (architect, 2026-08-25), so
            the statement gets the full width and the eye has one path down it. */}
        <div class="relative mx-auto flex max-w-4xl flex-col items-center px-6 py-20 text-center sm:px-10 lg:py-28">
          <div class="flex flex-col items-center">
            {/* No brand lockup here (architect, 2026-08-25): the logo lives in the
                navbar, and repeating it above the headline put two copies of the
                brand on one screen and pushed the statement down the page. */}
            <span class="inline-flex items-center rounded-full border border-white/20 bg-white/10 px-3 py-1 text-[11px] font-medium tracking-wide text-noviq-50">
              Private AI over your own documents
            </span>

            {/* ⚠️ The headline names the READER'S EXPOSURE, not the product
                (architect, 2026-08-25). An officer's real fear is not "search is
                slow" — it is being answerable, personally, for a rule they never
                found. Every rewrite of this line must keep the accountability,
                and must not overclaim: Noviq retrieves and cites, it does not
                decide, and it does not indemnify anyone. */}
            <h1 class="mt-6 text-4xl font-bold leading-[1.06] tracking-tight sm:text-5xl xl:text-[3.4rem]">
              The rule you missed
              <br class="hidden sm:block" /> is the one they will
              <br class="hidden sm:block" /> ask you about.
            </h1>

            <p class="mt-6 text-base leading-relaxed text-noviq-100/90 sm:text-lg">
              Inquiries, audit paras and appeals all turn on a provision somebody
              should have read. Noviq answers from your organisation's own rulebook
              and cites the document, the rule and the page — so what you sign, you
              can defend.
            </p>

            <p class="mt-5 text-sm leading-relaxed text-noviq-200/85">
              Runs on infrastructure you control. Built for any organisation that has
              to answer from its own rules, starting with the Government of Sindh.
            </p>

            {/* Primary action is the QUOTE, not sign-in: a first-time visitor has
                no credentials, and the sign-in card beside this is already the
                path for people who do. */}
            {/* One action only, centred under the statement (architect,
                2026-08-28): the demo button used to sit beside this and split
                the choice. The demo is still reachable from the nav link and
                /demo. */}
            <div class="mt-10 flex flex-col items-center gap-3 sm:flex-row sm:justify-center">
              <a
                href="#quote"
                class="inline-flex w-full items-center justify-center rounded-full bg-white px-7 py-3
                       text-sm font-semibold text-noviq-800 transition-colors hover:bg-noviq-50 sm:w-auto"
              >
                Request a quote
              </a>
            </div>
          </div>
        </div>
      </section>

      {/* ── Who it is for ────────────────────────────────────────────── */}
      {/* Five tracks, each led by the READER'S problem rather than by a feature.
          The order is deliberate: government is the market we are actually in,
          and policy last because it is the least developed of the five. Keep the
          claims inside what the product does today — retrieval with citations.
          Nothing here may imply legal advice, a decision, or a guarantee. */}
      <section id="who" class="border-t border-noviq-line bg-noviq-paper">
        <div class="mx-auto max-w-6xl px-6 py-20 sm:px-10">
          <div class="max-w-3xl">
            <div class="text-[11px] font-semibold uppercase tracking-[0.16em] text-noviq-500">
              Who it is for
            </div>
            <h2 class="mt-3 text-3xl font-bold leading-tight tracking-tight text-noviq-ink sm:text-4xl">
              Different work. The same problem.
            </h2>
            <p class="mt-4 text-base leading-relaxed text-noviq-700">
              The answer is already written down somewhere in your organisation. Noviq
              finds it, quotes it, and tells you which document and page it came from.
            </p>
          </div>

          <div class="mt-12 grid gap-5 sm:grid-cols-2 lg:grid-cols-3">
            <For each={AUDIENCES}>
              {(a) => (
                <div class="flex flex-col rounded-2xl border border-noviq-line bg-white p-6">
                  <span class="flex h-11 w-11 items-center justify-center rounded-full bg-noviq-100 text-noviq-700">
                    <svg class="h-[22px] w-[22px]" viewBox="0 0 24 24" fill="none" stroke="currentColor"
                         stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round">
                      <path d={a.icon} />
                    </svg>
                  </span>
                  <h3 class="mt-4 text-lg font-semibold tracking-tight text-noviq-ink">{a.title}</h3>
                  {/* The fear, in their words. */}
                  <p class="mt-2 text-[13px] font-medium leading-snug text-noviq-800">{a.fear}</p>
                  <p class="mt-3 flex-1 text-[13px] leading-relaxed text-noviq-700">{a.body}</p>
                  <a
                    href="#quote"
                    onClick={() => setQuoteSector(a.id)}
                    class="mt-5 inline-flex items-center gap-1.5 text-[13px] font-semibold text-noviq-800 hover:text-noviq-900"
                  >
                    Request a quote
                    <svg class="h-3.5 w-3.5" viewBox="0 0 24 24" fill="none" stroke="currentColor"
                         stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round">
                      <path d="M5 12h14M13 6l6 6-6 6" />
                    </svg>
                  </a>
                </div>
              )}
            </For>
          </div>
        </div>
      </section>

      {/* ── Request a quote ──────────────────────────────────────────── */}
      <section id="quote" class="border-t border-noviq-line bg-noviq-panel/50">
        <div class="mx-auto grid max-w-6xl gap-12 px-6 py-20 sm:px-10 lg:grid-cols-[0.9fr_1.1fr] lg:gap-16">
          <div>
            <div class="text-[11px] font-semibold uppercase tracking-[0.16em] text-noviq-500">
              Request a quote
            </div>
            <h2 class="mt-3 text-3xl font-bold leading-tight tracking-tight text-noviq-ink sm:text-4xl">
              Tell us what your people have to answer.
            </h2>
            <p class="mt-4 text-base leading-relaxed text-noviq-700">
              Noviq is deployed on infrastructure your organisation controls, priced as a
              licence with annual support. Tell us the scale and the material, and we will
              come back with what it would take.
            </p>
            <ul class="mt-8 space-y-3.5">
              <For each={QUOTE_POINTS}>
                {(t) => (
                  <li class="flex gap-3 text-[14px] leading-relaxed text-noviq-800">
                    <svg class="mt-1 h-4 w-4 flex-shrink-0 text-noviq-600" viewBox="0 0 24 24" fill="none"
                         stroke="currentColor" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round">
                      <path d="M4 12.5 9 17.5 20 6.5" />
                    </svg>
                    {t}
                  </li>
                )}
              </For>
            </ul>
            <p class="mt-8 text-[13px] leading-relaxed text-noviq-600">
              Or write to{" "}
              <a class="font-medium text-noviq-800 underline" href="mailto:admin@noviq.pk">
                admin@noviq.pk
              </a>
              .
            </p>
          </div>
          <QuoteRequestForm initialSector={quoteSector()} />
        </div>
      </section>

      {/* ── Acknowledgements ─────────────────────────────────────────── */}
      <section id="acknowledgements" class="border-t border-noviq-line bg-noviq-panel/50">
        <div class="mx-auto max-w-6xl px-6 py-20 sm:px-10">
          <div class="max-w-3xl">
            <div class="text-[11px] font-semibold uppercase tracking-[0.16em] text-noviq-500">
              Acknowledgements
            </div>
            <h2 class="mt-3 text-2xl font-bold tracking-tight text-noviq-ink sm:text-3xl">
              Technical contributors and institutional support
            </h2>
            {/* ⚠️ Wording deliberately DIVERGES from Annexure-E here. The annexure says
                development was carried out "under the supervision and guidance of the
                TMR Wing"; the founders confirmed (2026-08-21) that the Wing assisted at
                particular points rather than supervising throughout, so the public page
                must not claim more than that. Do not "restore" the annexure phrasing. */}
            <p class="mt-4 text-sm leading-relaxed text-noviq-800/80 sm:text-base">
              Noviq was built by its founding team, with assistance at various stages from
              the TMR Wing and with technical and academic input from experts in artificial
              intelligence, computing and emerging technologies. The contributions recorded
              below are acknowledged with appreciation.
            </p>
          </div>

          {/* Three paths, each a labelled band. The institutions inside a band keep
              exactly the card they had — only the grouping is new. A band with no
              institutions is dropped rather than rendered empty. */}
          <For each={CONTRIBUTOR_PATHS}>
            {(track) => {
              const groups = () => CONTRIBUTORS.filter((c) => c.path === track.id);
              return (
                <Show when={groups().length > 0}>
                  <div class="mt-14 first:mt-12">
                    <div class="flex items-baseline gap-3 border-b border-noviq-line pb-3">
                      <h3 class="text-lg font-semibold tracking-tight text-noviq-ink sm:text-xl">
                        {track.title}
                      </h3>
                      <span class="rounded-full bg-noviq-100 px-2 py-0.5 text-[11px] font-medium tabular-nums text-noviq-700">
                        {groups().reduce((n, g) => n + g.people.length, 0)}
                      </span>
                    </div>
                    <p class="mt-3 max-w-3xl text-[13px] leading-relaxed text-noviq-700">
                      {track.blurb}
                    </p>

                    <div class="mt-6 grid gap-5 sm:grid-cols-2 lg:grid-cols-3">
                      <For each={groups()}>
              {(group) => (
                <div class="flex flex-col rounded-xl border border-noviq-line bg-white p-5 shadow-sm transition-shadow hover:shadow-md">
                  {/* The institution logo carries the identity; the text label below it
                      stays because several marks (Proxima, FAST) do not spell the name a
                      visitor would recognise. `alt` is empty because that label is the
                      accessible name — an alt here would read the institution twice. */}
                  <div class="flex items-start justify-between gap-3 border-b border-noviq-line pb-4">
                    <div class="min-w-0">
                      <div class="flex h-10 items-center">
                        <img
                          src={group.logo}
                          alt=""
                          loading="lazy"
                          class="max-h-10 max-w-[10.5rem] object-contain object-left"
                        />
                      </div>
                      <h3 class="mt-2.5 text-[11px] font-semibold uppercase tracking-[0.12em] text-noviq-600">
                        {group.institution}
                      </h3>
                    </div>
                    <span class="mt-0.5 shrink-0 rounded-full bg-noviq-100 px-2 py-0.5 text-[11px] font-medium tabular-nums text-noviq-700">
                      {group.people.length}
                    </span>
                  </div>

                  <ul class="mt-4 space-y-4">
                    <For each={group.people}>
                      {(p) => (
                        <li
                          class={
                            "flex items-start gap-3 " +
                            (p.featured
                              ? "-mx-1.5 rounded-lg border border-noviq-200 bg-noviq-50 px-2.5 py-2.5"
                              : "")
                          }
                        >
                          {/* Photo where the institution publishes one; otherwise an
                              initials monogram, so a missing headshot never leaves a
                              broken frame or a ragged row. */}
                          <Show
                            when={p.photo}
                            fallback={
                              <span
                                aria-hidden="true"
                                class="flex h-11 w-11 shrink-0 items-center justify-center rounded-full
                                       bg-noviq-100 text-[12px] font-semibold tracking-wide text-noviq-700"
                              >
                                {initialsOf(p.name)}
                              </span>
                            }
                          >
                            <img
                              src={p.photo}
                              alt={p.name}
                              loading="lazy"
                              class={
                                "shrink-0 rounded-full object-cover object-top " +
                                (p.featured
                                  ? "h-14 w-14 border-2 border-noviq-400 shadow-sm"
                                  : "h-11 w-11 border border-noviq-line")
                              }
                            />
                          </Show>

                          <div class="min-w-0">
                            <Show
                              when={p.linkedin}
                              fallback={
                                <div
                                  class={
                                    p.featured
                                      ? "text-[15px] font-bold tracking-tight text-noviq-900"
                                      : "text-sm text-noviq-ink"
                                  }
                                >
                                  {p.name}
                                </div>
                              }
                            >
                              <a
                                href={p.linkedin}
                                target="_blank"
                                rel="noopener noreferrer"
                                class={
                                  "group inline-flex items-center gap-1.5 hover:text-noviq-700 " +
                                  (p.featured
                                    ? "text-[15px] font-bold tracking-tight text-noviq-900"
                                    : "text-sm text-noviq-ink")
                                }
                              >
                                <span class="underline decoration-noviq-300 decoration-1 underline-offset-2 group-hover:decoration-noviq-600">
                                  {p.name}
                                </span>
                                <LinkedInMark class="h-3 w-3 shrink-0 text-noviq-500 opacity-70 group-hover:opacity-100" />
                              </a>
                            </Show>

                            <Show when={p.title}>
                              <div
                                class={
                                  "mt-0.5 leading-snug " +
                                  (p.featured
                                    ? "text-xs font-semibold text-noviq-700"
                                    : "text-xs text-noviq-600")
                                }
                              >
                                {p.title}
                              </div>
                            </Show>
                            <Show when={p.alsoAt}>
                              <div class="mt-0.5 text-xs leading-snug text-noviq-600">
                                {/* Neutral "Also ·" rather than "Also at" — the value is
                                    sometimes a plain organisation ("Securiti") and
                                    sometimes a role phrase ("Founder, Leaveit2AI"), and
                                    "Also at Founder, …" does not read. */}
                                <span class="text-noviq-500">Also ·</span> {p.alsoAt}
                              </div>
                            </Show>
                            <Show when={p.note}>
                              <div class="mt-1 text-xs leading-relaxed text-noviq-600/80">{p.note}</div>
                            </Show>
                          </div>
                        </li>
                      )}
                    </For>
                  </ul>
                </div>
              )}
                      </For>
                    </div>
                  </div>
                </Show>
              );
            }}
          </For>
        </div>
      </section>

      <footer class="border-t border-noviq-line bg-noviq-paper">
        <div class="mx-auto flex max-w-6xl flex-col gap-3 px-6 py-8 sm:flex-row sm:items-center sm:justify-between sm:px-10">
          <div class="flex items-center gap-2.5">
            <img src={markUrl} alt="" class="h-6 w-6 object-contain" />
            <span class="text-xs text-noviq-600/85">
              Noviq — private AI over your own documents
            </span>
          </div>
          <div class="flex flex-wrap items-center gap-x-4 gap-y-2 text-xs text-noviq-600/70">
            <span>With thanks to the TMR Wing and our academic contributors.</span>
            <button
              type="button"
              onClick={() => showPage("privacy")}
              class="text-noviq-700 underline hover:text-noviq-900"
            >
              Privacy
            </button>
            <button
              type="button"
              onClick={() => showPage("terms")}
              class="text-noviq-700 underline hover:text-noviq-900"
            >
              Terms
            </button>
          </div>
        </div>
      </footer>
      </Show>
    </div>
  );
};

export default LoginScreen;
