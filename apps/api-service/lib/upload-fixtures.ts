/**
 * P4 — Upload fixture registry and safe structured descriptors.
 *
 * PURE. No I/O — no fetch, no fs, no secrets, no timers. The API resolves a
 * registered fixture to a bounded descriptor and the extension performs the
 * structured FileList operation; no page script is generated here.
 *
 * FAIL-CLOSED: only paths registered in `UPLOAD_FIXTURES` may be uploaded. An
 * unknown path is rejected before any browser mutation — no bytes are ever
 * invented for an unregistered path.
 */

export interface UploadFixtureV01 {
  /** Original filename surfaced to the page (a fixture name, never a secret). */
  filename: string
  /** MIME type declared on the File object. */
  mimeType: string
  /** File bytes as base64 for the structured extension operation. */
  base64: string
}

/**
 * Registered fixture paths that may be uploaded. Key = the compiler-corpus data
 * value bound to the upload step (`{ file: 'resume.pdf' }`). Content is a tiny
 * static PDF shell — the fixture server accepts any multipart upload and the
 * P4 contract only requires that the REAL bytes travel to the page.
 */
const UPLOAD_FIXTURES: Record<string, UploadFixtureV01> = {
  'resume.pdf': {
    filename: 'resume.pdf',
    mimeType: 'application/pdf',
    base64:
      'JVBERi0xLjQKMSAwIG9iago8PCAvVHlwZSAvQ2F0YWxvZyAvUGFnZXMgMiAwIFIgPj4KZW5kb2JqCjIgMCBvYmoKPDwgL1R5cGUgL1BhZ2VzIC9LaWRzIFszIDAgUl0gL0NvdW50IDEgPj4KZW5kb2JqCjMgMCBvYmoKPDwgL1R5cGUgL1BhZ2UgL1BhcmVudCAyIDAgUiAvTWVkaWFCb3ggWzAgMCAyMDAgMjAwXSA+PgplbmRvYmoKeHJlZgowIDMKMDAwMDAwMDAwMCA2NTUzNSBmIAp0cmFpbGVyCjw8IC9TaXplIDMgL1Jvb3QgMSAwIFIgPj4Kc3RhcnR4cmVmCjAKJSVFT0YK',
  },
}

/** True when `path` is a registered upload fixture (fail-closed gate). */
export function isKnownUploadFixture(path: string): boolean {
  return Object.prototype.hasOwnProperty.call(UPLOAD_FIXTURES, String(path ?? ''))
}

/** Return a copy of a registered fixture descriptor, or null for unknown paths. */
export function getUploadFixture(path: string): UploadFixtureV01 | null {
  const fixture = UPLOAD_FIXTURES[String(path ?? '')]
  return fixture ? { ...fixture } : null
}
