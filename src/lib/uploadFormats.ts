// What the uploader will actually accept.
//
// ⚠️ Kept deliberately narrower than `services::parsers::FileFormat`, which also
// knows audio, video and code. Those parse, but nothing downstream turns them
// into answerable text today — there is no transcription step — so offering them
// in the picker promised a capability the product does not have. Advertise only
// what indexes into something retrievable. The server enforces the same list in
// `api/uploads.rs`; this is the picker-side half so unsupported files are hard
// to pick in the first place rather than rejected after a slow upload.

/** `accept` attribute for every upload input. */
export const UPLOAD_ACCEPT =
  ".pdf,.docx,.xlsx,.xlsm,.pptx,.txt,.md,.markdown,.png,.jpg,.jpeg,.gif,.webp,.bmp";

/** Human-readable version of the same list, for hint text under a drop zone. */
export const UPLOAD_HINT = "PDF, Word, Excel, PowerPoint, text and images";
