# my-pdf-tools

A local desktop app for working with PDF files. Everything runs on your own machine: no network, no telemetry, your documents never leave your computer.

> **Status: early development (pre-alpha).** The project is in its first stage and there is nothing to download yet. This README describes the goals and the planned scope.

## What it does (planned)

The first version (V1) focuses on converting between PDF and Markdown:

- **PDF → Markdown**, with two modes:
  - *Notes*: flowing text from digital PDFs (headings, lists, code, tables).
  - *Slides*: one section per slide, nested bullet lists, images for diagrams.
- **Markdown → PDF**, with consistent typography and the same output on every operating system.

Each conversion produces a short report with non-fatal warnings (for example, low-confidence tables), because PDF → Markdown is heuristic and lossy by nature.

The long-term vision is a local "all-in-one" PDF toolbox where new tools can be added without breaking the existing ones.

## Not in V1

OCR (scanned or handwritten PDFs), machine-learning models, other formats such as DOCX or HTML, auto-update, and a web or hosted version.

## Tech stack

- [Tauri 2](https://tauri.app/) for the desktop shell
- React + TypeScript (Vite) for the UI
- Rust for the conversion engine

The engine is built around a neutral intermediate document model, so each format needs one reader and one writer instead of a converter for every pair.

## Development

Requirements: [Rust](https://rustup.rs/) (stable), [Node.js](https://nodejs.org/) and the [Tauri prerequisites](https://tauri.app/start/prerequisites/) for your platform.

```bash
npm install
npm run tauri dev
```

To produce a release build:

```bash
npm run tauri build
```

## Platforms

Windows and macOS. Builds are currently unsigned, so the operating system will show a security warning on first launch.

## License

[AGPL-3.0](LICENSE). Contributions are welcome and are accepted under the same license.

> The `LICENSE` file will be added together with the first release of the code.
