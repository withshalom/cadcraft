#!/usr/bin/env python3
"""Turn a build or test log into GitHub annotations, so a failure can be read without the log.

    annotate.py error|notice TITLE LOGFILE

`error`: the compiler's errors and warnings (each `error`/`warning` block up to the next blank
line) and failed tests' output, as at most 10 error annotations. `notice`: the whole file as
notices of up to 4,000 characters each (at most 10).
"""
import re
import sys


def esc(s: str) -> str:
    return s.replace("%", "%25").replace("\r", "%0D").replace("\n", "%0A")


def chunks(text: str, size: int):
    out, cur = [], ""
    for line in text.splitlines(keepends=True):
        if len(cur) + len(line) > size and cur:
            out.append(cur)
            cur = ""
        cur += line
    if cur:
        out.append(cur)
    return out


def main() -> None:
    kind, title, path = sys.argv[1], sys.argv[2], sys.argv[3]
    text = open(path, encoding="utf-8", errors="replace").read()
    if kind == "notice":
        for i, c in enumerate(chunks(text, 4000)[:10]):
            print(f"::notice title={title} ({i + 1})::{esc(c)}")
        return
    blocks = []
    for m in re.finditer(r"^(error|warning)(\[[A-Z0-9]+\])?:.*?(?=\n\s*\n|\Z)", text, re.S | re.M):
        blocks.append(m.group(0))
    for m in re.finditer(r"^---- .*? stdout ----\n.*?(?=\n\n|\Z)", text, re.S | re.M):
        blocks.append(m.group(0))
    if not blocks:
        blocks = [text[-4000:]]
    # The names of every failed test first, in one annotation: the details
    # below are capped at nine.
    failed = re.findall(r"^    (\S+)$", text.split("failures:")[-1], re.M) if "failures:" in text else []
    if failed:
        print(f"::error title={title} (failed tests)::{esc(chr(10).join(failed)[:4000])}")
    for b in blocks[:9]:
        print(f"::error title={title}::{esc(b[:4000])}")


main()
