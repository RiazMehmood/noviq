/**
 * Confidential-detail masking (Feature 020, Phase 1) — client-side only.
 *
 * Detects structured PII (CNIC, phone, money, file/case no., email, date) in a
 * query with regex, lets the caller add/remove ranges, then swaps confidential
 * spans for reversible labels (`[NAME_1]`, `[CNIC_1]`) so the server/DB/logs only
 * ever see the labelled text. The label→value map lives ONLY in the browser
 * session (never sent to the server) and is used to re-fill real values into the
 * answer for on-screen display.
 *
 * Phase 2 adds name detection via a small server NER model behind /api/detect_pii;
 * the range/label/refill machinery here is reused unchanged.
 *
 * Deliberately does NOT match legal-structure tokens (Rule 16, Article 25, Section
 * 7) — masking those would wreck retrieval.
 */

export type PiiType = "cnic" | "phone" | "money" | "fileno" | "email" | "date" | "name" | "manual";

export interface Span {
  start: number; // inclusive char offset into the raw query
  end: number; // exclusive
  text: string; // the raw matched text
  type: PiiType;
  id: string; // stable id: `${type}:${start}` — used to toggle include/exclude
}

/** Human label + accent colour per category (used by the composer UI). */
export const PII_META: Record<PiiType, { label: string; token: string }> = {
  cnic: { label: "CNIC", token: "CNIC" },
  phone: { label: "Phone", token: "PHONE" },
  money: { label: "Amount", token: "AMOUNT" },
  fileno: { label: "File no.", token: "FILE" },
  email: { label: "Email", token: "EMAIL" },
  date: { label: "Date", token: "DATE" },
  name: { label: "Name", token: "NAME" },
  manual: { label: "Marked", token: "INFO" },
};

// ── Detectors ────────────────────────────────────────────────────────────────
// Ordered by priority — earlier wins when two patterns overlap (see mergeSpans).
const DETECTORS: { type: PiiType; re: RegExp }[] = [
  // Pakistani CNIC — with dashes (42101-1234567-1), spaces, or 13 plain digits.
  { type: "cnic", re: /\b\d{5}[-\s]?\d{7}[-\s]?\d\b/g },
  // Email
  { type: "email", re: /\b[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}\b/g },
  // Money — currency-prefixed (Rs 50,000 / PKR 5 lac) OR a number with a scale
  // word (5 lac, 2 million, 10 thousand, 3 crore) or trailing currency (50000 rupees).
  {
    type: "money",
    re: /(?:\bRs\.?|\bPKR)\s?\d[\d,]*(?:\.\d+)?(?:\s?(?:lac|lakh|thousand|hundred|million|billion|crore|arab)s?)?\b|\b\d[\d,]*(?:\.\d+)?\s?(?:lac|lakh|thousand|million|billion|crore|arab|rupees|pkr)s?\b/gi,
  },
  // File / case number: No. F.1(2)/2023, No.SO(A)/2021, F.12-3/2020
  { type: "fileno", re: /\bNo\.?\s?[A-Za-z.()\/-]*\d+(?:\([\dA-Za-z]+\))?[\/-]\d{2,4}\b|\bF\.?\s?\d+(?:[.-]\d+)?\([\dA-Za-z]+\)\/\d{2,4}\b/g },
  // Pakistani phone: 0300-1234567, 021-1234567, +92 300 1234567
  { type: "phone", re: /\b(?:\+92|0)\d{2,3}[-\s]?\d{7}\b/g },
  // Date: 12/03/2024, 1-2-24
  { type: "date", re: /\b\d{1,2}[-/]\d{1,2}[-/]\d{2,4}\b/g },
];

/**
 * Person-name detection (Phase 2a — client-side heuristic). A no-AVX VM CPU +
 * near-full GPU make a server NER model (GLiNER) a heavy lift, so this ships
 * automatic name masking now: runs of Title-Case words that aren't common
 * legal/government/geographic terms, plus single names after an honorific/trigger.
 * False positives are expected and handled by the officer's exclude control; a
 * model-based upgrade (Phase 2b) can raise recall later.
 */
