#!/usr/bin/env python3
"""Correct cargo-dist's npm registry instruction without changing other release notes."""
import argparse
import re
import sys


NPM_HEADING = "### Install prebuilt binaries into your npm project"
VERSION = r"[0-9]+\.[0-9]+\.[0-9]+(?:-[0-9A-Za-z.-]+)?"
NPM_BLOCK = re.compile(
    rf"(?m)^{re.escape(NPM_HEADING)}\r?\n\r?\n```sh\r?\n"
    rf"npm install holdmap@(?P<version>{VERSION})\r?\n```(?P<ending>\r?\n|$)"
)


def prepare_notes(notes: str, tag: str, repository: str, npm_published: bool = False) -> str:
    """Accept literal release identities and replace only the known generated npm block."""
    if not re.fullmatch(rf"v{VERSION}", tag):
        raise ValueError("release tag must be a literal vX.Y.Z version or prerelease")
    if not re.fullmatch(r"[A-Za-z0-9][A-Za-z0-9-]*/[A-Za-z0-9_.-]+", repository):
        raise ValueError("repository must be a literal GitHub owner/repository name")
    if repository.split("/", 1)[1] in (".", ".."):
        raise ValueError("repository name cannot be a path traversal component")
    if "\0" in notes:
        raise ValueError("release notes cannot contain NUL bytes")

    headings = re.findall(rf"(?m)^{re.escape(NPM_HEADING)}\r?$", notes)
    if not headings:
        return notes
    if len(headings) != 1:
        raise ValueError("release notes contain multiple generated npm sections")
    match = NPM_BLOCK.search(notes)
    if match is None:
        raise ValueError("unexpected cargo-dist npm block; review the generated notes")
    if match["version"] != tag[1:]:
        raise ValueError("npm instruction version differs from the release tag")
    if npm_published:
        return notes

    newline = "\r\n" if "\r\n" in match[0] else "\n"
    url = f"https://github.com/{repository}/releases/download/{tag}/holdmap-npm-package.tar.gz"
    replacement = newline.join([
        "### Download the npm package",
        "",
        "The npm package is attached to this release; it is not published to the npm registry.",
        "",
        f"[Download holdmap-npm-package.tar.gz]({url})",
    ]) + match["ending"]
    return notes[:match.start()] + replacement + notes[match.end():]


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--tag", required=True)
    parser.add_argument("--repo", required=True)
    parser.add_argument("--npm-registry-published", action="store_true",
                        help="retain the registry instruction only when npm publication is configured")
    args = parser.parse_args()
    try:
        notes = sys.stdin.buffer.read().decode("utf-8")
        prepared = prepare_notes(notes, args.tag, args.repo, args.npm_registry_published)
    except (ValueError, UnicodeError) as error:
        parser.error(str(error))
    sys.stdout.buffer.write(prepared.encode("utf-8"))


if __name__ == "__main__":
    main()
