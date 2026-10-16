// Image target resolution (spec: Images & attachments). Pure: no DOM.
//
// `resolveImageTarget(src, notePath)` classifies a markdown image's target:
// - remote http(s) targets → { kind: "remote", src } — the renderer shows the
//   broken-image placeholder; they are never fetched (no-network invariant);
// - a leading `/` is vault-root-relative; otherwise the target resolves
//   relative to the containing note's folder;
// - dot segments normalise; a path that climbs out of the vault root is
//   refused as { kind: "escape" } (the Swift tandem: handler refuses too —
//   belt and braces);
// - otherwise → { kind: "vault", path } — the renderer uses
//   `tandem:<path>` (the Swift WKURLSchemeHandler serves the file if it
//   exists under the vault root; a missing file surfaces as the
//   broken-image placeholder with alt text).

const REMOTE = /^https?:\/\//i;

export function resolveImageTarget(src, notePath) {
  if (REMOTE.test(src)) return { kind: "remote", src };

  let raw = src;
  let noteDir = "";
  if (raw.startsWith("/")) {
    raw = raw.slice(1); // vault-root-relative: the note's folder does not apply
  } else {
    noteDir = notePath.includes("/")
      ? notePath.slice(0, notePath.lastIndexOf("/") + 1)
      : "";
  }
  const segments = (noteDir + raw).split("/");
  const out = [];
  for (const seg of segments) {
    if (seg === "" || seg === ".") continue;
    if (seg === "..") {
      if (out.length === 0) return { kind: "escape" };
      out.pop();
    } else {
      out.push(seg);
    }
  }
  return { kind: "vault", path: out.join("/") };
}
