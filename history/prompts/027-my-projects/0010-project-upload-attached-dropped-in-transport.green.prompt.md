---
id: 0010
title: Project upload reported "no document was indexed" because the transport dropped `attached`
stage: green
date: 2026-08-29
surface: agent
model: claude-opus-5
feature: 027-my-projects
branch: 028-noviq-pk-cutover
user: Riaz Mehmood
command: (chat) "still after creating new project and adding document it is showing this error ... why?"
labels: [bug, frontend, uploads, projects]
links:
  spec: specs/027-my-projects/spec.md
  ticket: n/a
  adr: n/a
  pr: n/a
files:
  - src/api/transport.ts
tests:
  - "npx tsc --noEmit: clean"
---

## Prompt

still after creating new project and adding document it is showing this error "Uploaded, but no document was indexed." why?

## Response snapshot

Traced the message to `src/components/AddProjectDocuments.tsx:79`, which fires when the
server returns no rejections AND `outcome.attached` is empty.

The server is correct: `src-tauri/src/api/uploads.rs:572-637` polls for the source rows
(up to 10s), writes the `project_source` membership, sets `sources.project_id`, and
returns the ids in `UploadOutcome.attached` (`uploads.rs:160`).

The loss was client-side. `uploadDocuments` in `src/api/transport.ts` parsed the JSON into
`Partial<UploadOutcome>` and then rebuilt a NEW object listing only two fields:

    return { folder: body.folder ?? null, rejected: body.rejected ?? [] };

`attached` was therefore always `undefined` at the call site, so a fully successful project
upload took the "nothing was indexed" branch. The same drop silently killed two other
features that read the response in `IndexedFolders.tsx:565,569` — the split-instrument
prompt (`multi_instrument`) and the near-duplicate warning (`similar_existing`).

Fix: copy the whole outcome through instead of whitelisting two fields.

## Outcome

- ✅ Impact: project uploads now confirm and close the dialog instead of reporting a false
  failure; the multi-instrument and near-duplicate notices come back to life.
- 🧪 Tests: `npx tsc --noEmit` clean. Not yet exercised against the live server — the VM was
  unreachable this session (VPN gateway down), so behavioural verification is outstanding.
- 📁 Files: `src/api/transport.ts` (one return statement + comment).
- 🔁 Next prompts: deploy the frontend and re-run a real project upload end to end.
- 🧠 Reflection: the previous fix in this area (2026-08-15) corrected the SERVER's timing —
  it waited for the rows — and the comment at `AddProjectDocuments.tsx:70-76` records an
  earlier client-side fix that stopped re-deriving ids from folder listings. Both were right.
  The remaining fault was one layer below either: a DTO mapper that had never been widened
  when the field was added, so the field existed in the type, in the server response and in
  the consumer, and was destroyed in transit.

## Evaluation notes (flywheel)

- Failure modes observed: a hand-written response mapper that whitelists fields silently
  drops every field added after it was written. The TypeScript type did not catch it because
  the discarded members are all optional.
- Graders run and results (PASS/FAIL): n/a
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): return `body` widened with defaults, or type the
  mapper so a new required field forces a compile error — several other transports in this
  file rebuild objects the same way and may hide the same class of bug.
