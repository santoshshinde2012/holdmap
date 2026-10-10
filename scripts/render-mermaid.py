#!/usr/bin/env python3
"""Render the documentation's Mermaid sources into their white SVG previews."""

import argparse
from pathlib import Path
import re
import subprocess
import tempfile
import xml.etree.ElementTree as ET


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--mmdc", default="mmdc", help="Mermaid CLI 12.0.0 executable")
    parser.add_argument("--puppeteer-config", type=Path, help="Optional Chromium configuration")
    args = parser.parse_args()
    root = Path(__file__).resolve().parent.parent
    pattern = re.compile(
        r"!\[[^\n]*\]\(([^\n)]+\.svg)\)\s*"
        r"<details>\s*<summary>Mermaid source</summary>\s*"
        r"```mermaid\n(.*?)\n```\s*</details>",
        re.DOTALL,
    )
    count = 0
    with tempfile.TemporaryDirectory(prefix="holdmap-mermaid-") as directory:
        for document in (root / "README.md", root / "docs/architecture.md"):
            text = document.read_text()
            diagrams = pattern.findall(text)
            if len(diagrams) != text.count("```mermaid\n"):
                raise ValueError(f"{document}: Mermaid source is missing its SVG preview")
            for reference, source in diagrams:
                output = (document.parent / reference).resolve()
                if not output.is_relative_to(root / "docs/diagrams"):
                    raise ValueError(f"Unexpected diagram output: {output}")
                output.parent.mkdir(parents=True, exist_ok=True)
                input_file = Path(directory) / f"{output.stem}.mmd"
                svg_file = Path(directory) / output.name
                input_file.write_text(source + "\n")
                command = [
                    args.mmdc, "--input", str(input_file), "--output", str(svg_file),
                    "--svgId", output.stem, "--backgroundColor", "#ffffff",
                    "--no-font-embed", "--quiet",
                ]
                if args.puppeteer_config:
                    command += ["--puppeteerConfigFile", str(args.puppeteer_config.resolve())]
                subprocess.run(command, check=True)
                svg = svg_file.read_text()
                element = ET.fromstring(svg)
                if "background-color: white" not in element.get("style", "") and (
                    "background-color: rgb(255, 255, 255)" not in element.get("style", "")
                    and "background-color: #ffffff" not in element.get("style", "")
                ):
                    raise ValueError(f"{output.name}: SVG canvas is not explicitly white")
                output.write_text(svg + "\n")
                print(f"Rendered {output.relative_to(root)}")
                count += 1
    print(f"Rendered {count} Mermaid previews with white backgrounds")


if __name__ == "__main__":
    main()
