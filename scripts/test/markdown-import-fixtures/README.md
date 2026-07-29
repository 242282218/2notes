# Markdown Import Fixtures

## Purpose

This directory contains deterministic Markdown inputs for the third-stage import acceptance and for future importer regression tests. The files are inputs only: importing them creates new entries and never modifies the fixture files.

## Fixture catalog

| File                       | Coverage                                                          | Expected result                                                                         |
| -------------------------- | ----------------------------------------------------------------- | --------------------------------------------------------------------------------------- |
| `01-structured-export.md`  | 2notes JSON frontmatter, H1 title, tags, status, provenance       | imports as a task named `Imported task`; original content is `Captured before import\n` |
| `02-yaml-and-wikilinks.md` | YAML frontmatter, headings, nested lists, marks, links, WikiLinks | imports as material named `Knowledge links`; both WikiLinks remain searchable text      |
| `03-html-warning.md`       | Unsupported HTML block                                            | imports successfully and reports an unsupported-HTML warning                            |
| `04-empty.md`              | Empty Markdown file                                               | imports as a valid entry containing one empty paragraph                                 |

## Manual verification

1. In **Settings → Markdown import**, choose this directory.
2. Confirm the preview lists four Markdown files and shows the warning for `03-html-warning.md`.
3. Commit the import. Verify four new entries are created, then inspect the expected title, metadata, original content, structured blocks and WikiLinks above.
4. Re-run the same import. Verify the report skips all four files and creates no duplicates.

The exact number of preview warnings can change if the parser gains additional safe-degradation warnings. The acceptance requirement is that unsupported content is visible to the user and does not execute or silently disappear.

## Scope boundary

These fixtures validate parser and import behavior. They are not the 1,000-file performance corpus and do not substitute for packaged Windows smoke evidence.