// Capitalised words that are NOT names in this bureaucratic corpus.
const NAME_STOPLIST = new Set(
  (
    "government department ministry secretary secretariat authority commission board committee " +
    "office officer division section rule rules article act ordinance regulation regulations schedule " +
    "chapter part clause sindh punjab balochistan khyber pakhtunkhwa pakistan islamic republic federal " +
    "provincial national assembly senate court supreme high chief minister president governor cabinet " +
    "finance revenue education health police service services civil servant servants public procurement " +
    "general director deputy additional joint chairman member tribunal agency corporation bank limited " +
    "company project development planning works force request quotation quotations method methods contract " +
    "contracts tender bid pension grade scale annexure form dated subject reference notification order " +
    "letter memo circular the this that when where what how can may shall will under per also section " +
    // currency tokens — never names (so "Rs"/"PKR" don't get masked as a person)
    "rs pkr rupees rupee " +
    // major Pakistani places (so "Karachi", "Lahore" aren't masked as names)
    "karachi lahore islamabad rawalpindi peshawar quetta multan faisalabad hyderabad sukkur larkana " +
    "gujranwala sialkot bahawalpur sargodha mardan nawabshah mirpur gilgit muzaffarabad thatta badin " +
    "local district tehsil taluka union council province provincial capital city town village " +
    // pure honorifics — they TRIGGER a name but are not part of it (mask "Bilal", not "Mr. Bilal")
    "mr mrs ms miss dr prof"
  ).split(/\s+/)
);

// Common English words — the BLOCKLIST that powers high-recall name detection:
// any capitalised word that is neither a common word nor a legal/govt/place term
// (NAME_STOPLIST) is treated as a probable name. Bounded lists beat an unbounded
// name whitelist. Covers sentence-starters, question/aux verbs, frequent verbs &
// nouns, and letter/office vocabulary.
const COMMON_WORDS = new Set(
  (
    "i we you he she it they me us him her them my our your his their its mine ours yours " +
    "a an the this that these those there here who whom whose which what when where why how " +
    "is are was were be been being am has have had do does did done will would shall should can could " +
    "may might must ought need needs dare used " +
    "and or but nor so yet if then else than as because while although though unless until since whether " +
    "of in on at to from by for with about under over above below between into through during before after " +
    "against within without upon per via across along around near off out up down " +
    "not no yes all any some none each every both either neither other another same such more most less least " +
    "few many much several enormous little lot lots number numbers " +
    "please kindly dear sir madam regarding attached enclosed thanks thank regards yours respectfully hereby " +
    "whereas herein hereof thereof therefore hence thus accordingly further furthermore moreover however " +
    "tell give show explain describe summarize summarise quote list define provide need want get make take " +
    "send submit apply applied request requested approve approved grant granted issue issued receive allow " +
    "require required process draft prepare help assist know let ask answer find check verify confirm ensure " +
    "consider follow following mention mentioned state stated note noted read write add remove update create " +
    "procedure procedures process rule rules law laws policy policies document documents file files application " +
    "applications form forms leave salary pension promotion transfer posting appointment employee employees case " +
    "cases matter matters subject reference letter order notice approval sanction budget payment amount amounts " +
    "date dates deadline deadlines time times question questions answer answers detail details information report " +
    "reports meeting decision decisions right rights duty duties step steps condition conditions requirement " +
    "requirements eligibility criteria criterion status stage stages level levels type types kind sort way ways " +
    "example examples point points term terms period days months years age minimum maximum total whole full part " +
    "parts general special various relevant respective concerned above said given new old current previous next " +
    "last first final initial main basic simple complete entire overall proper actual real true false correct wrong " +
    "good bad high low long short big small large great important necessary possible available applicable eligible"
  ).split(/\s+/)
);
// Honorifics / triggers that promote even a single following Title-Case word to a name.
const NAME_TRIGGERS = new Set(
  (
    "mr mrs ms miss dr prof syed malik sheikh mian begum hafiz qari maulana mufti " +
    "named applicant petitioner respondent complainant appellant"
  ).split(" ")
);

// Common Pakistani first names + surnames — lets single and lowercase names
// ("shahid", "Yasir") be caught, which a Title-Case-run heuristic can't. Curated
// to avoid common-English-word collisions. Matched case-insensitively.
const NAME_GAZETTEER = new Set(
  (
    // male first names
    "muhammad mohammad ahmad ahmed ali hassan hasan hussain hussein bilal usman osman umar omar " +
    "hamza saad zain zayn fahad kamran imran salman adnan arslan faisal tariq asif aslam akhtar akram " +
    "anwar ayaz shahid yasir yaseen yasin waqar waqas nadeem naveed shakeel shoaib sohail junaid rizwan " +
    "rehman rahman hafeez habib jamil khalid mahmood mehmood masood javed pervez irfan iftikhar zubair " +
    "zeeshan danish haris haroon zohaib basit talha ammar owais uzair noman numan mudassar mubashir " +
    "shahzad shehryar sikandar taimoor asad murtaza raza abbas qasim mustafa shahbaz nawaz asim atif azhar " +
    "faizan rehan wasif ahsan hammad ibrahim ismail yousaf yusuf ayaan zohaib " +
    // female first names
    "ayesha aisha fatima khadija maryam mariam zainab hina saba amna aiman nadia rabia sadia farah iqra " +
    "mahnoor areeba komal saima nabila samina rukhsana shazia bushra mehwish maria sumaira uzma naila " +
    "laiba zoya anum madiha sidra fariha nimra areej hania " +
    // surnames
    "khan malik sheikh shaikh chaudhry chaudhary butt awan gondal shah syed qureshi siddiqui ansari memon " +
    "baloch bhatti dogar gujjar jatoi khokhar mughal sial abbasi durrani kakar mengal marri bugti jamali " +
    "soomro lodhi niazi tarar cheema sandhu bajwa warraich mirza kazmi naqvi rizvi zaidi hashmi farooqi"
  ).split(/\s+/)
);

