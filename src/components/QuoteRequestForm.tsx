/**
 * "Request a quote" — the public intake form on the landing page.
 *
 * ⚠️ Posts to `POST /api/quotes`, the ONLY unauthenticated write in this system,
 * so this component must never carry a session token and must never be reused
 * inside the authenticated app. It is deliberately a plain `fetch` rather than
 * the shared `call()` transport, which attaches the bearer token.
 *
 * The server is the authority on every rule expressed here — required fields,
 * lengths, the sector list, rate limits. The checks below exist to give a person
 * a fast answer, never to be the gate.
 */
import { Component, createSignal, For, Show } from "solid-js";

/** Must stay in lockstep with `SECTORS` in `api/quotes.rs`. */
const SECTORS = [
  { id: "government", label: "Government" },
  { id: "academia", label: "Academia" },
  { id: "industry", label: "Industry" },
  { id: "developers", label: "Developers" },
  { id: "policy", label: "Policy" },
];

const SIZES = ["1–20", "21–100", "101–500", "501–2,000", "2,000+"];

interface Props {
  /** Pre-selected track when the visitor arrives from an audience card. */
  initialSector?: string;
}

const QuoteRequestForm: Component<Props> = (props) => {
  const [organisation, setOrganisation] = createSignal("");
  const [contactName, setContactName] = createSignal("");
  const [email, setEmail] = createSignal("");
  const [phone, setPhone] = createSignal("");
  const [sector, setSector] = createSignal(props.initialSector ?? "government");
  const [orgSize, setOrgSize] = createSignal("");
  const [message, setMessage] = createSignal("");
  // Honeypot — hidden from people, filled by naive bots. Never shown, never
  // labelled, and excluded from the tab order.
  const [website, setWebsite] = createSignal("");

  const [busy, setBusy] = createSignal(false);
  const [sent, setSent] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);

  const inputClass =
    "w-full rounded-lg border border-noviq-line bg-white px-3.5 py-2.5 text-sm text-noviq-ink " +
    "placeholder:text-noviq-400 focus:border-noviq-500 focus:outline-none focus:ring-2 " +
    "focus:ring-noviq-500/20 disabled:opacity-50 transition-colors";
  const labelClass = "mb-1.5 block text-[12px] font-medium text-noviq-700";

  const submit = async (e: Event) => {
    e.preventDefault();
    if (busy()) return;
    setError(null);

    if (!organisation().trim()) return setError("Please tell us which organisation you are from.");
    if (!email().trim()) return setError("Please give us an email address to reply to.");

    setBusy(true);
    try {
      const res = await fetch("/api/quotes", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          organisation: organisation().trim(),
          contact_name: contactName().trim() || null,
          email: email().trim(),
          phone: phone().trim() || null,
          sector: sector(),
          org_size: orgSize() || null,
          message: message().trim() || null,
          website: website(),
        }),
      });
      if (!res.ok) {
        // The server's message is written for a person (including the rate-limit
        // one, which tells them to email us instead), so show it as-is.
        let msg = "Something went wrong. Please email admin@noviq.pk instead.";
        try {
          const body = await res.json();
          // ⚠️ The API's shape is `{ error: { code, message } }` — a nested
          // OBJECT, not a string. Reading `body.error` as a string silently fell
          // through to the generic fallback, which would have hidden the
          // rate-limit message telling people to email us instead. Verified
          // against the live endpoint, 2026-08-25.
          const nested = body?.error?.message;
          if (typeof nested === "string" && nested) msg = nested;
          else if (typeof body?.message === "string" && body.message) msg = body.message;
          else if (typeof body?.error === "string" && body.error) msg = body.error;
        } catch {
          /* non-JSON error body — keep the fallback */
        }
        setError(msg);
        return;
      }
      setSent(true);
    } catch {
      setError("We could not reach the server. Please email admin@noviq.pk instead.");
    } finally {
      setBusy(false);
    }
  };

  return (
    <Show
      when={!sent()}
      fallback={
        <div class="rounded-2xl border border-noviq-200 bg-noviq-50 p-8 text-center">
          <div class="mx-auto flex h-12 w-12 items-center justify-center rounded-full bg-noviq-700 text-white">
            <svg class="h-6 w-6" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2"
                 stroke-linecap="round" stroke-linejoin="round">
              <path d="M4 12.5 9 17.5 20 6.5" />
            </svg>
          </div>
          <h3 class="mt-4 text-lg font-semibold text-noviq-ink">Thank you — we have it.</h3>
          <p class="mx-auto mt-2 max-w-md text-sm leading-relaxed text-noviq-700">
            Your request is with us and we will reply to{" "}
            <span class="font-medium text-noviq-ink">{email()}</span>. If it is urgent, write to{" "}
            <a class="font-medium text-noviq-800 underline" href="mailto:admin@noviq.pk">
              admin@noviq.pk
            </a>
            .
          </p>
        </div>
      }
    >
      <form onSubmit={submit} class="rounded-2xl border border-noviq-line bg-white p-6 sm:p-8">
        <div class="grid gap-4 sm:grid-cols-2">
          <div class="sm:col-span-2">
            <label class={labelClass} for="q-org">
              Organisation <span class="text-noviq-500">*</span>
            </label>
            <input
              id="q-org"
              class={inputClass}
              value={organisation()}
              onInput={(e) => setOrganisation(e.currentTarget.value)}
              placeholder="Department, university, company"
              disabled={busy()}
              required
            />
          </div>

          <div>
            <label class={labelClass} for="q-name">Your name</label>
            <input
              id="q-name"
              class={inputClass}
              value={contactName()}
              onInput={(e) => setContactName(e.currentTarget.value)}
              placeholder="Name and designation"
              disabled={busy()}
            />
          </div>

          <div>
            <label class={labelClass} for="q-email">
              Email <span class="text-noviq-500">*</span>
            </label>
            <input
              id="q-email"
              type="email"
              class={inputClass}
              value={email()}
              onInput={(e) => setEmail(e.currentTarget.value)}
              placeholder="you@organisation.gov.pk"
              disabled={busy()}
              required
            />
          </div>

          <div>
            <label class={labelClass} for="q-phone">Phone</label>
            <input
              id="q-phone"
              class={inputClass}
              value={phone()}
              onInput={(e) => setPhone(e.currentTarget.value)}
              placeholder="Optional"
              disabled={busy()}
            />
          </div>

          <div>
            <label class={labelClass} for="q-size">People who would use it</label>
            <select
              id="q-size"
              class={inputClass}
              value={orgSize()}
              onChange={(e) => setOrgSize(e.currentTarget.value)}
              disabled={busy()}
            >
              <option value="">Not sure yet</option>
              <For each={SIZES}>{(s) => <option value={s}>{s}</option>}</For>
            </select>
          </div>

          <div class="sm:col-span-2">
            <span class={labelClass}>You are</span>
            <div class="flex flex-wrap gap-2">
              <For each={SECTORS}>
                {(s) => (
                  <button
                    type="button"
                    onClick={() => setSector(s.id)}
                    disabled={busy()}
                    class="rounded-full border px-3.5 py-1.5 text-[13px] transition-colors"
                    classList={{
                      "border-noviq-700 bg-noviq-700 text-white": sector() === s.id,
                      "border-noviq-line bg-white text-noviq-700 hover:border-noviq-400":
                        sector() !== s.id,
                    }}
                  >
                    {s.label}
                  </button>
                )}
              </For>
            </div>
          </div>

          <div class="sm:col-span-2">
            <label class={labelClass} for="q-msg">What would you want it to answer?</label>
            <textarea
              id="q-msg"
              class={`${inputClass} min-h-[110px] resize-y`}
              value={message()}
              onInput={(e) => setMessage(e.currentTarget.value)}
              placeholder="The rules, records or files your people search today — and what goes wrong when they cannot find them."
              disabled={busy()}
            />
          </div>
        </div>

        {/* Honeypot. `aria-hidden` + `tabindex=-1` keep it away from people and
            from screen readers; it is not `display:none`, because some bots skip
            hidden inputs and the point is that they fill it in. */}
        <div class="absolute h-0 w-0 overflow-hidden opacity-0" aria-hidden="true">
          <label>
            Website
            <input
              tabindex="-1"
              autocomplete="off"
              value={website()}
              onInput={(e) => setWebsite(e.currentTarget.value)}
            />
          </label>
        </div>

        <Show when={error()}>
          <p class="mt-4 rounded-lg border border-red-200 bg-red-50 px-3.5 py-2.5 text-[13px] text-red-700">
            {error()}
          </p>
        </Show>

        {/* Button alone — the reassurance line that used to sit beside it was
            removed (architect, 2026-08-25). */}
        <div class="mt-6">
          <button
            type="submit"
            disabled={busy()}
            class="w-full whitespace-nowrap rounded-lg bg-noviq-700 px-5 py-2.5 text-sm
                   font-semibold text-white transition-colors hover:bg-noviq-800 disabled:opacity-50 sm:w-auto"
          >
            {busy() ? "Sending…" : "Request a quote"}
          </button>
        </div>
      </form>
    </Show>
  );
};

export default QuoteRequestForm;
