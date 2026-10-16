// The editor bundle build (ticket 07; spec: Editor — Packaging).
//
// Regenerates the committed bundle under Sources/Tandem/Resources/Editor/:
//   index.html   — copied from Editor/index.html (one HTML page)
//   editor.js    — esbuild bundle of Editor/src/main.js (IIFE, minified)
//   style.css    — Editor/style.css + KaTeX's CSS folded in (fonts reduced
//                  to woff2), so the bundle stays one CSS file
//   fonts/       — the 20 KaTeX woff2 faces style.css references
//   LICENSES.md  — generated: every package that actually landed in the
//                  bundle, with its npm-declared license; the build FAILS
//                  if any non-MIT package lands in it (no GPL/AGPL ships)
//
// Reproducible by construction: the inputs are the pinned
// Editor/package.json + lockfile (npm ci) and checked-in sources; esbuild
// embeds no timestamps and the LICENSES.md generation sorts every list.
// `npm ci && npm run build` must regenerate the committed bytes exactly
// (verified: a second build leaves `git diff` clean).

import {
  cpSync,
  existsSync,
  mkdirSync,
  readFileSync,
  readdirSync,
  rmSync,
  statSync,
  writeFileSync,
} from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { build } from "esbuild";

const editorDir = path.dirname(fileURLToPath(import.meta.url));
const outDir = path.resolve(editorDir, "..", "Sources", "Tandem", "Resources", "Editor");
const nm = path.join(editorDir, "node_modules");

// 0. Clean output — the build is a total regeneration, never an overlay.
rmSync(outDir, { recursive: true, force: true });
mkdirSync(path.join(outDir, "fonts"), { recursive: true });

// 1. editor.js — the single JS bundle (self-mounting IIFE).
const buildResult = await build({
  entryPoints: [path.join(editorDir, "src", "main.js")],
  outfile: path.join(outDir, "editor.js"),
  bundle: true,
  format: "iife",
  minify: true,
  target: "es2022", // the macOS 27 WebKit floor; es2022 is comfortably under it
  charset: "utf8",
  legalComments: "none", // attribution lives in LICENSES.md, not the bytes
  metafile: true,
  logLevel: "silent",
});

// 2. style.css — hand-written Ligne CSS + KaTeX's CSS folded in.
//    KaTeX's font stacks ship woff2 + woff + ttf; WebKit has had woff2
//    since macOS 10.12, so only the woff2 faces are referenced and copied
//    (the woff/ttf variants would add ~0.8 MB for no gain — research note
//    §2). The relative `fonts/` URLs resolve from style.css's output
//    location (the two sit side by side in the bundle).
const katexCssRaw = readFileSync(path.join(nm, "katex", "dist", "katex.min.css"), "utf8");
const woff2OnlyRe =
  /url\(fonts\/([\w-]+)\.woff2\) format\("woff2"\),url\(fonts\/\1\.woff\) format\("woff"\),url\(fonts\/\1\.ttf\) format\("truetype"\)/g;
const katexCss = katexCssRaw.replace(woff2OnlyRe, (_m, f) => `url(fonts/${f}.woff2) format("woff2")`);
if (katexCss === katexCssRaw)
  throw new Error("KaTeX CSS font rewrite did not match — katex version drifted?");

const cssFontRefs = [...new Set([...katexCss.matchAll(/url\(fonts\/([\w.-]+)\)/g)].map((m) => m[1]))];
const woff2Files = readdirSync(path.join(nm, "katex", "dist", "fonts"))
  .filter((f) => f.endsWith(".woff2"))
  .sort();
for (const f of woff2Files) cpSync(path.join(nm, "katex", "dist", "fonts", f), path.join(outDir, "fonts", f));
const cssMissing = cssFontRefs.filter((f) => !woff2Files.includes(f));
const unusedFonts = woff2Files.filter((f) => !cssFontRefs.includes(f));
if (cssMissing.length || unusedFonts.length)
  throw new Error(
    `KaTeX font set mismatch (css refs without file: ${cssMissing}; files never referenced: ${unusedFonts})`
  );

const baseCss = readFileSync(path.join(editorDir, "style.css"), "utf8");
writeFileSync(
  path.join(outDir, "style.css"),
  baseCss +
    "\n/* KaTeX 0.19.0 (MIT) — folded in by build.mjs so the bundle stays\n" +
    "   one CSS file. Font stacks reduced to woff2; the faces live in\n" +
    "   fonts/ next to this file. */\n" +
    katexCss +
    "\n"
);

