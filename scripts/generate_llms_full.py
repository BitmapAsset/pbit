#!/usr/bin/env python3
"""Rebuild the model-readable docs bundle from its source pages (Python 3.9+)."""
import argparse
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SOURCES = ("README.md", "docs/persona.md", "docs/agents.md", "REPRODUCE.md")


def render():
    text = "# probbit: full documentation for language models\n\n"
    text += ("> The README, docs/persona.md, docs/agents.md and REPRODUCE.md of "
             "https://github.com/BitmapAsset/probbit in one file. Index: llms.txt. "
             "Generated from those four files; regenerate it whenever one of them changes.\n")
    for name in SOURCES:
        text += ("\n\n\n---\n\n# FILE: " + name + "\n\nSource: "
                 "https://github.com/BitmapAsset/probbit/blob/main/" + name + "\n\n")
        text += (ROOT / name).read_text(encoding="utf-8").rstrip() + "\n"
    return text


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true", help="fail if llms-full.txt is stale")
    args = parser.parse_args()
    target = ROOT / "llms-full.txt"
    expected = render()
    if args.check:
        if not target.exists() or target.read_text(encoding="utf-8") != expected:
            parser.exit(1, "llms-full.txt is stale; run python3 scripts/generate_llms_full.py\n")
        print("llms-full.txt matches its four source pages")
    else:
        target.write_text(expected, encoding="utf-8")
        print("regenerated llms-full.txt from four source pages")


if __name__ == "__main__":
    main()
