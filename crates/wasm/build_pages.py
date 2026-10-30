#!/usr/bin/env python3
"""Regenerate the file:// pages from www/bitblock_wasm.wasm (the single source of wasm bytes).

  www/bitblock.html       launcher (no wasm)            <- pages/launcher.html (copied)
  www/play.html           play client (embedded wasm)   <- pages/play.template.html + pages/play_client.js
  www/bitblock-lab.html   dev/known-answer lab (embed)  <- re-embedded in place

Every embedding page must decode to exactly the sidecar bytes; this script asserts it.
"""
import base64, hashlib, re, shutil, sys
from pathlib import Path

here = Path(__file__).resolve().parent
www, pages = here / "www", here / "pages"
raw = (www / "bitblock_wasm.wasm").read_bytes()
b64 = base64.b64encode(raw).decode()
literal = " +\n".join('      "%s"' % b64[i:i + 112] for i in range(0, len(b64), 112))
EMBED = re.compile(r"(const WASM_B64 =\n)(.*?)(;\n)", re.S)

def embedded(html):
    m = EMBED.search(html)
    return base64.b64decode("".join(re.findall(r'"([A-Za-z0-9+/=]*)"', m.group(2))))

play = (pages / "play.template.html").read_text()
play = play.replace("__WASM_B64__", literal).replace("__PLAY_CLIENT_JS__", (pages / "play_client.js").read_text())
(www / "play.html").write_text(play)

lab = (www / "bitblock-lab.html").read_text()
m = EMBED.search(lab)
(www / "bitblock-lab.html").write_text(lab[:m.start(2)] + literal + lab[m.end(2):])

shutil.copyfile(pages / "launcher.html", www / "bitblock.html")

md5 = hashlib.md5(raw).hexdigest()
for name in ("play.html", "bitblock-lab.html"):
    got = embedded((www / name).read_text())
    if got != raw:
        sys.exit(f"{name}: embed != sidecar")
    print(f"{name}: embed == sidecar ({len(got)} bytes, md5 {hashlib.md5(got).hexdigest()})")
assert "WASM_B64" not in (www / "bitblock.html").read_text()
print(f"bitblock.html: launcher (no wasm); sidecar md5 {md5}")
