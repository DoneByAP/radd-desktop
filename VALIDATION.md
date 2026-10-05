# Validation record

## Version 0.2: beginner interface redesign

The interface now has a light canvas, contrasting navigation sidebar, numbered workflow, built-in sample button, separate review screen, beginner explanations, collapsed technical settings, and a results-reading guide. A failed run retains its failure state even when users navigate to another screen.

The app-internal `ui-capture` feature exercised the sample-selection and analysis callbacks with the real engine and rendered the actual GUI at 1220×880 and 940×720. Screens inspected: initial setup, selected sample, review, successful results, beginner guide, settings, missing engine, and unsupported input. This uses the app's own renderer and does not inject mouse or keyboard input into the desktop. Scrollbars are visible when content exceeds the window.

The production build excludes `ui-capture`. The existing unit and real-engine integration tests were rerun for the redesign. Native OS file dialogs and Linux/macOS runtime behavior were not re-tested in this pass.

For repeatable visual testing, compile with `--features ui-capture`; set `RADD_UI_CAPTURE_DIR` to a scratch folder and `RADD_TEST_ENGINE` to the engine path; run from the project directory (which contains samples/). Set `RADD_UI_SMALL=1` to exercise the smaller layout. The test writes screenshots and sample analysis output into the scratch folder and closes automatically. Test window persistence uses that same folder.

## Version 0.1: initial engine and GUI verification

Tested on Windows x86_64 on October 4, 2026 (America/Los_Angeles), with Rust 1.94.0.

## Passed

- GUI release build, using the checked-in Cargo.lock.
- Three unit tests: exact CLI arguments and paths with spaces/shell punctuation, input/config validation, and unique output directories.
- Real-engine integration test: capability check; assembly, JSON, FASTA and frequency JSON output from the upstream hello_x32 sample; parsing both JSON outputs; paths with spaces; cancellation; dropping the job (GUI shutdown path); and unsupported-input error reporting.
- Engine `--help` checked against every GUI command-line flag. In particular, the current flags are `--config-file` and `--fasta-prefix`.
- Engine `--arches` reports AArch64, ARM/Thumb, MIPS variants, PowerPC, RISC-V and x86 variants.
- Direct custom-header analysis of upstream thermostat.bin using thermostat.json produced assembly and JSON. The engine emitted alignment warnings, which are expected to remain visible in its log.
- Native Windows GUI launched. Main workflow layout and built-in Guide tab were visually inspected; controls appeared in the accessibility tree. Desktop automation was stopped by the user with Escape before a complete click-through analysis. No more desktop input was sent afterward.
- PowerShell build scripts parsed successfully; both Bash build scripts passed `bash -n`.
- Rust source formatted with the official Rust 1.94 formatter.

## Engine build details

- RADD 0.5.3, source `4fd4f59c9a442e6414c20ae3fd364a082e97f1ff`.
- Rapstone source `8f24eb77` and RHP source `96a1e132` (full revisions in scripts/engine.Cargo.lock).
- All default architecture features enabled.
- Build command: `cargo build --release --locked --bin radd --config profile.release.lto=false --config profile.release.package.rapstone.opt-level=0`.
- Rapstone optimization is lowered to avoid a very slow generated-decoder optimization step. This is a development bundle, not a performance benchmark build. The GUI is built with normal release optimization.

## Not verified here

- Linux/macOS compilation or runtime behavior. Build scripts and a GitHub Actions matrix are provided; those remote jobs have not run.
- Windows ARM, Linux ARM or Intel/Apple Silicon macOS distribution variants.
- Signed installers, macOS notarization, or clean-machine prerequisite checks.
- Exhaustive binary-format correctness or performance on very large inputs. This app relies on the upstream engine's analysis.
- Full graphical click-through analysis after the user stopped desktop automation. The underlying runner was exercised against the real engine through the integration test.

Checksums for the delivered archives and executables are in the accompanying SHA256SUMS.txt.
