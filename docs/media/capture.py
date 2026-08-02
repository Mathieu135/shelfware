#!/usr/bin/env python3
"""Turn captured terminal output into the framed SVG screenshots in the README.

The captures must come from a real terminal, since shelfware drops its styling
when stdout is not a TTY. Regenerate both after changing the report:

    cargo build --release
    script -q /dev/null ./target/release/shelfware       > /tmp/report.ansi
    script -q /dev/null ./target/release/shelfware clean > /tmp/clean.ansi
    docs/media/capture.py /tmp/report.ansi shelfware         docs/media/report.svg
    docs/media/capture.py /tmp/clean.ansi  "shelfware clean" docs/media/clean.svg

On Linux `script` takes its arguments the other way round:
`script -qc ./target/release/shelfware /dev/null > /tmp/report.ansi`.

Only bold, dim and reset are understood, which is all the report emits.
"""
import re
import sys

COLS = 100
FS = 13.0           # font-size
# Upper bound on the advance of the monospace fallbacks in the stack (they run
# 0.60-0.66em). Both the frame and textLength use it, so a renderer that honours
# textLength lands exactly on the grid and one that ignores it still fits.
ADV = FS * 0.66
LH = FS * 1.55      # line height
PAD_X, PAD_TOP, PAD_BOT = 18.0, 44.0, 18.0

BG, CHROME = "#16181d", "#21242b"
FG, BOLD, DIM = "#cdd3de", "#ffffff", "#7d8590"
PROMPT = "#5ec98d"

# Presentation attributes rather than a <style> block: GitHub's SVG sanitizer
# drops embedded CSS, which would leave the whole capture unstyled.
ATTRS = {
    "fg": f'fill="{FG}"',
    "b": f'fill="{BOLD}" font-weight="600"',
    "d": f'fill="{DIM}"',
}

TOKEN = re.compile(r"\x1b\[([0-9;]*)m")


def wrap(text, cols):
    """Hard wrap like a terminal does, ignoring the zero-width escapes."""
    out, width, buf = [], 0, ""
    i = 0
    while i < len(text):
        m = TOKEN.match(text, i)
        if m:
            buf += m.group(0)
            i = m.end()
            continue
        if width == cols:
            out.append(buf)
            buf, width = "", 0
        buf += text[i]
        width += 1
        i += 1
    out.append(buf)
    return out


def esc(s):
    return s.replace("&", "&amp;").replace("<", "&lt;").replace(">", "&gt;")


def spans(line):
    """Split one line into (text, css-class) runs."""
    out, pos, cls = [], 0, "fg"
    for m in TOKEN.finditer(line):
        if m.start() > pos:
            out.append((line[pos:m.start()], cls))
        code = m.group(1)
        cls = {"1": "b", "2": "d"}.get(code, "fg")
        pos = m.end()
    if pos < len(line):
        out.append((line[pos:], cls))
    return out


def render(raw, command, out_path):
    # `script` prepends a literal "^D" plus backspaces; keep ESC (0x1b) only.
    raw = re.sub(r"[\x00-\x08\x0b-\x1a\x1c-\x1f\x7f]", "", raw)
    raw = re.sub(r"\A\^D", "", raw)
    lines = [f"\x00{command}"]
    for line in raw.strip("\n").split("\n"):
        lines.extend(wrap(line, COLS))

    width = PAD_X * 2 + COLS * ADV
    height = PAD_TOP + len(lines) * LH + PAD_BOT

    parts = [
        f'<svg xmlns="http://www.w3.org/2000/svg" width="{width:.0f}" height="{height:.0f}" '
        f'viewBox="0 0 {width:.0f} {height:.0f}" font-family="ui-monospace,SFMono-Regular,'
        f'Menlo,Consolas,monospace" font-size="{FS}">',
        f'<rect width="{width:.0f}" height="{height:.0f}" rx="10" fill="{BG}"/>',
        f'<path d="M0 10a10 10 0 0 1 10-10h{width - 20:.0f}a10 10 0 0 1 10 10v18H0z" fill="{CHROME}"/>',
        '<circle cx="19" cy="15" r="5" fill="#ec6a5f"/>',
        '<circle cx="37" cy="15" r="5" fill="#f4bf50"/>',
        '<circle cx="55" cy="15" r="5" fill="#61c554"/>',
    ]

    for n, line in enumerate(lines):
        y = PAD_TOP + n * LH
        # textLength pins the grid: whatever monospace font the viewer resolves,
        # the columns line up and nothing overflows the frame.
        cells = len(TOKEN.sub("", line).replace("\x00", "$ "))
        span = f' textLength="{cells * ADV:.2f}" lengthAdjust="spacingAndGlyphs"' if cells else ""
        parts.append(f'<text x="{PAD_X}" y="{y:.1f}" xml:space="preserve"{span}>')
        if line.startswith("\x00"):
            parts.append(
                f'<tspan fill="{PROMPT}">$ </tspan><tspan fill="{FG}">{esc(line[1:])}</tspan>'
            )
        else:
            for text, cls in spans(line):
                if text:
                    parts.append(f"<tspan {ATTRS[cls]}>{esc(text)}</tspan>")
        parts.append("</text>")

    parts.append("</svg>")
    with open(out_path, "w", encoding="utf-8") as fh:
        fh.write("\n".join(parts) + "\n")
    print(f"{out_path}  {len(lines)} lines  {width:.0f}x{height:.0f}")


if __name__ == "__main__":
    src, command, dest = sys.argv[1], sys.argv[2], sys.argv[3]
    with open(src, encoding="utf-8", errors="replace") as fh:
        render(fh.read(), command, dest)
