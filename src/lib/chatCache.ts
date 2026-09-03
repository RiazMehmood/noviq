/**
 * localStorage keys for the chat transcript cache, and the purge that runs on
 * sign-out.
 *
 * Since migration 020 the server is the source of truth for chat history — it
 * is what makes a transcript follow the account onto a second device. These
 * local copies are now only a CACHE, kept for two reasons: the rail paints
 * instantly on load instead of waiting for a round trip, and the local copy of a
 * question retains the confidential values the officer actually typed (the
 * server only ever sees the masked form, and the label→value map never leaves
 * the browser — F020).
 *
 * Because the cache outlives a session, it MUST be purged on sign-out: on a
 * shared browser the next officer to sign in would otherwise be handed the
 * previous one's questions, with the real unmasked values in them.
 */

/** Ordered list of past conversations (most recent first). */
export const CONVERSATIONS_KEY = "noviq:conversations";

/** One conversation's cached transcript. */
export const turnsKey = (conversationId: string) => `noviq:turns:${conversationId}`;

/** Per-conversation confidential label→value map (sessionStorage, never sent). */
export const maskKey = (conversationId: string) => `noviq_mask_${conversationId}`;

/**
 * Drop every cached transcript and mask map for this browser. Called on logout.
 * Nothing is lost that the officer can't get back by signing in again — the
 * turns themselves live on the server — except the unmasked confidential values,
 * which are exactly what must not survive a sign-out.
 */
export function clearChatCache(): void {
  try {
    for (const key of Object.keys(localStorage)) {
      if (key === CONVERSATIONS_KEY || key.startsWith("noviq:turns:")) {
        localStorage.removeItem(key);
      }
    }
    for (const key of Object.keys(sessionStorage)) {
      if (key.startsWith("noviq_mask_")) sessionStorage.removeItem(key);
    }
  } catch {
    /* storage disabled or quota-locked — non-fatal, nothing to purge */
  }
}
