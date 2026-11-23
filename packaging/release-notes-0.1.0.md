The first release of **OutMark**: turn PDFs into clean Markdown, and Markdown into polished PDFs, without your files ever leaving your computer. No account, no upload, no telemetry.

## Download

| System | File |
| --- | --- |
| **Windows** (64-bit) | `OutMark-0.1.0-windows-x64.zip` |
| **macOS** (Apple Silicon and Intel) | `OutMark-0.1.0-macos-universal.zip` |

Unzip it and follow **`LEEME.txt`** (in Spanish). There is nothing to install.

The builds are not signed with a paid certificate, so Windows and macOS show a security warning the first time you open the app. `LEEME.txt` explains how to continue. `SHA256SUMS.txt` lets you verify the downloads.

## What's included

- **PDF → Markdown** for notes, articles, books and slide decks: headings, paragraphs, lists, code blocks, multi-column text, tables and images
- **Markdown → PDF** with the same typography on every computer
- **Batches**: a single queue, progress, cancel, retry and a copyable report
- Results go to your Downloads folder or one you choose, and existing files are never overwritten

The interface is in Spanish for now.

## Known limitations

- No OCR: scanned or handwritten PDFs have no text to extract
- Bold and italic are not kept when reading a PDF
- Merged cells, multi-line cells and tables across pages are not understood; tables on slides stay as text, with a warning
- Images are placed after the text of their page, not at their exact position, and some LaTeX figures are not found
- Multi-column text becomes a single column, because Markdown has no columns

## License

GNU AGPL-3.0. The full list of third-party licenses is in `THIRD-PARTY-LICENSES.html`, inside each download.