const isTitleCase = (w: string) => /^[A-Z][a-zA-Z'.-]*$/.test(w) && w.length > 1 && w !== w.toUpperCase();
// Letters-only lowercase key, so "Mr." and "Government," match the stoplist.
const nameKey = (w: string) => w.toLowerCase().replace(/[^a-z]/g, "");
// A token that COULD be part of a person name — the run-builder predicate:
//  - a known name (any case, via the gazetteer), OR
//  - a capitalised word that is NOT a common English word and NOT a legal/govt/
//    place term.
// This is deliberately high-recall; `detectNames` then applies MODERATE
// corroboration (gazetteer / honorific-trigger / 2+ word run) before actually
// masking, so a lone unknown capitalised word is no longer masked.
const isNameToken = (w: string) => {
  const k = nameKey(w);
  if (!k) return false;
  if (NAME_GAZETTEER.has(k)) return true;
  return isTitleCase(w) && !NAME_STOPLIST.has(k) && !COMMON_WORDS.has(k);
};

export function detectNames(text: string): Span[] {
  const out: Span[] = [];
  // Tokenise into words with their offsets.
  const tokens: { text: string; start: number }[] = [];
  const re = /[A-Za-z][A-Za-z'.-]*/g;
  let m: RegExpExecArray | null;
  while ((m = re.exec(text)) !== null) tokens.push({ text: m[0], start: m.index });

  let i = 0;
  while (i < tokens.length) {
    if (!isNameToken(tokens[i].text)) {
      i++;
      continue;
    }
    // Extend a run of consecutive name tokens.
    let j = i;
    while (j < tokens.length && isNameToken(tokens[j].text)) j++;
    const run = tokens.slice(i, j);
    // MODERATE corroboration (loosened from the earlier "mask every capitalised
    // word"): only mask a run that is actually evidenced as a name —
    //   • it contains a known Pakistani name (gazetteer), OR
    //   • it is a run of 2+ capitalised non-common words (a likely full name), OR
    //   • the single word directly follows an honorific/trigger ("Mr.", "applicant").
    // A LONE, unknown capitalised word (a sentence-starter or a capitalised term we
    // don't recognise) is NOT auto-masked — the officer can mask it by hand.
    const hasKnownName = run.some((t) => NAME_GAZETTEER.has(nameKey(t.text)));
    const prev = i > 0 ? tokens[i - 1] : undefined;
    const triggered = !!prev && NAME_TRIGGERS.has(nameKey(prev.text));
    const isMultiWord = run.length >= 2;
    if (hasKnownName || triggered || isMultiWord) {
      const start = run[0].start;
      let end = run[run.length - 1].start + run[run.length - 1].text.length;
      while (end > start && !/[A-Za-z]/.test(text[end - 1])) end--; // trim trailing . , etc.
      out.push({ start, end, text: text.slice(start, end), type: "name", id: `name:${start}` });
    }
    i = j;
  }
  return out;
}

/** Run all detectors over `text` and return non-overlapping spans, earliest first. */
export function detect(text: string): Span[] {
  const found: Span[] = [];
  for (const { type, re } of DETECTORS) {
    re.lastIndex = 0;
    let m: RegExpExecArray | null;
    while ((m = re.exec(text)) !== null) {
      if (m[0].length === 0) {
        re.lastIndex++;
        continue;
      }
      found.push({ start: m.index, end: m.index + m[0].length, text: m[0], type, id: `${type}:${m.index}` });
    }
  }
  // Names are lower priority — structured PII wins any (rare) overlap via mergeSpans.
  found.push(...detectNames(text));
  return mergeSpans(found);
}

/** Drop spans that overlap an already-accepted (earlier) span. On an equal start,
 * the LONGER span wins (e.g. money "Rs 5 lac" beats a stray name token "Rs"). */
export function mergeSpans(spans: Span[]): Span[] {
  const sorted = [...spans].sort((a, b) => a.start - b.start || b.end - a.end);
  const out: Span[] = [];
  let lastEnd = -1;
  for (const s of sorted) {
    if (s.start >= lastEnd) {
      out.push(s);
      lastEnd = s.end;
    }
  }
  return out;
}

/** A confidential value that was masked in a turn (for the confirmation UI). */
export interface MaskedItem {
  value: string;
  type: PiiType;
}

/** Distinct masked values from a set of ranges (used to show the officer what was masked). */
export function maskedItemsFromRanges(ranges: Span[]): MaskedItem[] {
  const seen = new Set<string>();
  const out: MaskedItem[] = [];
  for (const r of ranges) {
    if (seen.has(r.text)) continue;
    seen.add(r.text);
    out.push({ value: r.text, type: r.type });
  }
  return out;
}

/** Split `text` into plain / highlighted segments around every masked value —
 * used to render the officer's own query bubble with the masked bits marked. */
export function highlightSegments(text: string, values: string[]): { text: string; masked: boolean }[] {
  const terms = values.filter(Boolean).sort((a, b) => b.length - a.length);
  if (terms.length === 0) return [{ text, masked: false }];
  const segs: { text: string; masked: boolean }[] = [];
  let i = 0;
  outer: while (i < text.length) {
    for (const t of terms) {
      if (text.startsWith(t, i)) {
        segs.push({ text: t, masked: true });
        i += t.length;
        continue outer;
      }
    }
    // accumulate plain chars until the next match
    const last = segs[segs.length - 1];
    if (last && !last.masked) last.text += text[i];
    else segs.push({ text: text[i], masked: false });
    i++;
  }
  return segs;
}

const MARK_STYLE = "background:#dcfce7;border-radius:3px;padding:0 2px;box-decoration-break:clone";

/** Wrap every occurrence of a masked value in an inline `<mark>` (survives
 * marked+DOMPurify) so refilled values are visibly flagged in the answer. */
export function highlightMarkdown(text: string, values: string[]): string {
  let out = text;
  for (const v of [...values].filter(Boolean).sort((a, b) => b.length - a.length)) {
    // escape regex metachars in the value
    const esc = v.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
    out = out.replace(new RegExp(esc, "g"), `<mark style="${MARK_STYLE}">${v}</mark>`);
  }
  return out;
}

// ── Reversible label session ─────────────────────────────────────────────────
/**
 * Holds the label↔value map for one conversation. Same value → same label (so the
 * model can reason about "the same person"). Serialisable to sessionStorage; the
 * map is NEVER sent to the server.
 */
export class MaskSession {
  private valueToLabel = new Map<string, string>();
  private labelToValue = new Map<string, string>();
  private counters: Partial<Record<PiiType, number>> = {};

  /** Get (or mint) the label for a raw value of a given type. */
  labelFor(value: string, type: PiiType): string {
    const existing = this.valueToLabel.get(value);
    if (existing) return existing;
    const token = PII_META[type].token;
    const n = (this.counters[type] = (this.counters[type] ?? 0) + 1);
    const label = `[${token}_${n}]`;
    this.valueToLabel.set(value, label);
    this.labelToValue.set(label, value);
    return label;
  }

  /**
   * Replace `ranges` in `raw` with labels, right-to-left so offsets stay valid.
   * Returns the labelled text to send to the server.
   */
  mask(raw: string, ranges: Span[]): string {
    const sorted = [...ranges].sort((a, b) => b.start - a.start);
    let out = raw;
    for (const r of sorted) {
      const value = raw.slice(r.start, r.end);
      const label = this.labelFor(value, r.type);
      out = out.slice(0, r.start) + label + out.slice(r.end);
    }
    return out;
  }

  /** Swap labels back to real values for on-screen display. */
  refill(text: string): string {
    if (!text || this.labelToValue.size === 0) return text;
    let out = text;
    for (const [label, value] of this.labelToValue) {
      out = out.split(label).join(value);
    }
    return out;
  }

  get size(): number {
    return this.labelToValue.size;
  }

  toJSON(): string {
    return JSON.stringify({ v: Array.from(this.valueToLabel.entries()), c: this.counters });
  }

  static fromJSON(s: string | null): MaskSession {
    const sess = new MaskSession();
    if (!s) return sess;
    try {
      const { v, c } = JSON.parse(s) as { v: [string, string][]; c: Partial<Record<PiiType, number>> };
      for (const [value, label] of v) {
        sess.valueToLabel.set(value, label);
        sess.labelToValue.set(label, value);
      }
      sess.counters = c ?? {};
    } catch {
      /* corrupt session blob — start fresh */
    }
    return sess;
  }
}