// 3. index.html — copied verbatim.
cpSync(path.join(editorDir, "index.html"), path.join(outDir, "index.html"));

// 4. LICENSES.md — generated from the metafile: exactly the packages whose
//    code landed in editor.js, with their npm-declared license. Non-MIT is
//    a build failure (spec: nothing GPL/AGPL ships).
function packageNameOf(input) {
  const marker = "node_modules/";
  const i = input.lastIndexOf(marker);
  if (i === -1) return null; // our own Editor/src
  const parts = input.slice(i + marker.length).split("/");
  return parts[0].startsWith("@") ? parts.slice(0, 2).join("/") : parts[0];
}

const bundled = new Map(); // name -> {version, license}
for (const input of Object.keys(buildResult.metafile.inputs)) {
  const name = packageNameOf(input);
  if (!name || bundled.has(name)) continue;
  const pkgDir = path.join(nm, name);
  const pkg = JSON.parse(readFileSync(path.join(pkgDir, "package.json"), "utf8"));
  const license = typeof pkg.license === "string" ? pkg.license : pkg.license?.type;
  if (String(license).trim().toUpperCase() !== "MIT")
    throw new Error(`non-MIT package landed in the editor bundle: ${name} (${license})`);
  bundled.set(name, { version: pkg.version, license });
}
if (!bundled.has("katex")) throw new Error("katex missing from the bundle inputs");

function copyrightLine(pkgDir) {
  for (const f of ["LICENSE", "LICENSE.md", "LICENSE.txt"]) {
    const p = path.join(pkgDir, f);
    if (existsSync(p)) {
      const m = readFileSync(p, "utf8").match(/Copyright[^\n]+/i);
      if (m) return m[0];
    }
  }
  throw new Error(`no copyright line found under ${pkgDir}`);
}

const byCopyright = new Map(); // copyright line -> [package names]
for (const name of [...bundled.keys()].sort()) {
  const line = copyrightLine(path.join(nm, name));
  if (!byCopyright.has(line)) byCopyright.set(line, []);
  byCopyright.get(line).push(name);
}

const MIT_TEXT = `The MIT License (MIT)

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in
all copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN
THE SOFTWARE.`;

const pkgRows = [...bundled.entries()]
  .sort((a, b) => a[0].localeCompare(b[0]))
  .map(([name, v]) => `| \`${name}\` | ${v.version} |`)
  .join("\n");
const copyrightBlocks = [...byCopyright.entries()]
  .sort((a, b) => a[0].localeCompare(b[0]))
  .map(([line, names]) => `${line}\n\n${names.map((n) => `\`${n}\``).join(", ")}`)
  .join("\n\n");

writeFileSync(
  path.join(outDir, "LICENSES.md"),
  `# Third-party licenses — Tandem editor bundle

Generated by \`Editor/build.mjs\` — do not edit by hand. The committed
bundle in this directory contains, alongside Tandem's own \`Editor/src\`,
the code of the packages below (bundled into \`editor.js\` by esbuild) plus
KaTeX's stylesheet (folded into \`style.css\`) and KaTeX woff2 fonts
(\`fonts/\`). Versions are the pinned set from \`Editor/package.json\` +
lockfile — npm metadata is the source of truth (spec: Editor — Packaging).

Every bundled package is MIT-licensed; the build fails if a non-MIT
package ever lands in the bundle (spec: nothing GPL/AGPL ships).

| package | version |
| --- | --- |
${pkgRows}

## MIT license (every package above)

${MIT_TEXT}

Copyright holders:

${copyrightBlocks}
`
);

// 5. Manifest — file sizes for the record.
const size = (p) => statSync(p).size;
const files = [
  "index.html",
  "editor.js",
  "style.css",
  "LICENSES.md",
  ...woff2Files.map((f) => `fonts/${f}`),
].sort();
let total = 0;
console.log("Editor bundle — Sources/Tandem/Resources/Editor/");
for (const f of files) {
  const s = size(path.join(outDir, f));
  total += s;
  console.log(`  ${String(s).padStart(9)} B  ${f}`);
}
console.log(`  ${String(total).padStart(9)} B  total (${files.length} files)`);
