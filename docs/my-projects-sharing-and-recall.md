# My Projects — what sharing does, and what it cannot undo

**Status**: built on branch `027-my-projects`, **not deployed**. The feature is
behind an admin gate that is **off** by default, so nothing here is visible to
users until an administrator enables **My Projects** in the admin console.

Audience: testers, and whoever writes the user-facing copy. This records
behaviour that is deliberate, correct, and **opposite to what users will
expect** — the kind of thing that becomes a support incident if it is discovered
rather than disclosed.

---

## The limitation: a shared document cannot be recalled

When you share a project, the other person does **not** get access to your
project. They get **their own project** containing the same documents, and they
own it. From that moment the two are independent, permanently, in both
directions.

That independence is the point, and it has one consequence that cannot be
designed away:

> **Once you share a project, you can no longer remove those documents from the
> people you shared it with.**

Concretely:

| You do this | What happens to their copy |
|---|---|
| Delete a document from your project | Their copy **still answers from it** |
| Delete your entire project | Their copy is **untouched** |
| Rename or reorganise your project | They see **no change** |
| Correct or replace a document | They keep the **old version** — tell them out of band |
| Ask for the share to be "revoked" | **There is no such operation** for a project |

The same holds in reverse: nothing they do reaches your project either.

### Why it works this way

Sharing copies the *membership list*, not the files. That is what makes a shared
project searchable within seconds instead of after minutes of re-processing, and
what stops one person's edits from damaging another's work. The document itself
is stored once and referenced by both projects; it is deleted only when **no**
project anywhere still references it. So "delete" means "remove from my
project", never "destroy everywhere".

There is no technical remedy available inside this model. The mitigation is
disclosure: the share dialog states, at the moment of sharing, that the share
cannot be undone.

### What to tell users

**Share a project the way you would send a copy of a file by email, not the way
you would grant access to a folder.** If a document is sensitive enough that you
might later need to withdraw it, do not put it in a project you share.

---

## Chat sharing is different — and *is* revocable

Sharing a **chat** is not the same operation. A chat link gives a signed-in
Noviq user **read** access: they can read the transcript and open the citations
in it. They cannot ask new questions in it, and they cannot see the rest of your
project — not through search, not through anything else.

- A chat link **can** be revoked, and revoking it kills the link.
- ⚠️ But if someone took their own copy while the link was live, **that copy
  survives revocation** — because taking a copy is the project copy described
  above, with all the same consequences.
- Chat links are never reachable to someone who is not signed in to Noviq.

---

## Two smaller behaviours worth stating plainly

**A project starts empty.** It contains none of the shared legal corpus. A
question asked inside a project reaches that project's documents and nothing
else — not the corpus, and not your own documents in your other projects. If the
project has no documents, you are told there is nothing to search rather than
being given an answer from elsewhere.

**The corpus tick is per question, and does not stick.** Ticking "also search
the legal corpus" applies to that one question. The next question starts with it
off again. This is deliberate — it means you cannot leave a project
accidentally widened — but testers reliably read it as a bug, so it is worth
saying first.

**Storage**: receiving a shared project costs the recipient nothing against
their storage allowance, because no new bytes are stored.

---

## Related

- Requirement FR-024 in [`specs/027-my-projects/spec.md`](../specs/027-my-projects/spec.md)
- [ADR-0028 — Copy-on-Share and Cross-User Content Linking](../history/adr/0028-copy-on-share-and-cross-user-content-linking.md) — why copy rather than shared access, and the content-lifetime rule
- [ADR-0029 — Project Retrieval Isolation](../history/adr/0029-project-retrieval-isolation-via-a-subtractive-scoping-seam.md) — why a chat link cannot become project access
- ⚠️ Project documents are handled **exactly as ordinary uploads are today**,
  including with respect to third-party inference (ADR-0025). "My Projects" is a
  workspace boundary, not a new privacy boundary, and the UI says so where
  documents are added.
