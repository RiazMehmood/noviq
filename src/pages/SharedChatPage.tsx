// Feature 027 (US6) — a shared chat, read-only.
//
// ⚠️ There is deliberately NO composer on this page. A viewer cannot ask a
// question here, and that is not enforced by hiding a control: no route anywhere
// accepts a question for a conversation the caller does not own (FR-029). The
// absence of the capability is the enforcement.
//
// Citations open through `get_shared_chunk`, which authorises PER CHUNK — the
// viewer can read the passages this answer rests on and nothing else. It does
// not widen what they can see, so global search still returns none of the
// owner's project (ADR-0029).

import { createSignal, For, onMount, Show } from "solid-js";
import { useNavigate, useParams } from "@solidjs/router";
import { call } from "../api/transport";

interface SharedCitation {
  n?: number;
  instrument?: string;
  path?: string;
  chunk_id?: string;
  provenance?: string;
}

interface SharedTurn {
  question: string;
  answer: string;
  citations: SharedCitation[];
  created_at: number;
}

interface SharedChunk {
  chunk_id: string;
  content: string;
  file_name: string;
  unit_label: string | null;
  page_number: number | null;
}

export default function SharedChatPage() {
  const params = useParams();
  const navigate = useNavigate();
  const [turns, setTurns] = createSignal<SharedTurn[]>([]);
  const [error, setError] = createSignal<string | null>(null);
  const [passage, setPassage] = createSignal<SharedChunk | null>(null);
  const [forking, setForking] = createSignal(false);

  onMount(async () => {
    try {
      const res = await call<{ turns: SharedTurn[] }>("get_shared_chat", {
        token: params.token,
      });
      setTurns(res.turns);
    } catch (e) {
      // Signed out, revoked, or the feature is off — all indistinguishable on
      // purpose, so the message must not speculate about which.
      setError(
        e instanceof Error
          ? e.message
          : "This link is not available. It may have been revoked, or you may need to sign in.",
      );
    }
  });

  async function openCitation(chunkId?: string) {
    if (!chunkId) return;
    try {
      setPassage(await call<SharedChunk>("get_shared_chunk", {
        token: params.token,
        chunk_id: chunkId,
      }));
    } catch (e) {
      setError(e instanceof Error ? e.message : "Could not open that passage.");
    }
  }

  async function onFork() {
    if (forking()) return;
    setForking(true);
    try {
      const res = await call<{ project_id: string; conversation_id: string }>(
        "fork_shared_chat",
        { token: params.token },
      );
      navigate(`/chat/${res.conversation_id}`);
    } catch (e) {
      setError(e instanceof Error ? e.message : "Could not take a copy.");
      setForking(false);
    }
  }

  return (
    <div class="mx-auto w-full max-w-3xl px-4 py-8">
      <Show when={error()}>
        <p class="rounded border border-red-300 bg-red-50 px-3 py-2 text-sm text-red-800">
          {error()}
        </p>
      </Show>

      <Show when={!error()}>
        <header class="mb-6 flex items-start justify-between gap-4">
          <div>
            <h1 class="text-xl font-semibold">Shared conversation</h1>
            <p class="mt-1 text-sm opacity-70">
              You are reading a copy someone shared with you. To continue this work, take your
              own copy.
            </p>
          </div>
          <button
            class="shrink-0 rounded bg-blue-600 px-4 py-2 text-sm text-white disabled:opacity-50"
            onClick={onFork}
            disabled={forking()}
          >
            {forking() ? "Copying…" : "Take a copy"}
          </button>
        </header>

        <ol class="space-y-6">
          <For each={turns()}>
            {(t) => (
              <li>
                <p class="font-medium">{t.question}</p>
                <p class="mt-2 whitespace-pre-wrap">{t.answer}</p>
                <Show when={t.citations.length > 0}>
                  <ul class="mt-3 flex flex-wrap gap-2">
                    <For each={t.citations}>
                      {(c) => (
                        <li>
                          <button
                            class="rounded border px-2 py-1 text-xs hover:bg-gray-50"
                            onClick={() => openCitation(c.chunk_id)}
                          >
                            {c.instrument ?? "Source"} {c.path ?? ""}
                            {/* FR-016 — never let an uploaded draft read as
                                enacted law. */}
                            <Show when={c.provenance === "corpus"}>
                              <span class="ml-1 opacity-60">· law</span>
                            </Show>
                            <Show when={c.provenance === "project"}>
                              <span class="ml-1 opacity-60">· project document</span>
                            </Show>
                          </button>
                        </li>
                      )}
                    </For>
                  </ul>
                </Show>
              </li>
            )}
          </For>
        </ol>

        <Show when={passage()}>
          {(p) => (
            <aside class="mt-8 rounded border bg-gray-50 p-4">
              <div class="mb-2 flex items-center justify-between">
                <span class="text-sm font-medium">
                  {p().file_name}
                  <Show when={p().unit_label}> · {p().unit_label}</Show>
                  <Show when={p().page_number}> · p.{p().page_number}</Show>
                </span>
                <button class="text-sm opacity-70" onClick={() => setPassage(null)}>
                  Close
                </button>
              </div>
              <p class="whitespace-pre-wrap text-sm">{p().content}</p>
            </aside>
          )}
        </Show>
      </Show>
    </div>
  );
}
